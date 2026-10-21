//! Pure enable-bit updates shared by PIO batches and native regression tests.

/// Records `enabled` for the state machines selected by `sm_mask` in `mask`/`values`.
/// Bits outside `sm_mask` retain their previous request; each selected bit uses the latest request.
pub(crate) fn set_enable(
    mask: &mut u8,
    values: &mut u8,
    sm_mask: u8,
    enabled: bool,
) {
    *mask |= sm_mask;
    *values = (*values & !sm_mask) | if enabled { sm_mask } else { 0 };
}

/// Returns hardware enable bits after applying `values` for `mask` to `current`.
/// Unselected state machines are preserved; executing a batch does not consume its requests.
pub(crate) fn apply_enable(
    current: u8,
    mask: u8,
    values: u8,
) -> u8 {
    (current & !mask) | values
}

#[cfg(test)]
/// Last-request ordering, state-machine independence and persistent batch semantics.
mod tests {
    use super::*;

    /// Both transition directions and repeated requests obey the latest value.
    #[test]
    fn latest_request_wins_in_both_directions() {
        for requests in [[true, false], [false, true], [true, true], [false, false]] {
            let (mut mask, mut values) = (0, 0);
            for enabled in requests {
                set_enable(&mut mask, &mut values, 1, enabled);
            }
            assert_eq!(mask, 1);
            assert_eq!(apply_enable(0, mask, values) & 1 != 0, requests[1]);
        }
    }

    /// Updating one machine preserves earlier requests for others and untouched hardware bits.
    #[test]
    fn state_machines_are_independent() {
        let (mut mask, mut values) = (0, 0);
        set_enable(&mut mask, &mut values, 1, true);
        set_enable(&mut mask, &mut values, 2, true);
        set_enable(&mut mask, &mut values, 1, false);
        assert_eq!(apply_enable(0b1100, mask, values), 0b1110);
        set_enable(&mut mask, &mut values, 4, false);
        assert_eq!(apply_enable(0b1100, mask, values), 0b1010);
    }

    /// Execution preserves requests for reuse while subsequent setters can reverse them.
    #[test]
    fn execution_and_reuse_keep_requests_editable() {
        let (mut mask, mut values) = (0, 0);
        set_enable(&mut mask, &mut values, 1, true);
        let mut hardware = apply_enable(0b1000, mask, values);
        assert_eq!(hardware, 0b1001);
        assert_eq!(apply_enable(hardware, mask, values), hardware);
        set_enable(&mut mask, &mut values, 1, false);
        hardware = apply_enable(hardware, mask, values);
        assert_eq!(hardware, 0b1000);
        set_enable(&mut mask, &mut values, 2, true);
        assert_eq!(apply_enable(hardware, mask, values), 0b1010);
    }
}
