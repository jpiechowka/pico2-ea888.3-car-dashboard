use crate::*;

pub const HEADER_BYTES: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    Header,
    Version,
    Length,
    Checksum,
    Payload,
    Output,
    Validation(ValidationError),
    Json { line: usize, column: usize },
}

/// CRC-32/ISO-HDLC: reflected polynomial, all-one initial/final XOR.
pub fn crc32(bytes: &[u8]) -> u32 { crc_parts(&[bytes]) }

fn crc_parts(parts: &[&[u8]]) -> u32 {
    let mut crc = u32::MAX;
    for bytes in parts {
        for &byte in *bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xEDB88320 & 0_u32.wrapping_sub(crc & 1));
            }
        }
    }
    crc ^ u32::MAX
}

pub fn decode(bytes: &[u8]) -> Result<ValidatedConfig, ConfigError> {
    if bytes.len() < HEADER_BYTES || bytes.len() > MAX_FILE_BYTES {
        return Err(ConfigError::Length);
    }
    if &bytes[..4] != b"EAV1" {
        return Err(ConfigError::Header);
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != VERSION && !(cfg!(feature = "json") && matches!(version, 1 | 2)) {
        return Err(ConfigError::Version);
    }
    let len = usize::from(u16::from_le_bytes([bytes[6], bytes[7]]));
    if len != bytes.len() - HEADER_BYTES {
        return Err(ConfigError::Length);
    }
    let checksum = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    if checksum != crc_parts(&[&bytes[..8], &bytes[HEADER_BYTES..]]) {
        return Err(ConfigError::Checksum);
    }
    #[cfg(feature = "json")]
    if matches!(version, 1 | 2) {
        if version == 2 {
            let (config, rest) = postcard::take_from_bytes::<crate::legacy::LegacyConfigV2>(&bytes[HEADER_BYTES..])
                .map_err(|_| ConfigError::Payload)?;
            if !rest.is_empty() {
                return Err(ConfigError::Payload);
            }
            postcard::serialize_with_flavor(
                &config,
                Compare {
                    remaining: &bytes[HEADER_BYTES..],
                },
            )
            .map_err(|_| ConfigError::Payload)?;
            return config.upgrade();
        }
        let (config, rest) = postcard::take_from_bytes::<crate::legacy::LegacyConfig>(&bytes[HEADER_BYTES..])
            .map_err(|_| ConfigError::Payload)?;
        if !rest.is_empty() {
            return Err(ConfigError::Payload);
        }
        postcard::serialize_with_flavor(
            &config,
            Compare {
                remaining: &bytes[HEADER_BYTES..],
            },
        )
        .map_err(|_| ConfigError::Payload)?;
        return config.upgrade();
    }
    let (config, remainder) =
        postcard::take_from_bytes::<Config>(&bytes[HEADER_BYTES..]).map_err(|_| ConfigError::Payload)?;
    if !remainder.is_empty() {
        return Err(ConfigError::Payload);
    }
    // Postcard usize lengths accept a wider varint on a 64-bit host than on
    // Pico/Wasm. Require the canonical representation on every platform. This
    // streaming comparison adds no payload-sized temporary or allocation.
    postcard::serialize_with_flavor(
        &config,
        Compare {
            remaining: &bytes[HEADER_BYTES..],
        },
    )
    .map_err(|_| ConfigError::Payload)?;
    validate(config).map_err(ConfigError::Validation)
}

struct Compare<'a> {
    remaining: &'a [u8],
}

impl postcard::ser_flavors::Flavor for Compare<'_> {
    type Output = ();

    fn try_push(
        &mut self,
        byte: u8,
    ) -> postcard::Result<()> {
        self.try_extend(&[byte])
    }

    fn try_extend(
        &mut self,
        bytes: &[u8],
    ) -> postcard::Result<()> {
        if self.remaining.get(..bytes.len()) != Some(bytes) {
            return Err(postcard::Error::SerializeBufferFull);
        }
        self.remaining = &self.remaining[bytes.len()..];
        Ok(())
    }

    fn finalize(self) -> postcard::Result<()> {
        if self.remaining.is_empty() {
            Ok(())
        } else {
            Err(postcard::Error::SerializeBufferFull)
        }
    }
}

/// Only a validated wrapper can be encoded. On error the destination may have
/// been modified, but no successful byte slice is returned.
pub fn encode<'a>(
    config: &ValidatedConfig,
    out: &'a mut [u8],
) -> Result<&'a [u8], ConfigError> {
    let capacity = out.len().min(MAX_FILE_BYTES);
    if capacity < HEADER_BYTES {
        return Err(ConfigError::Output);
    }
    let payload_len = postcard::to_slice(config.config(), &mut out[HEADER_BYTES..capacity])
        .map_err(|_| ConfigError::Output)?
        .len();
    let len = u16::try_from(payload_len).map_err(|_| ConfigError::Output)?;
    out[..4].copy_from_slice(b"EAV1");
    out[4..6].copy_from_slice(&VERSION.to_le_bytes());
    out[6..8].copy_from_slice(&len.to_le_bytes());
    let end = HEADER_BYTES + payload_len;
    let checksum = crc_parts(&[&out[..8], &out[HEADER_BYTES..end]]);
    out[8..12].copy_from_slice(&checksum.to_le_bytes());
    Ok(&out[..end])
}

#[cfg(feature = "json")]
pub fn from_json_bytes(bytes: &[u8]) -> Result<ValidatedConfig, ConfigError> {
    if bytes.len() > MAX_JSON_BYTES {
        return Err(ConfigError::Length);
    }
    let json_error = |e: serde_json::Error| {
        ConfigError::Json {
            line: e.line(),
            column: e.column(),
        }
    };
    #[derive(serde::Deserialize)]
    struct Version {
        version: u16,
    }
    let version: Version = serde_json::from_slice(bytes).map_err(json_error)?;
    if version.version == 1 {
        return serde_json::from_slice::<crate::legacy::LegacyConfig>(bytes)
            .map_err(json_error)?
            .upgrade();
    }
    if version.version == 2 {
        return serde_json::from_slice::<crate::legacy::LegacyConfigV2>(bytes)
            .map_err(json_error)?
            .upgrade();
    }
    if version.version != VERSION {
        return Err(ConfigError::Version);
    }
    let config: Config = serde_json::from_slice(bytes).map_err(json_error)?;
    validate(config).map_err(ConfigError::Validation)
}

#[cfg(feature = "json")]
pub fn to_json(config: &ValidatedConfig) -> Result<alloc::string::String, ConfigError> {
    // The wrapper prevents serde_json's NaN/Infinity -> null serialization.
    let text = serde_json::to_string_pretty(config.config()).map_err(|e| {
        ConfigError::Json {
            line: e.line(),
            column: e.column(),
        }
    })?;
    if text.len() > MAX_JSON_BYTES {
        return Err(ConfigError::Length);
    }
    Ok(text)
}
