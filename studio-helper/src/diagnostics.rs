//! Byte-bounded summaries for fallible native operations.
use dashboard_helper_protocol::Text;

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
mod tests {
    use super::*;
    #[test]
    fn exact_bounds_preserve_text_and_truncation_preserves_utf8() {
        assert_eq!(summary::<4>("😀").as_str(), "😀");
        assert_eq!(summary::<5>("ééé").as_str(), "é...");
        assert_eq!(summary::<2>("éé").as_str(), "..");
        assert_eq!(summary::<0>("x").as_str(), "");
        assert_eq!(summary::<8>("ok").as_str(), "ok");
    }
}
