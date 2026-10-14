use dashboard_build_profiles::{
    ClockError,
    ClockSelection,
    CustomClock,
    ProfileId,
    ProfileKind,
    UserObservation,
    Voltage,
    catalogue,
    resolve,
};

type PllFields = (u8, u16, u8, u8);
type SpiFields = (u8, u8);
type ProfileCase = (ProfileId, u32, u32, Voltage, PllFields, u32, u32, SpiFields);
#[test]
fn exact_all_eight_alias_sets_are_globally_unique() {
    let expected: &[&[&str]] = &[
        &[
            "stock",
            "pico2",
            "pico2-run",
            "pico2-fast",
            "pico2-fast-run",
            "pico2-150-62-1v10",
            "pico2-150-62-1v10-run",
        ],
        &["132-66", "pico2-132-66-1v10", "pico2-132-66-1v10-run"],
        &["140-70", "pico2-140-70-1v10", "pico2-140-70-1v10-run"],
        &["150-15", "pico2-150-15-1v10", "pico2-150-15-1v10-run"],
        &["250-62.5", "pico2-250-62-1v10", "pico2-250-62-1v10-run"],
        &["280-70", "pico2-280-70-1v30", "pico2-280-70-1v30-run"],
        &["290-72.5", "pico2-290-72-1v30", "pico2-290-72-1v30-run"],
        &["300-75", "pico2-300-75-1v30", "pico2-300-75-1v30-run"],
    ];
    let mut all = std::collections::BTreeSet::new();
    for (p, aliases) in catalogue().iter().zip(expected) {
        assert_eq!(p.aliases, *aliases, "{:?}", p.id);
        for alias in p.aliases {
            assert!(all.insert(*alias), "duplicate {alias}");
        }
    }
    assert_eq!(all.len(), 28);
}

#[cfg(feature = "serde")]
#[test]
fn every_named_selection_has_an_exact_serde_roundtrip() {
    for (p, name) in catalogue().iter().zip([
        "stock",
        "cpu132_spi66",
        "cpu140_spi70",
        "timing150_spi15",
        "cpu250_spi62",
        "cpu280_spi70",
        "legacy290_spi72",
        "cpu300_spi75",
    ]) {
        let expected = format!("{{\"named\":\"{name}\"}}");
        let selected = ClockSelection::Named(p.id);
        assert_eq!(serde_json::to_string(&selected).unwrap(), expected);
        assert_eq!(serde_json::from_str::<ClockSelection>(&expected).unwrap(), selected);
    }
}
const CASES: &[ProfileCase] = &[
    (
        ProfileId::Stock,
        150_000_000,
        62_500_000,
        Voltage::V1_10,
        (1, 125, 5, 2),
        150_000_000,
        37_500_000,
        (2, 1),
    ),
    (
        ProfileId::Cpu132Spi66,
        132_000_000,
        66_000_000,
        Voltage::V1_10,
        (1, 132, 6, 2),
        132_000_000,
        66_000_000,
        (2, 0),
    ),
    (
        ProfileId::Cpu140Spi70,
        140_000_000,
        70_000_000,
        Voltage::V1_10,
        (1, 105, 3, 3),
        140_000_000,
        70_000_000,
        (2, 0),
    ),
    (
        ProfileId::Timing150Spi15,
        150_000_000,
        15_000_000,
        Voltage::V1_10,
        (1, 125, 5, 2),
        150_000_000,
        15_000_000,
        (2, 4),
    ),
    (
        ProfileId::Cpu250Spi62,
        250_000_000,
        62_500_000,
        Voltage::V1_10,
        (1, 125, 6, 1),
        250_000_000,
        62_500_000,
        (2, 1),
    ),
    (
        ProfileId::Cpu280Spi70,
        280_000_000,
        70_000_000,
        Voltage::V1_30,
        (1, 70, 3, 1),
        280_000_000,
        70_000_000,
        (2, 1),
    ),
    (
        ProfileId::Legacy290Spi72,
        290_000_000,
        72_500_000,
        Voltage::V1_30,
        (1, 121, 5, 1),
        290_400_000,
        48_400_000,
        (2, 2),
    ),
    (
        ProfileId::Cpu300Spi75,
        300_000_000,
        75_000_000,
        Voltage::V1_30,
        (1, 125, 5, 1),
        300_000_000,
        75_000_000,
        (2, 1),
    ),
];

