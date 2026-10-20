use dashboard_core::health::Fault;
use dashboard_core::health::reset::{BootEvidence, RecoveryCause, ResetRecord, ResetStage, boot_type_from_sys_info};

#[test]
fn hardware_reason_and_rom_boot_type_do_not_hide_unknown_watchdogs() {
    for raw in [1, 2, 3, 4, 0x8000_0000, u32::MAX] {
        for boot in [None, Some(0), Some(0x80), Some(1), Some(0xFF)] {
            let evidence = BootEvidence::capture(raw, boot, [0; 4]);
            assert!(evidence.watchdog_fault(), "raw={raw} boot={boot:?}");
            assert_eq!(evidence.raw_reason, raw);
            assert_eq!(evidence.boot_type, boot);
        }
    }
    for boot in [None, Some(0), Some(2), Some(0xFF)] {
        let evidence = BootEvidence::capture(0, boot, [0; 4]);
        assert!(!evidence.watchdog_fault());
        assert_eq!(evidence.label(), "OTHER");
    }
    for (raw, label) in [(1, "WD TIMEOUT"), (2, "WD FORCED"), (3, "WD BOTH"), (4, "WD UNKNOWN")] {
        assert_eq!(BootEvidence::capture(raw, Some(0), [0; 4]).label(), label);
    }
}

#[test]
fn only_documented_intentional_boot_types_normalize_watchdog_reason() {
    for boot in [2, 3, 4, 13, 0x82, 0x83, 0x84, 0x8D] {
        for raw in [1, 2, 3] {
            let evidence = BootEvidence::capture(raw, Some(boot), ResetRecord::booting().encode());
            assert!(!evidence.watchdog_fault());
            assert_eq!(evidence.label(), "ROM REBOOT");
            assert_eq!(evidence.previous, None);
        }
    }
    // A reserved hardware bit must remain visible even with a known ROM boot.
    assert!(BootEvidence::capture(4, Some(2), [0; 4]).watchdog_fault());
}

#[test]
fn normal_rom_reboot_can_look_like_a_watchdog_timeout_without_retained_context() {
    // picotool 2.3.1 NORMAL reboot uses delay=500 ms, param0=BOOT_PARTITION_NONE
    // (-1 as u32), param1=0. RP2350 A2 varm_reboot writes params to scratch2/3,
    // clears scratch4 and enables the timer. See the pinned sources in
    // docs/validation/probe-slot4-followup-2026-10-21.md. This is a source-derived
    // fixture, not a hardware reset simulation or evidence of a startup stall.
    for stage in [
        ResetStage::Booting,
        ResetStage::Supervised,
        ResetStage::Failure(RecoveryCause::Input),
    ] {
        let before = ResetRecord { stage, elapsed_ms: 931 };
        let mut scratch = before.encode();
        scratch[2] = u32::MAX;
        scratch[3] = 0;
        let after = BootEvidence::capture(1, Some(0), scratch);
        assert_eq!(after.raw_reason, 1);
        assert_eq!(after.boot_type, Some(0));
        assert_eq!(after.previous, None);
        assert_eq!(after.label(), "WD TIMEOUT");
        assert!(after.watchdog_fault());

        // The same metadata with an intact record must retain its observation.
        // Neither missing context nor NORMAL type is a reason to suppress a fault.
        assert_eq!(
            BootEvidence::capture(1, Some(0), before.encode()).previous,
            Some(before)
        );
    }
}

#[test]
fn rom_reply_requires_exact_supported_shape_and_preserves_chained_flag() {
    let words = [0x40, 0x0099_8255, 0, 0, 0];
    assert_eq!(boot_type_from_sys_info(5, words), Some(0x82));
    for status in [-19, -1, 0, 1, 4, 6] {
        assert_eq!(boot_type_from_sys_info(status, words), None);
    }
    for flags in [0, 0x20, 0x41, u32::MAX] {
        assert_eq!(boot_type_from_sys_info(5, [flags, 0x0200, 0, 0, 0]), None);
    }
}

#[test]
fn retained_record_roundtrips_all_stages_and_rejects_nonwatchdog_stale_words() {
    let mut stages = vec![ResetStage::Booting, ResetStage::Supervised];
    for code in 1..=12 {
        stages.push(ResetStage::Failure(RecoveryCause::from_code(code).unwrap()));
    }
    for stage in stages {
        for elapsed_ms in [0, 1, 15_000, u32::MAX] {
            let record = ResetRecord { stage, elapsed_ms };
            assert_eq!(ResetRecord::decode(record.encode()), Some(record));
            assert_eq!(
                BootEvidence::capture(1, Some(0), record.encode()).previous,
                Some(record)
            );
            assert_eq!(BootEvidence::capture(0, Some(0), record.encode()).previous, None);
        }
    }
}

