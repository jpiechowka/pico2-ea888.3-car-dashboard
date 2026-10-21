//! Build environment admission and generated clock/sidecar consistency.

use std::ffi::OsString;

use dashboard_build_profiles::{ClockSelection, ProfileId, catalogue};
use dashboard_config_tools::clocks::{custom_clock, generate, selection};

/// Partial triples and non-decimal values fail; all typed voltages and full triples parse.
#[test]
fn all_presence_subsets_and_invalid_decimal_inputs_fail_closed() {
    for mask in 0..8 {
        let raw = std::array::from_fn(|i| {
            (mask & (1 << i) != 0).then(|| OsString::from(["50000000", "25000000", "1100"][i]))
        });
        assert_eq!(custom_clock(&raw).is_ok(), mask == 0 || mask == 7, "mask {mask}");
    }
    for key in 0..3 {
        for bad in [
            "",
            "+1",
            "-1",
            "1.0",
            "1e3",
            "0x10",
            "1_000",
            " 1100",
            "1100\n",
            "１",
            "4294967296",
        ] {
            let mut raw = ["50000000", "25000000", "1100"].map(|s| Some(OsString::from(s)));
            raw[key] = Some(bad.into());
            assert!(custom_clock(&raw).is_err(), "key {key} {bad:?}");
        }
    }
    for mv in [1100, 1150, 1200, 1250, 1300] {
        assert!(
            custom_clock(&[
                Some("50000000".into()),
                Some("25000000".into()),
                Some(mv.to_string().into())
            ])
            .is_ok()
        );
    }
    for mv in [1050, 1350, 1111] {
        assert!(
            custom_clock(&[
                Some("50000000".into()),
                Some("25000000".into()),
                Some(mv.to_string().into())
            ])
            .is_err()
        );
    }
    for p in catalogue() {
        let features = vec![p.feature.to_owned()];
        for key in 0..3 {
            let mut raw = [None, None, None];
            raw[key] = Some("".into());
            assert!(selection(&features, &raw).unwrap_err().contains("named profile"));
        }
    }
}

/// Generated literals and metadata match an independent PLL/SPI table for all profiles.
#[test]
fn literal_generation_matches_independent_pll_and_spi_fixtures() {
    let cases = [
        (ProfileId::Stock, 150000000, 37500000, (1, 125, 5, 2), (2, 1)),
        (ProfileId::Cpu132Spi66, 132000000, 66000000, (1, 132, 6, 2), (2, 0)),
        (ProfileId::Cpu140Spi70, 140000000, 70000000, (1, 105, 3, 3), (2, 0)),
        (ProfileId::Timing150Spi15, 150000000, 15000000, (1, 125, 5, 2), (2, 4)),
        (ProfileId::Cpu250Spi62, 250000000, 62500000, (1, 125, 6, 1), (2, 1)),
        (ProfileId::Cpu280Spi70, 280000000, 70000000, (1, 70, 3, 1), (2, 1)),
        (ProfileId::Legacy290Spi72, 290400000, 48400000, (1, 121, 5, 1), (2, 2)),
        (ProfileId::Cpu300Spi75, 300000000, 75000000, (1, 125, 5, 1), (2, 1)),
    ];
    for (id, cpu, spi, (refdiv, fbdiv, post1, post2), (cps, scr)) in cases {
        let (source, metadata) = generate(ClockSelection::Named(id), "3", vec!["demo".into()]).unwrap();
        let r: serde_json::Value = serde_json::from_slice(&metadata).unwrap();
        assert_eq!(r["clock"]["actual_cpu_hz"], cpu);
        assert_eq!(r["clock"]["actual_spi_hz"], spi);
        assert_eq!(
            r["clock"]["pll"],
            serde_json::json!({"refdiv":refdiv,"fbdiv":fbdiv,"postdiv1":post1,"postdiv2":post2})
        );
        assert_eq!(r["clock"]["cpsdvsr"], cps);
        assert_eq!(r["clock"]["scr"], scr);
        let source = String::from_utf8(source).unwrap();
        assert!(source.contains(&format!("ACTUAL_CPU_HZ: u32 = {cpu};")));
        assert!(source.contains(&format!("post_div1: {post1}, post_div2: {post2}")));
        assert_eq!(u64::from(12_000_000u32 / refdiv) * fbdiv / (post1 * post2) as u64, cpu);
        assert_eq!(cpu / (cps * (scr + 1)), spi);
    }
}

/// Every pair of distinct clock features conflicts; absent or explicit stock agrees.
#[test]
fn pair_conflicts_and_stock_default_identity_are_exact() {
    let rows = catalogue();
    for (i, a) in rows.iter().enumerate() {
        for b in &rows[i + 1..] {
            assert!(
                selection(&[a.feature.into(), b.feature.into()], &[None, None, None])
                    .unwrap_err()
                    .contains("Only one")
            );
        }
    }
    assert_eq!(
        selection(&[], &[None, None, None]).unwrap(),
        ClockSelection::Named(ProfileId::Stock)
    );
    assert_eq!(
        selection(&[rows[0].feature.into()], &[None, None, None]).unwrap(),
        ClockSelection::Named(ProfileId::Stock)
    );
}

/// Non-Unicode environment bytes fail explicitly rather than silently selecting defaults.
#[cfg(unix)]
#[test]
fn non_unicode_clock_is_not_ignored() {
    use std::os::unix::ffi::OsStringExt;
    assert!(
        custom_clock(&[
            Some(OsString::from_vec(vec![255])),
            Some("25000000".into()),
            Some("1100".into())
        ])
        .is_err()
    );
}
