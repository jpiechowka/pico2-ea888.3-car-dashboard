//! Browser address presentation only. Validation and stored bytes are shared.
pub fn parse_address(text: &str) -> Option<[u8; 6]> {
    let bytes = text.as_bytes();
    if bytes.len() != 17 {
        return None;
    }
    let mut address = [0; 6];
    for (i, byte) in address.iter_mut().enumerate() {
        let start = i * 3;
        if i < 5 && bytes[start + 2] != b':' {
            return None;
        }
        let pair = core::str::from_utf8(&bytes[start..start + 2]).ok()?;
        if !pair.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        *byte = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(address)
}
pub fn format_address(address: [u8; 6]) -> String {
    format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        address[0], address[1], address[2], address[3], address[4], address[5]
    )
}
