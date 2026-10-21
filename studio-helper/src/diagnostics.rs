//! Byte-bounded summaries for fallible native operations.
use dashboard_helper_protocol::Text;

/// Returns `value` bounded to `N` UTF-8 bytes, preserving character boundaries.
/// Truncated strings end in up to three ASCII dots; `N == 0` returns empty text.
/// The internal construction cannot exceed `N` and does not expose raw truncation bytes.
pub(crate) fn summary<const N: usize>(value: &str) -> Text<N> {
    let value = if value.len() <= N {
        value.to_owned()
    } else {
        let marker = &"..."[..N.min(3)];
        let end = value.floor_char_boundary(N - marker.len());
        format!("{}{marker}", &value[..end])
    };
    Text::new(value).expect("summary is bounded in UTF-8 bytes")
}

#[cfg(test)]
/// Boundary fixtures for zero/small capacities and multibyte Unicode truncation.
mod tests {
    use super::*;
    #[test]
    /// Assert exact-fit strings survive and truncation stays valid UTF-8 within every bound.
    fn exact_bounds_preserve_text_and_truncation_preserves_utf8() {
        assert_eq!(summary::<4>("😀").as_str(), "😀");
        assert_eq!(summary::<5>("ééé").as_str(), "é...");
        assert_eq!(summary::<2>("éé").as_str(), "..");
        assert_eq!(summary::<0>("x").as_str(), "");
        assert_eq!(summary::<8>("ok").as_str(), "ok");
    }
}
