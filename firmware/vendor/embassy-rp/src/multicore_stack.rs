//! Pure launch-stack arithmetic, also compiled directly by the host regression harness.

/// Returns the number of 32-bit words below the eight-byte-aligned top of `size` bytes.
/// Panics when the two launch argument words do not fit. This checks launch bookkeeping,
/// not the closure, compiler-generated startup frame, interrupts or application stack budget.
/// Division precedes multiplication, so even `usize::MAX` cannot overflow the word count.
pub(crate) const fn stack_words(size: usize) -> usize {
    core::assert!(size >= 8, "CORE1 stack cannot hold launch arguments");
    size / 8 * 2
}

#[cfg(test)]
/// Host regressions for rejected launch sizes, alignment and arithmetic overflow.
mod tests {
    extern crate std;
    use super::stack_words;

    #[test]
    /// Every allocation shorter than the two launch words must fail before pointer formation.
    fn rejects_short_launch_stacks() {
        for size in 0..8 {
            assert!(std::panic::catch_unwind(|| stack_words(size)).is_err(), "size={size}");
        }
    }

    #[test]
    /// Checks boundary tails and the application's 32 KiB allocation without changing capacity.
    fn retains_aligned_capacity_and_argument_space() {
        for (bytes, words) in [(8, 2), (15, 2), (16, 4), (31, 6), (32, 8), (32768, 8192)] {
            let actual = stack_words(bytes);
            assert_eq!(actual, words);
            assert_eq!(actual * 4 % 8, 0);
            assert!(actual * 4 <= bytes);
            assert!(actual >= 2);
        }
    }

    #[test]
    /// Mathematical extremes cannot overflow before allocation/pointer limits are considered.
    fn maximum_counts_do_not_overflow() {
        for size in [u32::MAX as usize, isize::MAX as usize, usize::MAX] {
            let bytes = stack_words(size) * 4;
            assert!(bytes <= size);
            assert!(size - bytes < 8);
        }
    }
}
