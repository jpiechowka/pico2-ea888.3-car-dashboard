use dashboard_config::startup::{StartupProfile, StartupStatus, StartupTheme};

#[test]
fn profile_wire_values_and_flags_match_independent_v1_bytes() {
    assert_eq!(
        StartupProfile::DEFAULT.encode().unwrap(),
        *b"EASP\x01\x00\x01\x03\x58\x02\xc4\x09\x00\x00\x00\x00"
    );
    for (theme_byte, theme) in [
        StartupTheme::Aezakmi,
        StartupTheme::Turbo,
        StartupTheme::Sanic,
        StartupTheme::BoostBro,
        StartupTheme::WhereLambo,
        StartupTheme::Custom,
    ]
    .into_iter()
    .enumerate()
    {
        for (status_byte, status) in [StartupStatus::Off, StartupStatus::Brief, StartupStatus::Detailed]
            .into_iter()
            .enumerate()
        {
            for flags in 0..8u8 {
                let profile = StartupProfile {
                    theme,
                    status,
                    status_ms: 2000,
                    duration_ms: 10000,
                    loop_animation: flags & 1 != 0,
                    warning_led: flags & 2 != 0,
                    interrupt_on_attention: flags & 4 != 0,
                };
                let bytes = [
                    b'E',
                    b'A',
                    b'S',
                    b'P',
                    1,
                    theme_byte as u8,
                    status_byte as u8,
                    flags,
                    0xD0,
                    0x07,
                    0x10,
                    0x27,
                    0,
                    0,
                    0,
                    0,
                ];
                assert_eq!(profile.encode(), Some(bytes));
                assert_eq!(StartupProfile::decode(&bytes), Some(profile));
                assert_eq!(
                    profile.total_duration(),
                    if status == StartupStatus::Off { 10000 } else { 12000 }
                );
            }
        }
    }
}

#[test]
fn profile_rejects_unknown_fields_lengths_and_out_of_range_durations() {
    let valid = StartupProfile::DEFAULT.encode().unwrap();
    for length in 0..16 {
        assert_eq!(StartupProfile::decode(&valid[..length]), None);
    }
    let mut trailing = valid.to_vec();
    trailing.push(0);
    assert_eq!(StartupProfile::decode(&trailing), None);
    for (index, value) in [
        (0, b'X'),
        (4, 0),
        (4, 2),
        (5, 6),
        (5, 255),
        (6, 3),
        (6, 255),
        (7, 8),
        (7, 128),
        (12, 1),
        (13, 1),
        (14, 1),
        (15, 1),
    ] {
        let mut bytes = valid;
        bytes[index] = value;
        assert_eq!(StartupProfile::decode(&bytes), None, "byte {index} value {value}");
    }
    for (status_ms, duration_ms, accepted) in [
        (0, 0, true),
        (2000, 10000, true),
        (2001, 10000, false),
        (2000, 10001, false),
        (u16::MAX, 0, false),
        (0, u16::MAX, false),
    ] {
        let profile = StartupProfile {
            status_ms,
            duration_ms,
            ..StartupProfile::DEFAULT
        };
        let mut bytes = valid;
        bytes[8..10].copy_from_slice(&status_ms.to_le_bytes());
        bytes[10..12].copy_from_slice(&duration_ms.to_le_bytes());
        assert_eq!(profile.encode().is_some(), accepted);
        assert_eq!(StartupProfile::decode(&bytes), accepted.then_some(profile));
    }
}