fn custom(
    cpu_hz: u32,
    spi_max_hz: u32,
) -> ClockSelection {
    ClockSelection::Custom(CustomClock {
        cpu_hz,
        spi_max_hz,
        voltage: Voltage::V1_30,
    })
}

#[test]
fn named_profiles_match_independent_clock_table() {
    for &(id, cpu_request, spi_request, voltage, pll, cpu_actual, spi_actual, spi_div) in CASES {
        let r = resolve(ClockSelection::Named(id)).unwrap();
        assert_eq!(
            (r.requested_cpu_hz, r.requested_spi_hz, r.voltage),
            (cpu_request, spi_request, voltage),
            "{id:?}"
        );
        assert_eq!(
            (r.pll.refdiv, r.pll.fbdiv, r.pll.postdiv1, r.pll.postdiv2),
            pll,
            "{id:?}"
        );
        assert_eq!((r.actual_cpu_hz, r.actual_spi_hz), (cpu_actual, spi_actual), "{id:?}");
        assert_eq!((r.cpsdvsr, r.scr), spi_div, "{id:?}");
        assert_eq!(r.legacy_rounding, id == ProfileId::Legacy290Spi72);
    }
}

#[test]
fn catalogue_has_exact_ids_and_compatibility_metadata() {
    let profiles = catalogue();
    assert_eq!(profiles.len(), 8);
    for (profile, case) in profiles.iter().zip(CASES) {
        assert_eq!(profile.id, case.0);
        assert_eq!(
            (profile.requested_cpu_hz, profile.requested_spi_hz, profile.voltage),
            (case.1, case.2, case.3)
        );
        assert!(!profile.label.is_empty());
        assert_eq!(profile.panel_timing_warning, profile.id != ProfileId::Timing150Spi15);
    }
    assert_eq!(profiles[0].feature, "cpu150-spi62-1v10");
    assert!(profiles[0].aliases.contains(&"stock"));
    assert_eq!(profiles[1].feature, "cpu132-spi66-1v10");
    assert!(profiles[1].aliases.contains(&"132-66"));
    assert_eq!(profiles[2].feature, "cpu140-spi70-1v10");
    assert!(profiles[2].aliases.contains(&"140-70"));
    assert_eq!(profiles[3].feature, "cpu150-spi15-1v10");
    assert!(profiles[3].aliases.contains(&"150-15"));
    assert_eq!(profiles[4].feature, "cpu250-spi62-1v10");
    assert_eq!(profiles[5].feature, "cpu280-spi70-1v30");
    assert_eq!(profiles[5].kind, ProfileKind::Expert);
    assert_eq!(profiles[5].observation, Some(UserObservation::Working));
    assert_eq!(profiles[6].feature, "cpu290-spi72-1v30");
    assert!(profiles[6].aliases.contains(&"290-72.5"));
    assert_eq!(profiles[6].kind, ProfileKind::LegacyFailed);
    assert_eq!(profiles[6].observation, Some(UserObservation::Failed));
    assert_eq!(profiles[7].feature, "cpu300-spi75-1v30");
}

#[test]
fn named_expert_policy_matches_accepted_workspace_contract() {
    let expected = [
        (ProfileId::Stock, false),
        (ProfileId::Cpu132Spi66, true),
        (ProfileId::Cpu140Spi70, true),
        (ProfileId::Timing150Spi15, false),
        (ProfileId::Cpu250Spi62, true),
        (ProfileId::Cpu280Spi70, true),
        (ProfileId::Legacy290Spi72, true),
        (ProfileId::Cpu300Spi75, true),
    ];
    assert_eq!(catalogue().len(), expected.len());
    for (profile, (id, requires_expert)) in catalogue().iter().zip(expected) {
        assert_eq!(profile.id, id);
        assert_eq!(profile.requires_expert, requires_expert, "{id:?}");
    }
}

