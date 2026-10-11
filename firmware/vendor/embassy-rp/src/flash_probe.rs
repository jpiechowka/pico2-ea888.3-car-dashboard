//! Fixed read-only RP2350 JEDEC transaction, shared with host protocol tests.
//! The caller must already own QMI in RAM with XIP exited and watchdog armed.
//! `None` leaves unknown hardware state: fail-stop in RAM, never return to XIP.

pub const TIMING: u32 = 12 << 22;
pub const EN: u32 = 1;
pub const CS0: u32 = 4;
pub const BUSY: u32 = 2;
pub const TXFULL: u32 = 1 << 10;
pub const TXEMPTY: u32 = 1 << 11;
pub const RXEMPTY: u32 = 1 << 16;

pub trait Io {
    fn control(
        &mut self,
        value: u32,
    );
    fn status(&mut self) -> u32;
    fn write(
        &mut self,
        byte: u8,
    );
    fn read(&mut self) -> u8;
}

#[inline(always)]
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
