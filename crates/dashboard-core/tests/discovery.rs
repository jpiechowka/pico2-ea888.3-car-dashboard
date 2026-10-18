use dashboard_core::discovery::*;

fn report(
    id: u8,
    kind: ReportKind,
    bytes: &[u8],
    at_ms: u64,
) -> Advertisement {
    Advertisement::new(
        Peer {
            address: [0xC0, 0, 0, 0, 0, id],
            random: true,
        },
        kind,
        -65,
        bytes,
        at_ms,
    )
}

#[test]
fn connectable_advertisement_and_response_keep_independent_payloads() {
    let mut catalog = Catalog::new();
    catalog.observe(report(1, ReportKind::ScanResponse, &[4, 9, b'B', b'L', b'E'], 10));
    assert!(
        catalog.devices().is_empty(),
        "a response alone cannot prove connectability"
    );
    catalog.observe(report(1, ReportKind::Connectable, &[2, 1, 6, 3, 3, 0x0A, 0x18], 11));
    catalog.observe(report(1, ReportKind::ScanResponse, &[4, 9, b'B', b'L', b'E'], 12));
    let device = &catalog.devices()[0];
    assert_eq!(device.name().as_str(), "BLE");
    assert_eq!(device.advertisement.as_slice(), &[2, 1, 6, 3, 3, 0x0A, 0x18]);
    assert_eq!(device.response.as_slice(), &[4, 9, b'B', b'L', b'E']);
    assert_eq!(device.last_seen_ms, 12);
    assert_eq!(device.reports, 2);
}

#[test]
fn scan_response_does_not_erase_unnamed_or_complete_advertisement_name() {
    let mut catalog = Catalog::new();
    catalog.observe(report(1, ReportKind::Connectable, &[5, 9, b'T', b'E', b'S', b'T'], 11));
    catalog.observe(report(1, ReportKind::ScanResponse, &[2, 8, b'T'], 12));
    assert_eq!(catalog.devices()[0].name().as_str(), "TEST");
}

#[test]
fn address_kind_is_part_of_identity_and_order_does_not_follow_rssi() {
    let mut catalog = Catalog::new();
    let mut a = report(1, ReportKind::Connectable, &[], 0);
    catalog.observe(a);
    a.peer.random = false;
    catalog.observe(a);
    a.rssi = -20;
    catalog.observe(a);
    assert_eq!(catalog.devices().len(), 2);
    assert!(catalog.devices()[0].peer.random);
    assert_eq!(catalog.devices()[1].rssi, Some(-20));
}

#[test]
fn capacity_keeps_old_records_and_counts_rejected_reports() {
    let mut catalog = Catalog::new();
    for id in 0..60 {
        catalog.observe(report(id, ReportKind::Connectable, &[], 100));
    }
    assert_eq!(catalog.devices().len(), 48);
    assert_eq!(catalog.overflow_reports, 12);
    assert_eq!(catalog.devices()[0].peer.address[5], 0);
    catalog.observe(report(0, ReportKind::Connectable, &[2, 1, 6], 200));
    assert_eq!(catalog.devices()[0].last_seen_ms, 200);
}

#[test]
fn malformed_ad_lengths_terminate_without_reading_past_end() {
    for bytes in [&[4, 9, b'A'][..], &[1][..], &[2, 1][..]] {
        let parsed = AdFields::new(bytes).collect::<Vec<_>>();
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].is_err());
    }
    assert_eq!(AdFields::new(&[0, 255, 255]).count(), 0, "zero terminates AD padding");
    let good = AdFields::new(&[2, 1, 6, 3, 3, 0x0A, 0x18]).collect::<Vec<_>>();
    assert_eq!(good[1].as_ref().unwrap().kind, 3);
    assert_eq!(good[1].as_ref().unwrap().data, &[0x0A, 0x18]);
}

#[test]
fn nonconnectable_reports_never_create_a_connectable_record() {
    let mut catalog = Catalog::new();
    catalog.observe(report(1, ReportKind::Other, &[], 0));
    assert!(catalog.devices().is_empty());
    catalog.observe(report(1, ReportKind::Directed, &[], 1));
    assert_eq!(catalog.devices().len(), 1);
}

