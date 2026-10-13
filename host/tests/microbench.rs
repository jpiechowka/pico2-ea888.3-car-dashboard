use dashboard_host_validation::microbench::{self, Budget, Counter, Error, Measurement, SUITE_US, Timebase, TimerPort};

fn timer() -> Timebase {
    Timebase {
        paused: false,
        system_source: false,
        tick_enabled: true,
        tick_running: true,
        tick_cycles: 12,
        reference_hz: 12_000_000,
        debug_pause: 6,
    }
}
struct Port {
    state: Timebase,
    writes: Vec<u32>,
    fail_clear: bool,
    fail_restore: bool,
}
impl TimerPort for Port {
    fn read(&self) -> Timebase { self.state }

    fn write_debug_pause(
        &mut self,
        value: u32,
    ) {
        self.writes.push(value);
        if !(self.fail_clear && value == 0 || self.fail_restore && value == 6) {
            self.state.debug_pause = value;
        }
    }
}
fn port() -> Port {
    Port {
        state: timer(),
        writes: vec![],
        fail_clear: false,
        fail_restore: false,
    }
}
fn counter(
    us: u64,
    cycles: u32,
) -> Counter {
    Counter {
        wall_us: us,
        cycles,
        control: 1,
        demcr: 1 << 24,
    }
}

#[test]
fn raw_cycle_wrap_is_valid_but_missing_or_impossible_counters_are_explicit_errors() {
    assert_eq!(
        Measurement::check(counter(10, 20), counter(20, 2020), 250_000_000)
            .unwrap()
            .cycles,
        2000
    );
    assert_eq!(
        Measurement::check(counter(10, u32::MAX - 99), counter(11, 100), 300_000_000)
            .unwrap()
            .cycles,
        200
    );
    for (a, b, hz) in [
        (counter(1, 1), counter(1, 2), 150_000_000),
        (counter(2, 1), counter(1, 2), 150_000_000),
        (counter(0, 1), counter(SUITE_US + 1, 2), 150_000_000),
        (counter(0, 1), counter(1, 1), 150_000_000),
        (counter(0, 1), counter(1, 10000), 150_000_000),
        (counter(0, 1), counter(1, 2), 0),
        (counter(0, 1), counter(1, 2), 300_000_001),
    ] {
        assert!(Measurement::check(a, b, hz).is_err());
    }
    for mask in [1 << 25, 1 << 23] {
        let mut a = counter(10, 1);
        a.control |= mask;
        assert_eq!(
            Measurement::check(a, counter(20, 100), 150_000_000),
            Err(Error::CounterUnavailable)
        );
    }
    let mut a = counter(0, 1);
    a.control = 0;
    assert_eq!(
        Measurement::check(a, counter(1, 100), 150_000_000),
        Err(Error::CounterUnavailable)
    );
    a = counter(0, 1);
    a.demcr = 0;
    assert_eq!(
        Measurement::check(a, counter(1, 100), 150_000_000),
        Err(Error::CounterUnavailable)
    );
    assert!(Measurement::check(counter(u64::MAX - SUITE_US, 1), counter(u64::MAX, 100), 150_000_000).is_ok());
}

#[test]
fn every_normal_exit_restores_debug_pause_and_failed_timebases_never_run() {
    for fail in [false, true] {
        let mut p = port();
        let r = microbench::with_running_timer(&mut p, |p| {
            assert_eq!(p.read().debug_pause, 0);
            if fail { Err(Error::Checksum) } else { Ok(42) }
        });
        assert_eq!(p.state, timer());
        assert_eq!(p.writes, vec![0, 6]);
        assert_eq!(r, if fail { Err(Error::Checksum) } else { Ok(42) });
    }
    for field in 0..7 {
        let mut p = port();
        match field {
            0 => p.state.paused = true,
            1 => p.state.system_source = true,
            2 => p.state.tick_enabled = false,
            3 => p.state.tick_running = false,
            4 => p.state.tick_cycles = 13,
            5 => p.state.reference_hz = 0,
            _ => p.state.reference_hz = 12_000_001,
        }
        assert_eq!(
            microbench::with_running_timer(&mut p, |_| -> Result<(), Error> { panic!("invalid timer ran") }),
            Err(Error::Timebase)
        );
        assert!(p.writes.is_empty());
    }
    let mut p = port();
    p.fail_clear = true;
    assert_eq!(
        microbench::with_running_timer(&mut p, |_| -> Result<(), Error> { panic!("failed clear ran") }),
        Err(Error::Timebase)
    );
    assert_eq!(p.writes, vec![0, 6]);
    let mut p = port();
    p.fail_restore = true;
    assert_eq!(microbench::with_running_timer(&mut p, |_| Ok(42)), Err(Error::Restore));
    let mut p = port();
    assert_eq!(
        microbench::with_running_timer(&mut p, |p| {
            p.state.tick_cycles = 7;
            Ok(42)
        }),
        Err(Error::Timebase)
    );
    assert_eq!(p.state.debug_pause, 6);
}