#[test]
fn scratch_corruption_and_interrupted_publication_cannot_become_evidence() {
    let before = ResetRecord::booting();
    let after = ResetRecord {
        stage: ResetStage::Failure(RecoveryCause::Input),
        elapsed_ms: 931,
    };
    let encoded = after.encode();
    for word in 0..4 {
        for bit in 0..32 {
            let mut broken = encoded;
            broken[word] ^= 1 << bit;
            assert_eq!(ResetRecord::decode(broken), None, "word={word} bit={bit}");
        }
    }
    let mut scratch = before.encode();
    for (step, (index, value)) in after.scratch_writes().into_iter().enumerate() {
        scratch[index] = value;
        if step < 4 {
            assert_eq!(ResetRecord::decode(scratch), None);
        } else {
            assert_eq!(ResetRecord::decode(scratch), Some(after));
        }
    }
    assert_eq!(ResetRecord::decode([0; 4]), None);
    assert_eq!(ResetRecord::decode([u32::MAX; 4]), None);
    let mut rom_overwrite = encoded;
    rom_overwrite[2] = 0;
    rom_overwrite[3] = 0;
    assert_eq!(ResetRecord::decode(rom_overwrite), None);
}

#[test]
fn frozen_scratch_fixture_rejects_future_version_reserved_bits_and_unknown_stage() {
    // Independent CRC-32/ISO-HDLC fixtures, computed with Python zlib over the
    // three preceding little-endian words. A repaired CRC cannot admit new semantics.
    let valid = [0x5744_5231, 0x0100_0015, 931, 0x014A_4DC9];
    assert_eq!(
        ResetRecord::decode(valid),
        Some(ResetRecord {
            stage: ResetStage::Failure(RecoveryCause::Input),
            elapsed_ms: 931,
        })
    );
    for unknown in [
        [0x5744_5231, 0x0200_0015, 931, 0x46EA_3719],
        [0x5744_5231, 0x0100_0115, 931, 0xA73D_467D],
        [0x5744_5231, 0x0100_00FF, 931, 0x135A_7460],
    ] {
        assert_eq!(ResetRecord::decode(unknown), None);
    }
}

#[test]
fn every_raw_bit_and_unknown_boot_byte_remains_conservative() {
    for bit in 0..32 {
        assert!(BootEvidence::capture(1 << bit, None, [0; 4]).watchdog_fault());
    }
    for boot in 0..=255 {
        let fault = BootEvidence::capture(1, Some(boot), [0; 4]).watchdog_fault();
        assert_eq!(fault, ![2, 3, 4, 13, 130, 131, 132, 141].contains(&boot));
    }
}

#[test]
fn first_failure_survives_later_feeds_and_failure_reports() {
    let mut record = ResetRecord::booting();
    assert_eq!(record.stage, ResetStage::Booting);
    record.supervised(50);
    assert_eq!(
        record,
        ResetRecord {
            stage: ResetStage::Supervised,
            elapsed_ms: 50
        }
    );
    record.fail(RecoveryCause::Display, 95);
    record.supervised(110);
    record.fail(RecoveryCause::RadioFatal, 120);
    assert_eq!(
        record,
        ResetRecord {
            stage: ResetStage::Failure(RecoveryCause::Display),
            elapsed_ms: 95
        }
    );
    assert_eq!(ResetRecord::booting().stage, ResetStage::Booting);
    let mut saturated = ResetRecord::booting();
    saturated.supervised(u64::MAX);
    assert_eq!(saturated.elapsed_ms, u32::MAX);
    saturated.fail(RecoveryCause::Clock, u64::MAX);
    assert_eq!(saturated.elapsed_ms, u32::MAX);
}

#[test]
fn stable_codes_reject_future_values_and_map_every_monitor_fault() {
    for (code, cause) in [
        (1, RecoveryCause::Clock),
        (2, RecoveryCause::Supervisor),
        (3, RecoveryCause::Acquisition),
        (4, RecoveryCause::Rendering),
        (5, RecoveryCause::Input),
        (6, RecoveryCause::Display),
        (7, RecoveryCause::Maintenance),
        (8, RecoveryCause::RadioFatal),
        (9, RecoveryCause::RadioStalled),
        (10, RecoveryCause::RadioStartup),
        (11, RecoveryCause::DisplayFatal),
        (12, RecoveryCause::RecoveryRequested),
    ] {
        assert_eq!(RecoveryCause::from_code(code), Some(cause));
        assert_eq!(cause.code(), code);
    }
    let faults = [
        (Fault::Clock, RecoveryCause::Clock),
        (Fault::Supervisor, RecoveryCause::Supervisor),
        (Fault::Acquisition, RecoveryCause::Acquisition),
        (Fault::Rendering, RecoveryCause::Rendering),
        (Fault::Input, RecoveryCause::Input),
        (Fault::Display, RecoveryCause::Display),
        (Fault::Maintenance, RecoveryCause::Maintenance),
    ];
    for (fault, cause) in faults {
        assert_eq!(RecoveryCause::from(fault), cause);
    }
    for code in 0..=255 {
        let cause = RecoveryCause::from_code(code);
        assert_eq!(cause.is_some(), (1..=12).contains(&code));
        if let Some(cause) = cause {
            assert_eq!(cause.code(), code);
        }
        let stage = ResetStage::from_code(code);
        assert_eq!(stage.is_some(), matches!(code, 1 | 2 | 17..=28));
        if let Some(stage) = stage {
            assert_eq!(stage.code(), code);
        }
    }
}