#[test]
fn payload_truncation_rssi_unknown_and_time_reversal_are_explicit() {
    let mut catalog = Catalog::new();
    let mut a = report(1, ReportKind::Connectable, &[3; 40], 200);
    a.rssi = 127;
    catalog.observe(a);
    let device = &catalog.devices()[0];
    assert_eq!(device.rssi, None);
    assert!(device.advertisement.truncated);
    assert_eq!(device.age_ms(100), 0);
    catalog.observe(report(1, ReportKind::Connectable, &[], 100));
    assert_eq!(catalog.devices()[0].last_seen_ms, 200);
}

#[test]
fn untrusted_names_are_printable_and_bounded() {
    let mut catalog = Catalog::new();
    catalog.observe(report(1, ReportKind::Connectable, &[6, 9, b'A', 10, 0xFF, 0, b'Z'], 0));
    assert_eq!(catalog.devices()[0].name().as_str(), "A???Z");
    assert_eq!(
        report(1, ReportKind::Connectable, &[], 0).peer.to_string().as_str(),
        "C0:00:00:00:00:01"
    );
}

#[test]
fn click_freezes_selected_peer_and_back_invalidates_late_results() {
    let mut probe = Probe::new();
    probe.event(Event::Report {
        revision: 1,
        report: report(1, ReportKind::Connectable, &[], 100),
    });
    probe.event(Event::Report {
        revision: 1,
        report: report(2, ReportKind::Connectable, &[], 100),
    });
    probe.input(Input::Rotate(1), 100);
    let request = probe.input(Input::Click, 101).expect("click inspects");
    assert_eq!(
        request.action,
        Action::Inspect(report(2, ReportKind::Connectable, &[], 0).peer)
    );
    assert_eq!(probe.screen, Screen::Details);
    let back = probe.input(Input::Back, 102).expect("back resumes scan");
    assert!(back.revision > request.revision);
    probe.event(Event::State {
        revision: request.revision,
        phase: RadioPhase::Fault,
        error: ProbeError::Timeout,
    });
    assert_ne!(probe.phase, RadioPhase::Fault);
    assert_eq!(probe.screen, Screen::List);
}

#[test]
fn stale_peer_and_empty_list_do_not_initiate_connection() {
    let mut probe = Probe::new();
    assert!(probe.input(Input::Click, 100).is_none());
    probe.catalog.observe(report(1, ReportKind::Connectable, &[], 1));
    assert!(probe.input(Input::Click, 60_000).is_none());
    assert_eq!(probe.error, ProbeError::InvalidPeer);
}

#[test]
fn huge_encoder_steps_are_bounded_and_pause_does_not_clear_evidence() {
    let mut probe = Probe::new();
    for id in 0..5 {
        probe.catalog.observe(report(id, ReportKind::Connectable, &[], 0));
    }
    probe.input(Input::Rotate(i32::MAX), 1);
    assert_eq!(probe.selected, 4);
    probe.input(Input::Rotate(i32::MIN), 1);
    assert_eq!(probe.selected, 0);
    assert_eq!(probe.input(Input::Back, 1).unwrap().action, Action::Scan(false));
    assert_eq!(probe.catalog.devices().len(), 5);
    assert_eq!(probe.input(Input::Recover, 2).unwrap().action, Action::Scan(true));
    assert!(probe.catalog.devices().is_empty());
}

#[test]
fn vendor_or_wrong_service_values_are_not_allowlisted() {
    let dis = Uuid::from_wire(&[0x0A, 0x18]).unwrap();
    let fw = Uuid::from_wire(&[0x26, 0x2A]).unwrap();
    assert!(may_read(dis, fw, 2));
    assert!(!may_read(dis, fw, 8));
    assert!(!may_read(Uuid::from_wire(&[0xF0, 0xFF]).unwrap(), fw, 2));
    assert!(!may_read(dis, Uuid::from_wire(&[0xF1, 0xFF]).unwrap(), 2));
}

