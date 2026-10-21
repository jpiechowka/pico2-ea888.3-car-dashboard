//! Host-testable admission for the RP2040 unique-ID assembly transaction.
//! Counter width: [manufacturer SSI register definitions](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2040/hardware_regs/include/hardware/regs/ssi.h).

/// Calls `read` with `out` only when its length fits SSI's 16-bit NDF counter after four dummy bytes.
/// Returns success without calling `read` for an empty output, `too_long` for excessive length,
/// otherwise the callback result. The callback remains responsible for chip support and XIP safety.
pub(crate) fn read_unique_id<E>(
    out: &mut [u8],
    too_long: E,
    read: impl FnOnce(&mut [u8]) -> Result<(), E>,
) -> Result<(), E> {
    if out.is_empty() {
        return Ok(());
    }
    // NDF encodes total received bytes minus one: four dummy bytes plus output.
    if out.len() > 65536 - 4 {
        return Err(too_long);
    }
    read(out)
}

#[cfg(test)]
/// Regressions for the do-while assembly receive loop and its hardware count width.
mod tests {
    use super::read_unique_id;

    #[test]
    /// Empty reads must never invoke the assembly callback that unconditionally stores a byte.
    fn empty_read_has_no_hardware_effect() {
        assert_eq!(
            read_unique_id(&mut [], (), |_| panic!("empty transaction entered")),
            Ok(())
        );
    }

    #[test]
    /// Maximum valid count reaches the callback; one byte more is rejected before side effects.
    fn ssi_count_boundary_is_checked_before_read() {
        let mut output = [0u8; 65533];
        assert_eq!(
            read_unique_id(&mut output[..65532], 7, |out| {
                out[65531] = 0xA5;
                Ok(())
            }),
            Ok(())
        );
        assert_eq!(output[65531], 0xA5);
        assert_eq!(
            read_unique_id(&mut output, 7, |_| panic!("oversized transaction entered")),
            Err(7)
        );
    }

    #[test]
    /// An ordinary identifier read preserves output writes and propagates callback failures.
    fn valid_read_propagates_callback_result() {
        let mut output = [0u8; 8];
        assert_eq!(
            read_unique_id(&mut output, 7, |out| {
                out[0] = 0x42;
                Err(9)
            }),
            Err(9)
        );
        assert_eq!(output[0], 0x42);
    }
}
