//! CYW43's compact Intel HEX download format, not an executable image.
//! Validate the complete immutable blob before the first backplane write.

use crate::consts::*;

#[derive(Clone)]
pub(crate) struct Firmware<'a> {
    bytes: &'a [u8],
    remaining: u8,
    address_base: u32,
    chip_base: u32,
    finished: bool,
}

impl<'a> Firmware<'a> {
    pub(crate) fn new(bytes: &'a [u8], chip_base: u32) -> Result<Self, ()> {
        let version_len = usize::from(*bytes.first().ok_or(())?);
        let version = bytes.get(1..1 + version_len).ok_or(())?;
        if version.last() != Some(&0) || version[..version_len - 1].contains(&0) {
            return Err(());
        }
        let remaining = *bytes.get(1 + version_len).ok_or(())?;
        let firmware = Self {
            bytes: &bytes[2 + version_len..],
            remaining,
            address_base: 0,
            chip_base,
            finished: false,
        };
        let mut check = firmware.clone();
        let mut has_data = false;
        while check.next_data()?.is_some() {
            has_data = true;
        }
        if !has_data {
            return Err(());
        }
        Ok(firmware)
    }

    pub(crate) fn next_data(&mut self) -> Result<Option<(u32, &'a [u8])>, ()> {
        while !self.finished {
            self.remaining = self.remaining.checked_sub(1).ok_or(())?;
            let header = self.bytes.get(..4).ok_or(())?;
            let len = usize::from(header[0]);
            let address = u32::from(u16::from_be_bytes([header[1], header[2]]));
            let payload = self.bytes.get(4..4 + len).ok_or(())?;
            self.bytes = &self.bytes[4 + len..];
            match header[3] {
                BTFW_HEX_LINE_TYPE_DATA => {
                    if payload.is_empty() {
                        continue;
                    }
                    let dest = self
                        .address_base
                        .checked_add(address)
                        .and_then(|v| v.checked_add(self.chip_base))
                        .ok_or(())?;
                    // The upload includes a read/modify/write of the last word.
                    dest.checked_add(len as u32)
                        .and_then(|v| v.checked_add(3))
                        .ok_or(())?;
                    return Ok(Some((dest, payload)));
                }
                BTFW_HEX_LINE_TYPE_END_OF_DATA => {
                    if len != 0 || address != 0 || self.remaining != 0 || !self.bytes.is_empty() {
                        return Err(());
                    }
                    self.finished = true;
                }
                BTFW_HEX_LINE_TYPE_EXTENDED_ADDRESS | BTFW_HEX_LINE_TYPE_EXTENDED_SEGMENT_ADDRESS => {
                    let pair: [u8; 2] = payload.try_into().map_err(|_| ())?;
                    if address != 0 {
                        return Err(());
                    }
                    self.address_base = u32::from(u16::from_be_bytes(pair))
                        << if header[3] == BTFW_HEX_LINE_TYPE_EXTENDED_ADDRESS { 16 } else { 4 };
                }
                BTFW_HEX_LINE_TYPE_ABSOLUTE_32BIT_ADDRESS => {
                    let word: [u8; 4] = payload.try_into().map_err(|_| ())?;
                    if address != 0 {
                        return Err(());
                    }
                    // CYW43 uses type 5 as a persistent base, not an entry point.
                    self.address_base = u32::from_be_bytes(word);
                }
                _ => return Err(()),
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec::Vec;

    fn image(records: &[&[u8]]) -> Vec<u8> {
        let mut bytes = std::vec![2, b'V', 0, records.len() as u8];
        for record in records {
            bytes.extend_from_slice(record);
        }
        bytes
    }

    const EOF: &[u8] = &[0, 0, 0, 1];
    const DATA: &[u8] = &[1, 0, 8, 0, 42];

    #[test]
    fn modes_persist_across_records_and_zero_data_is_not_eof() {
        for (mode, base) in [
            (&[2, 0, 0, 4, 0, 2][..], 0x20000),
            (&[2, 0, 0, 2, 0, 2][..], 0x20),
            (&[4, 0, 0, 5, 0, 2, 0, 0][..], 0x20000),
        ] {
            let bytes = image(&[mode, &[0, 0, 0, 0], DATA, DATA, EOF]);
            let mut parsed = Firmware::new(&bytes, 0x19000000).unwrap();
            for _ in 0..2 {
                assert_eq!(parsed.next_data(), Ok(Some((0x19000000 + base + 8, &[42][..]))));
            }
            assert_eq!(parsed.next_data(), Ok(None));
            assert_eq!(parsed.next_data(), Ok(None));
        }
    }

    #[test]
    fn all_truncations_counts_types_lengths_and_overflows_fail_closed() {
        let good = image(&[DATA, EOF]);
        for n in 0..good.len() {
            assert!(Firmware::new(&good[..n], 0).is_err(), "length {n}");
        }
        for count in 0..=255 {
            let mut changed = good.clone();
            changed[3] = count;
            assert_eq!(Firmware::new(&changed, 0).is_ok(), count == 2);
        }
        for kind in 0..=255 {
            let bytes = image(&[&[1, 0, 8, kind, 42], EOF]);
            assert_eq!(Firmware::new(&bytes, 0).is_ok(), kind == 0);
        }
        for (base, offset) in [(u32::MAX, 0), (u32::MAX - 3, 0), (u32::MAX - 8, 8)] {
            let record = [1, 0, offset, 0, 42];
            assert!(Firmware::new(&image(&[&record, EOF]), base).is_err());
        }
        for malformed in [
            image(&[DATA]), image(&[EOF]), image(&[DATA, EOF, EOF]),
            image(&[DATA, &[0, 0, 1, 1]]),
            image(&[&[1, 0, 0, 4, 0], DATA, EOF]),
            image(&[&[2, 0, 1, 4, 0, 2], DATA, EOF]),
            image(&[&[4, 0, 0, 5, 0xff, 0xff, 0xff, 0xff], DATA, EOF]),
        ] {
            assert!(Firmware::new(&malformed, 0).is_err());
        }
        let mut trailing = good.clone();
        trailing.push(0);
        assert!(Firmware::new(&trailing, 0).is_err());
        for version in [&[0, 1][..], &[1, b'V', 1], &[3, b'V', 0, 0, 1]] {
            assert!(Firmware::new(version, 0).is_err());
        }
    }

    #[test]
    fn pinned_radio_image_has_exact_records_version_and_no_trailing_data() {
        let bytes = include_bytes!("../../cyw43-firmware/43439A0_btfw.bin");
        assert_eq!(&bytes[1..usize::from(bytes[0])], b"CYW4343A2_001.003.016.0065.0000_Generic_SDIO_37MHz_wlbga_BU_RPI_dl_signed");
        let mut parsed = Firmware::new(bytes, crate::CHIP.bluetooth_base_address).unwrap();
        let mut count = 0;
        let mut total = 0;
        while let Some((_, data)) = parsed.next_data().unwrap() {
            count += 1;
            total += data.len();
        }
        assert_eq!(count, 30);
        assert_eq!(total, 6746);
    }
}