#[test]
fn bluetooth_base_uuid_aliases_preserve_read_allowlist_without_matching_vendor_uuids() {
    // 0000180a-0000-1000-8000-00805f9b34fb in ATT little-endian order.
    let mut base = [
        0xFB, 0x34, 0x9B, 0x5F, 0x80, 0, 0, 0x80, 0, 0x10, 0, 0, 0x0A, 0x18, 0, 0,
    ];
    let dis = Uuid::from_wire(&base).unwrap();
    assert_eq!(dis.short(), Some(0x180A));
    assert!(may_read(dis, Uuid::from_wire(&[0x26, 0x2A, 0, 0]).unwrap(), 2));
    base[0] ^= 1;
    assert_eq!(Uuid::from_wire(&base).unwrap().short(), None);
    assert_eq!(Uuid::from_wire(&[0x26, 0x2A, 1, 0]).unwrap().short(), None);
}

#[test]
fn invalid_gatt_ranges_and_capacity_cannot_appear_complete() {
    let mut inspection = Inspection::new();
    let uuid = Uuid::from_wire(&[0x0A, 0x18]).unwrap();
    inspection.accept(InspectionItem::Service(Service {
        start: 0,
        end: 10,
        uuid,
    }));
    assert_eq!(inspection.malformed, 1);
    for i in 0..30 {
        inspection.accept(InspectionItem::Service(Service {
            start: 1 + i * 10,
            end: 10 + i * 10,
            uuid,
        }));
    }
    assert_eq!(inspection.services.len(), 24);
    assert_eq!(inspection.omitted, 6);
    inspection.accept(InspectionItem::Characteristic(Characteristic {
        service_start: 1,
        declaration: 4,
        handle: 3,
        properties: 2,
        uuid,
    }));
    assert_eq!(inspection.malformed, 2);
}

#[test]
fn deterministic_advertisement_fuzz_is_bounded() {
    let mut seed = 12345_u32;
    for len in 0..=64 {
        for _ in 0..100 {
            let mut bytes = [0_u8; 64];
            for b in &mut bytes {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                *b = seed as u8;
            }
            assert!(AdFields::new(&bytes[..len]).count() <= len);
            let mut catalog = Catalog::new();
            catalog.observe(report(1, ReportKind::Connectable, &bytes[..len], 1));
            assert!(catalog.devices()[0].name().bytes().all(|b| (32..=126).contains(&b)));
        }
    }
}

#[test]
fn cancelled_inspection_cannot_publish_values_or_completion_after_recovery() {
    let mut probe = Probe::new();
    probe.catalog.observe(report(1, ReportKind::Connectable, &[], 10));
    let revision = probe.input(Input::Click, 10).unwrap().revision;
    probe.input(Input::Recover, 11);
    for event in [
        Event::Item {
            revision,
            item: InspectionItem::Value(InfoValue {
                uuid: 0x2A29,
                value: Payload::new(b"old"),
                result: ReadResult::Ok,
            }),
        },
        Event::Complete {
            revision,
            limited: false,
        },
        Event::Report {
            revision,
            report: report(2, ReportKind::Connectable, &[], 12),
        },
    ] {
        probe.event(event);
    }
    assert!(probe.inspection.values.is_empty());
    assert!(!probe.inspection.complete);
    assert!(probe.catalog.devices().is_empty());
}

#[test]
fn interrupted_and_capacity_limited_results_remain_partial() {
    let mut probe = Probe::new();
    probe.catalog.observe(report(1, ReportKind::Connectable, &[], 0));
    let revision = probe.input(Input::Click, 0).unwrap().revision;
    probe.event(Event::Item {
        revision,
        item: InspectionItem::Service(Service {
            start: 1,
            end: 10,
            uuid: Uuid::from_wire(&[0x0A, 0x18]).unwrap(),
        }),
    });
    probe.event(Event::State {
        revision,
        phase: RadioPhase::Paused,
        error: ProbeError::Timeout,
    });
    assert_eq!(probe.inspection.services.len(), 1);
    assert!(!probe.inspection.complete);
    probe.event(Event::Complete {
        revision,
        limited: true,
    });
    assert!(!probe.inspection.complete);
    assert_eq!(probe.inspection.omitted, 1);
}

