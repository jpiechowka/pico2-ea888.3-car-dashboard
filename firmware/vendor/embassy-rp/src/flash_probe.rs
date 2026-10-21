//! Fixed read-only RP2350 JEDEC transaction, shared with host protocol tests.
//! The caller must already own QMI in RAM with XIP exited and watchdog armed.
//! `None` leaves unknown hardware state: fail-stop in RAM, never return to XIP.

/// Direct-QMI clock divider 12 in DIRECT_CSR's CLKDIV field.
pub const TIMING: u32 = 12 << 22;
/// DIRECT_CSR enable bit; switches away from normal memory-mapped transactions.
pub const EN: u32 = 1;
/// DIRECT_CSR forced CS0 assertion bit, held across the JEDEC command/reply.
pub const CS0: u32 = 4;
/// DIRECT_CSR status bit indicating active direct transaction work.
pub const BUSY: u32 = 2;
/// DIRECT_CSR status bit indicating no room in the transmit FIFO.
pub const TXFULL: u32 = 1 << 10;
/// DIRECT_CSR status bit indicating an empty transmit FIFO.
pub const TXEMPTY: u32 = 1 << 11;
/// DIRECT_CSR status bit indicating no received byte is available.
pub const RXEMPTY: u32 = 1 << 16;

/// SRAM-only QMI direct-register boundary; implementors must not fetch from XIP
/// while direct mode owns the bus, including helper code and referenced constants.
pub trait Io {
    /// Writes raw DIRECT_CSR `value`, including timing/enable/chip-select controls.
    fn control(
        &mut self,
        value: u32,
    );
    /// Returns the current DIRECT_CSR status/control word without clearing FIFO data.
    fn status(&mut self) -> u32;
    /// Enqueues one direct-transfer `byte`; caller has checked that TX is not full.
    fn write(
        &mut self,
        byte: u8,
    );
    /// Returns and consumes one received byte; caller has checked RX is nonempty.
    fn read(&mut self) -> u8;
}

#[inline(always)]
/// Polls `io` until every bit in `clear` is zero, decrementing shared `polls`
/// before each read. Returns the qualifying status or None when budget exhausts;
/// zero budget performs no register read and one budget permits exactly one.
fn wait(
    io: &mut impl Io,
    polls: &mut u32,
    clear: u32,
) -> Option<u32> {
    loop {
        *polls = polls.checked_sub(1)?;
        let status = io.status();
        if status & clear == 0 {
            return Some(status);
        }
    }
}

#[inline(always)]
/// Reads JEDEC 0x9f through SRAM-safe `io` using at most `polls` total status polls.
/// Returns the three response bytes packed manufacturer-first into 24 bits after
/// idle/empty checks and explicit CS release/direct-mode disable. None leaves
/// unknown bus state: caller must fail-stop in RAM, never resume XIP or retry as
/// if cleanup succeeded. The poll budget bounds software polls, not a stalled bus.
pub fn read_id(
    io: &mut impl Io,
    mut polls: u32,
) -> Option<u32> {
    io.control(TIMING | EN);
    let initial = wait(io, &mut polls, BUSY)?;
    if initial & (TXEMPTY | RXEMPTY) != TXEMPTY | RXEMPTY {
        return None;
    }
    io.control(TIMING | EN | CS0);
    let mut id = 0;
    for index in 0..4 {
        wait(io, &mut polls, TXFULL)?;
        io.write(if index == 0 { 0x9F } else { 0 });
        wait(io, &mut polls, RXEMPTY)?;
        let byte = io.read();
        if index != 0 {
            id = (id << 8) | u32::from(byte);
        }
    }
    let final_status = wait(io, &mut polls, BUSY)?;
    if final_status & (TXEMPTY | RXEMPTY) != TXEMPTY | RXEMPTY {
        return None;
    }
    // Explicitly release CS0 as well as disabling direct mode, only after idle.
    io.control(TIMING);
    Some(id)
}