#[test]
fn exact_custom_290_uses_second_reference_divider_without_legacy_observation() {
    let r = resolve(custom(290_000_000, 72_500_000)).unwrap();
    assert_eq!(
        (r.pll.refdiv, r.pll.fbdiv, r.pll.postdiv1, r.pll.postdiv2),
        (2, 145, 3, 1)
    );
    assert_eq!(
        (r.actual_cpu_hz, r.actual_spi_hz, r.cpsdvsr, r.scr),
        (290_000_000, 72_500_000, 2, 1)
    );
    assert!(!r.legacy_rounding);
}

#[test]
fn rejects_in_range_unrepresentable_clocks_without_rounding() {
    for cpu in [12_000_000, 290_000_001, 290_399_999] {
        assert_eq!(resolve(custom(cpu, 1_000_000)), Err(ClockError::NoExactPll), "{cpu}");
    }
}

#[test]
fn spi_ceil_quantization_and_reproducible_neighbors() {
    let r = resolve(custom(280_000_000, 69_000_000)).unwrap();
    assert_eq!((r.cpsdvsr, r.scr, r.actual_spi_hz), (2, 2, 46_666_666));
    let below = r.spi_neighbors.below_or_equal.unwrap();
    let above = r.spi_neighbors.above.unwrap();
    assert_eq!((below.actual_hz, below.cpsdvsr, below.scr), (46_666_666, 2, 2));
    assert_eq!((above.actual_hz, above.cpsdvsr, above.scr), (70_000_000, 2, 1));
    assert_eq!(
        (
            resolve(custom(280_000_000, below.request_hz)).unwrap().cpsdvsr,
            resolve(custom(280_000_000, below.request_hz)).unwrap().scr
        ),
        (2, 2)
    );
    assert_eq!(
        (
            resolve(custom(280_000_000, above.request_hz)).unwrap().cpsdvsr,
            resolve(custom(280_000_000, above.request_hz)).unwrap().scr
        ),
        (2, 1)
    );
    assert_eq!(
        resolve(custom(280_000_000, 70_000_000)).unwrap().actual_spi_hz,
        70_000_000
    );
}

#[test]
fn a_fractional_rate_above_request_stays_above_even_when_display_floor_matches() {
    let r = resolve(custom(140_000_000, 23_333_333)).unwrap();
    assert_eq!(r.actual_spi_hz, 17_500_000);
    let above = r.spi_neighbors.above.unwrap();
    assert_eq!(above.actual_hz, 23_333_333);
    assert_eq!((above.cpsdvsr, above.scr), (2, 2));
    assert!(u64::from(r.actual_cpu_hz) > u64::from(r.requested_spi_hz) * 2 * 3);
    assert_eq!(resolve(custom(140_000_000, above.request_hz)).unwrap().scr, 2);
}

#[test]
fn rejects_public_input_bounds() {
    for cpu in [0, 11_999_999, 300_000_001, u32::MAX] {
        assert_eq!(resolve(custom(cpu, 1_000_000)), Err(ClockError::CpuOutOfRange));
    }
    for spi in [0, 999_999, 75_000_001, u32::MAX] {
        assert_eq!(resolve(custom(280_000_000, spi)), Err(ClockError::SpiOutOfRange));
    }
    assert_eq!(
        resolve(custom(132_000_000, 70_000_000)),
        Err(ClockError::SpiAbovePeripheralLimit)
    );
}