#[test]
fn photo_pause_and_repeated_toggles_reject_queued_advertisements() {
    let mut probe = Probe::new();
    probe
        .catalog
        .observe(report(1, ReportKind::Connectable, &[2, 1, 6], 10));
    let paused = probe.input(Input::Details, 10).unwrap();
    assert_eq!(paused.action, Action::Scan(false));
    probe.event(Event::Report {
        revision: 1,
        report: report(1, ReportKind::Connectable, &[], 11),
    });
    assert_eq!(probe.selected_device().unwrap().last_seen_ms, 10);
    let resumed = probe.input(Input::Back, 12).unwrap();
    let paused_again = probe.input(Input::Back, 13).unwrap();
    assert_eq!(paused_again.action, Action::Scan(false));
    probe.event(Event::Report {
        revision: resumed.revision,
        report: report(2, ReportKind::Connectable, &[], 14),
    });
    assert_eq!(probe.catalog.devices().len(), 1);
}

#[test]
fn request_exhaustion_never_reuses_old_generation() {
    let mut probe = Probe::new();
    probe.request.revision = u32::MAX;
    assert!(probe.input(Input::Recover, 0).is_none());
    assert_eq!(probe.request.revision, u32::MAX);
    assert_eq!(probe.phase, RadioPhase::Fault);
}

#[test]
fn deterministic_control_and_report_interleavings_keep_bounded_identity() {
    let mut seed = 81231_u32;
    let mut probe = Probe::new();
    for now in 0..20_000_u64 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        match seed % 8 {
            0 => {
                probe.input(Input::Rotate(seed as i32), now);
            }
            1 => {
                probe.input(Input::Back, now);
            }
            2 => {
                probe.input(Input::Details, now);
            }
            3 => {
                probe.input(Input::Recover, now);
            }
            4 => {
                let selected = probe.selected_device().map(|d| d.peer);
                if let Some(Request {
                    action: Action::Inspect(peer),
                    ..
                }) = probe.input(Input::Click, now)
                {
                    assert_eq!(Some(peer), selected);
                    assert_eq!(probe.screen, Screen::Details);
                }
            }
            5 => {
                probe.event(Event::State {
                    revision: probe.request.revision,
                    phase: RadioPhase::Paused,
                    error: ProbeError::None,
                })
            }
            _ => {
                probe.event(Event::Report {
                    revision: probe.request.revision.saturating_sub(u32::from(seed & 16 != 0)),
                    report: report(seed as u8, ReportKind::Connectable, &[2, 1, 6], now),
                })
            }
        }
        assert!(probe.catalog.devices().len() <= 48);
        assert!(probe.catalog.devices().is_empty() || probe.selected < probe.catalog.devices().len());
        assert!(probe.page <= 4095);
    }
}

#[test]
fn useful_progress_monitors_cover_absent_encoder_idle_display_and_stalled_radio() {
    use dashboard_core::ble::runtime::Liveness;
    use dashboard_core::health::{Progress, ProgressMonitor};
    let mut current = Progress {
        acquisition: 0,
        rendering: 0,
        input: 0,
        completed: 0,
        submitted: 0,
        display_pending: true,
    };
    let mut owner = ProgressMonitor::new(0, current);
    let mut radio = Liveness::new(0);
    for now in (100..=2000).step_by(100) {
        current.rendering += 1;
        current.input += 1;
        // An absent encoder still completes one error/retry every 200 ms.
        current.acquisition += u32::from(now % 200 == 0);
        if now == 200 {
            current.display_pending = false;
            current.completed += 1;
        }
        assert!(owner.observe(now, current).is_ok());
        assert!(radio.observe(now as u32, now, false));
    }
    // A new transfer after idle gets its own deadline. Completed buffers alone
    // cannot mask a radio owner that has stopped reaching real bus boundaries.
    current.display_pending = true;
    for now in (2100..=2500).step_by(100) {
        current.rendering += 1;
        current.input += 1;
        current.acquisition += 1;
        if now == 2300 {
            current.display_pending = false;
            current.completed += 1;
        }
        assert!(owner.observe(now, current).is_ok());
        assert_eq!(radio.observe(2000, now, false), now < 2500);
    }
    assert!(
        !radio.observe(2001, 2501, false),
        "a late recovery must not cancel reset"
    );
}
