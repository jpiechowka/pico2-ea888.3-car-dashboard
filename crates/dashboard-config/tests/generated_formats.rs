#[path = "../../../test-support/generated.rs"]
mod generated;
use dashboard_config::*;

const SEED: u32 = 0x17EA_5005;
fn next(state: &mut u32) -> u32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state
}
fn budget() -> usize {
    std::env::var("DASHBOARD_FORMAT_CASES").map_or(512, |v| v.parse::<usize>().unwrap().clamp(16, 8192))
}
fn seal(bytes: &mut [u8]) {
    let mut checked = bytes[..8].to_vec();
    checked.extend_from_slice(&bytes[12..]);
    bytes[8..12].copy_from_slice(&crc32(&checked).to_le_bytes());
}

// A lost canonicality/semantic guard must not let the owned host parser and
// firmware's in-place publication disagree, including after a prior success.
#[test]
fn seeded_resealed_eav_mutations_agree_with_boot_publication() {
    let mut rng = SEED;
    let mut slot = ConfigSlot::new();
    let defaults = validate(default_config()).unwrap();
    let mut buffer = [0; MAX_FILE_BYTES];
    let mut custom = default_config();
    custom.views[0].name = "Previous".try_into().unwrap();
    custom.reduced_motion = true;
    let previous = validate(custom).unwrap();
    let original = encode(&previous, &mut buffer).unwrap().to_vec();
    let mut accepted = 0;
    let mut rejected = 0;
    for case in 0..budget() {
        let _case = generated::Case::new("format", u64::from(SEED), case);
        let mut input = original.clone();
        if case % 8 != 0 {
            let at = 12 + next(&mut rng) as usize % (input.len() - 12);
            input[at] ^= 1 << (next(&mut rng) % 8);
            seal(&mut input);
        }
        slot.load_at_boot(&original);
        let owned = decode(&input);
        let published = slot.load_at_boot(&input);
        match owned {
            Ok(valid) => {
                accepted += 1;
                assert_eq!(published.fault, None, "seed={SEED:x} case={case}");
                assert_eq!(published.config, valid, "seed={SEED:x} case={case}");
                assert_eq!(encode(&valid, &mut buffer).unwrap(), input);
                validate(valid.config().clone()).unwrap();
            }
            Err(error) => {
                rejected += 1;
                assert_eq!(published.fault, Some(error), "seed={SEED:x} case={case}");
                assert_eq!(published.config, defaults, "partial publication case={case}");
            }
        }
    }
    assert!(accepted > 0 && rejected > 0);
}

#[test]
fn seeded_profile_bytes_match_independent_wire_predicate() {
    use dashboard_config::startup::StartupProfile;
    let mut rng = SEED;
    for case in 0..budget() {
        let _case = generated::Case::new("format", u64::from(SEED), case);
        let mut bytes = *b"EASP\x02\x00\x01\x03\x58\x02\xc4\x09\x00\x00\x00\x00";
        let at = next(&mut rng) as usize % bytes.len();
        bytes[at] = next(&mut rng) as u8;
        let allowed = &bytes[..4] == b"EASP"
            && matches!(bytes[4], 1 | 2)
            && bytes[5] <= if bytes[4] == 1 { 5 } else { 10 }
            && bytes[6] <= 2
            && bytes[7] <= 7
            && u16::from_le_bytes([bytes[8], bytes[9]]) <= 2000
            && u16::from_le_bytes([bytes[10], bytes[11]]) <= 10000
            && bytes[12] <= if bytes[4] == 1 { 0 } else { 4 }
            && bytes[13] <= if bytes[4] == 1 { 0 } else { 4 }
            && bytes[14..] == [0, 0];
        let decoded = StartupProfile::decode(&bytes);
        assert_eq!(decoded.is_some(), allowed, "seed={SEED:x} case={case}");
        if let Some(profile) = decoded {
            assert!(profile.valid());
            assert_eq!(StartupProfile::decode(&profile.encode().unwrap()), Some(profile));
        }
    }
}

#[cfg(feature = "json")]
#[test]
fn seeded_json_and_frozen_migrations_converge_to_current_slot() {
    let mut rng = SEED;
    let mut slot = ConfigSlot::new();
    let mut buffer = [0; MAX_FILE_BYTES];
    for fixture in [
        include_bytes!("fixtures/v1-default.json").as_slice(),
        include_bytes!("fixtures/v2-default.json"),
        include_bytes!("fixtures/v3-default.json"),
        include_bytes!("fixtures/v4-default.json"),
    ] {
        let migrated = from_json_bytes(fixture).unwrap();
        let canonical = encode(&migrated, &mut buffer).unwrap();
        assert_eq!(slot.load_at_boot(canonical).config, migrated);
    }
    for case in 0..budget() {
        let _case = generated::Case::new("format", u64::from(SEED), case);
        let mut config = default_config();
        config.widgets.clear();
        config.orientation = Orientation::ALL[next(&mut rng) as usize % 4];
        config.reduced_motion = next(&mut rng) & 1 != 0;
        config.history_seconds = [10 + (next(&mut rng) % 891) as u16; 8];
        config.controls.hold_ms = 500 + (next(&mut rng) % 1001) as u16;
        let valid = validate(config).unwrap();
        let json = to_json(&valid).unwrap();
        assert_eq!(from_json_bytes(json.as_bytes()).unwrap(), valid);
        let canonical = encode(&valid, &mut buffer).unwrap().to_vec();
        assert_eq!(slot.load_at_boot(&canonical).config, valid);
        let mut mutated = json.into_bytes();
        let at = next(&mut rng) as usize % mutated.len();
        mutated[at] ^= 1 << (next(&mut rng) % 8);
        if let Ok(parsed) = from_json_bytes(&mutated) {
            let canonical = encode(&parsed, &mut buffer).unwrap();
            assert_eq!(slot.load_at_boot(canonical).config, parsed, "seed={SEED:x} case={case}");
            assert_eq!(from_json_bytes(to_json(&parsed).unwrap().as_bytes()).unwrap(), parsed);
        }
    }
}

#[test]
fn float_bit_boundaries_cannot_escape_resealed_validation() {
    let mut storage = [0; MAX_FILE_BYTES];
    for bits in [
        0x7FC00000, 0x7F800000, 0xFF800000, 0x7F7FFFFF, 0xFF7FFFFF, 0x00000001, 0x80000000,
    ] {
        let value = f32::from_bits(bits);
        let mut config = default_config();
        config.fuel = Fuel::Stoichiometric { ratio: value };
        assert!(validate(config.clone()).is_err(), "bits={bits:x}");
        let payload = postcard::to_slice(&config, &mut storage).unwrap();
        let mut bytes = b"EAV1".to_vec();
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        bytes.extend_from_slice(payload);
        seal(&mut bytes);
        assert!(matches!(decode(&bytes), Err(ConfigError::Validation(_))));
        let mut slot = ConfigSlot::new();
        assert_eq!(slot.load_at_boot(&bytes).config.config(), &default_config());
    }
}