#[test]
fn rational_pll_and_spi_invariants_hold_for_sample_grid() {
    for cpu in [
        132_000_000,
        140_000_000,
        150_000_000,
        250_000_000,
        280_000_000,
        290_000_000,
        300_000_000,
    ] {
        for spi in [
            1_000_000, 15_000_000, 37_500_000, 46_000_000, 62_500_000, 69_000_000, 70_000_000, 72_500_000, 75_000_000,
        ] {
            if spi > cpu / 2 {
                continue;
            }
            let r = resolve(custom(cpu, spi)).unwrap();
            let p = r.pll;
            assert_eq!(
                12_000_000u64 * u64::from(p.fbdiv),
                u64::from(cpu) * u64::from(p.refdiv) * u64::from(p.postdiv1) * u64::from(p.postdiv2)
            );
            let vco = 12_000_000u64 / u64::from(p.refdiv) * u64::from(p.fbdiv);
            assert!((750_000_000..=1_600_000_000).contains(&vco));
            assert_eq!(r.actual_spi_hz, cpu / (u32::from(r.cpsdvsr) * (u32::from(r.scr) + 1)));
            assert!(u64::from(cpu) <= u64::from(spi) * u64::from(r.cpsdvsr) * (u64::from(r.scr) + 1));
            let below = r.spi_neighbors.below_or_equal.unwrap();
            assert_eq!((below.cpsdvsr, below.scr), (r.cpsdvsr, r.scr));
            for (candidate, at_or_below) in [(Some(below), true), (r.spi_neighbors.above, false)] {
                let Some(candidate) = candidate else { continue };
                assert!((1_000_000..=75_000_000).contains(&candidate.request_hz));
                assert!(candidate.request_hz <= cpu / 2);
                let reproduced = resolve(custom(cpu, candidate.request_hz)).unwrap();
                assert_eq!((reproduced.cpsdvsr, reproduced.scr), (candidate.cpsdvsr, candidate.scr));
                let denominator = u64::from(candidate.cpsdvsr) * (u64::from(candidate.scr) + 1);
                assert_eq!(
                    candidate.actual_hz,
                    u32::try_from(u64::from(cpu) / denominator).unwrap()
                );
                assert_eq!(u64::from(cpu) <= u64::from(spi) * denominator, at_or_below);
            }
        }
    }
}

#[cfg(feature = "serde")]
#[test]
fn optional_serde_accepts_only_typed_voltages_and_exact_shapes() {
    let valid = r#"{"custom":{"cpu_hz":290000000,"spi_max_hz":72500000,"voltage":"v1_30"}}"#;
    let decoded: ClockSelection = serde_json::from_str(valid).unwrap();
    assert_eq!(serde_json::to_string(&decoded).unwrap(), valid);
    for invalid in [
        r#"{"custom":{"cpu_hz":290000000,"spi_max_hz":72500000,"voltage":"v1_35"}}"#,
        r#"{"custom":{"cpu_hz":290000000,"spi_max_hz":72500000,"voltage":1.30}}"#,
        r#"{"custom":{"cpu_hz":290000000,"spi_max_hz":72500000,"voltage":"v1_30","extra":1}}"#,
        r#"{"named":"made_up"}"#,
        r#"{"custom":{"cpu_hz":4294967296,"spi_max_hz":72500000,"voltage":"v1_30"}}"#,
    ] {
        assert!(serde_json::from_str::<ClockSelection>(invalid).is_err(), "{invalid}");
    }

    let resolved = resolve(decoded).unwrap();
    let wire = serde_json::to_string(&resolved).unwrap();
    let roundtrip: dashboard_build_profiles::ResolvedClock = serde_json::from_str(&wire).unwrap();
    assert_eq!(roundtrip, resolved);
    let profile_wire = serde_json::to_string(&catalogue()[0]).unwrap();
    assert!(profile_wire.contains("cpu150-spi62-1v10"));
    assert_eq!(
        serde_json::to_string(&ClockError::NoExactPll).unwrap(),
        r#""no_exact_pll""#
    );
}