#[test]
fn total_budget_operation_cap_and_regressed_clocks_close_admission() {
    let mut b = Budget::new(10);
    for n in 0..4 {
        b.admit(10 + n * 10, 8).unwrap();
        b.finish(11 + n * 10).unwrap();
    }
    assert_eq!(b.admit(100, 1), Err(Error::Budget));
    let mut b = Budget::new(0);
    b.admit(0, 1).unwrap();
    b.finish(SUITE_US).unwrap();
    assert_eq!(b.admit(SUITE_US, 1), Err(Error::Budget));
    let mut b = Budget::new(10);
    assert_eq!(b.admit(9, 1), Err(Error::Clock));
    assert_eq!(b.admit(11, 1), Err(Error::Budget));
    let mut b = Budget::new(0);
    b.admit(0, 1).unwrap();
    assert_eq!(b.finish(SUITE_US + 1), Err(Error::Budget));
    let mut b = Budget::new(0);
    b.admit(0, 1).unwrap();
    assert_eq!(b.admit(0, 1), Err(Error::Budget));
    assert_eq!(Budget::new(0).admit(0, 0), Err(Error::Budget));
}

#[test]
fn target_workloads_match_reference_pixels_and_history_after_every_bounded_batch() {
    use dashboard_host_validation::display::FrameBuffer;
    use dashboard_host_validation::microbench::workload::{Fixture, Kernel};
    use dashboard_host_validation::widgets::draw_boost_cell;
    use dashboard_host_validation::widgets::primitives::draw_mini_graph;
    use embedded_graphics::pixelcolor::Rgb565;
    use embedded_graphics::prelude::*;
    use embedded_graphics::primitives::Rectangle;
    struct Reference(Vec<u16>);
    impl OriginDimensions for Reference {
        fn size(&self) -> Size { Size::new(320, 240) }
    }
    impl DrawTarget for Reference {
        type Color = Rgb565;
        type Error = core::convert::Infallible;

        fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
            &mut self,
            pixels: I,
        ) -> Result<(), Self::Error> {
            for Pixel(p, c) in pixels {
                if (0..320).contains(&p.x) && (0..240).contains(&p.y) {
                    self.0[p.y as usize * 320 + p.x as usize] = c.into_storage();
                }
            }
            Ok(())
        }
    }
    let mut checksums = Vec::new();
    for kind in Kernel::ALL {
        let mut frame = Box::new(FrameBuffer::new());
        let mut f = Fixture::new();
        let mut reference = Reference(vec![0; 320 * 240]);
        for _ in 0..microbench::ITERATIONS {
            f.run(kind, &mut frame);
        }
        match kind {
            Kernel::Clear => reference.clear(Rgb565::new(3, 17, 29)).unwrap(),
            Kernel::Clipped => {
                reference
                    .fill_contiguous(
                        &Rectangle::new(Point::new(-3, 221), Size::new(23, 27)),
                        (0..23 * 27).map(|i| Rgb565::new((i % 32) as u8, ((i / 5) % 64) as u8, ((i / 11) % 32) as u8)),
                    )
                    .unwrap()
            }
            Kernel::GraphWidget => {
                let mut graph = core::array::from_fn::<_, 60, _>(|i| (i % 11) as f32 * 0.25);
                graph[16] = 8.0;
                draw_mini_graph(&mut reference, 160, 184, 155, 55, &graph, 60, 17, 60, 0.0, 8.0, |_| {
                    Rgb565::WHITE
                });
                draw_boost_cell(&mut reference, 0, 0, 160, 110, 1.6, 2.7, true, false, true, 0);
                assert_eq!(reference.0[186 * 320 + 312], Rgb565::WHITE.into_storage());
            }
            Kernel::History => {
                let s = f.snapshot();
                assert_eq!(s.value(), Some(-8.0));
                assert_eq!((s.minimum(), s.maximum()), (Some(-9.0), Some(9.0)));
                let expected: Vec<f32> = (512..572).map(|i| (i % 19) as f32 - 9.0).collect();
                assert_eq!(s.get_average(), Some(expected.iter().sum::<f32>() / 60.0));
                let history = s.history();
                assert_eq!((history.count(), history.bounds()), (24, Some((-9.0, 9.0))));
                let expected: Vec<_> = (548..572).map(|i| (i % 19) as f32 - 9.0).collect();
                assert_eq!(history.iter().filter_map(|(_, v)| v).collect::<Vec<_>>(), expected);
            }
        }
        for (pixel, expected) in frame.as_chunks::<2>().0.iter().zip(reference.0) {
            assert_eq!(u16::from_be_bytes([pixel[0], pixel[1]]), expected);
        }
        let checksum = f.checksum(kind, &frame).unwrap();
        checksums.push(checksum);
    }
    assert_eq!(checksums, Kernel::ALL.map(Kernel::expected_checksum));
}
