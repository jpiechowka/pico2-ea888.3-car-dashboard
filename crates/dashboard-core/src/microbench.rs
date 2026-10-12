//! Bounded benchmark admission and arithmetic. No guessed zero-cost samples.
#[path = "microbench/workload.rs"]
pub mod workload;
pub const SUITE_US: u64 = 1_000_000;
pub const ITERATIONS: u32 = 8;
pub const OPERATIONS: u32 = 4 * ITERATIONS;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Clock = 1,
    Frequency = 2,
    CounterUnavailable = 3,
    CounterRange = 4,
    Timebase = 5,
    Restore = 6,
    Budget = 7,
    Context = 8,
    Watchdog = 9,
    Checksum = 10,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Counter {
    pub wall_us: u64,
    pub cycles: u32,
    pub control: u32,
    pub demcr: u32,
}
impl Counter {
    pub fn available(self) -> bool { self.control & ((1 << 25) | (1 << 23) | 1) == 1 && self.demcr & (1 << 24) != 0 }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Measurement {
    pub start_us: u64,
    pub end_us: u64,
    pub cycles: u32,
}
impl Measurement {
    pub fn check(
        start: Counter,
        end: Counter,
        cpu_hz: u32,
    ) -> Result<Self, Error> {
        if cpu_hz == 0 || cpu_hz > 300_000_000 {
            return Err(Error::Frequency);
        }
        if !start.available() || !end.available() {
            return Err(Error::CounterUnavailable);
        }
        let wall = end.wall_us.checked_sub(start.wall_us).ok_or(Error::Clock)?;
        if wall == 0 || wall > SUITE_US {
            return Err(Error::Clock);
        }
        let cycles = end.cycles.wrapping_sub(start.cycles);
        if cycles == 0 {
            return Err(Error::CounterUnavailable);
        }
        // The outer wall sample encloses both counter reads. One microsecond
        // tolerance covers timer granularity; the <=300MHz/1s bound excludes
        // multiple wraps when the checked, non-pausing timer keeps running.
        let maximum = ((wall + 1) * u64::from(cpu_hz)).div_ceil(1_000_000);
        if u64::from(cycles) > maximum {
            return Err(Error::CounterRange);
        }
        Ok(Self {
            start_us: start.wall_us,
            end_us: end.wall_us,
            cycles,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timebase {
    pub paused: bool,
    pub system_source: bool,
    pub tick_enabled: bool,
    pub tick_running: bool,
    pub tick_cycles: u32,
    pub reference_hz: u32,
    pub debug_pause: u32,
}
impl Timebase {
    fn valid(self) -> bool {
        !self.paused
            && !self.system_source
            && self.tick_enabled
            && self.tick_running
            && self.reference_hz >= 1_000_000
            && self.reference_hz.is_multiple_of(1_000_000)
            && self.tick_cycles == self.reference_hz / 1_000_000
    }
}
pub trait TimerPort {
    fn read(&self) -> Timebase;
    fn write_debug_pause(
        &mut self,
        value: u32,
    );
}

/// Restore the saved debug-pause word after every normal success/error return.
/// A panic or hardware fault still uses watchdog reset, not Rust unwinding.
pub fn with_running_timer<P: TimerPort, R>(
    port: &mut P,
    run: impl FnOnce(&mut P) -> Result<R, Error>,
) -> Result<R, Error> {
    let saved = port.read();
    if !saved.valid() {
        return Err(Error::Timebase);
    }
    let expected = Timebase {
        debug_pause: saved.debug_pause & !6,
        ..saved
    };
    port.write_debug_pause(expected.debug_pause);
    let mut result = if port.read() == expected {
        run(port)
    } else {
        Err(Error::Timebase)
    };
    if port.read() != expected {
        result = Err(Error::Timebase);
    }
    port.write_debug_pause(saved.debug_pause);
    if port.read().debug_pause != saved.debug_pause {
        return Err(Error::Restore);
    }
    result
}

pub struct Budget {
    origin: u64,
    last: u64,
    operations: u32,
    active: bool,
    closed: bool,
}
impl Budget {
    pub const fn new(now_us: u64) -> Self {
        Self {
            origin: now_us,
            last: now_us,
            operations: 0,
            active: false,
            closed: false,
        }
    }

    pub fn admit(
        &mut self,
        now_us: u64,
        operations: u32,
    ) -> Result<(), Error> {
        let result = (|| {
            if self.closed || self.active || operations == 0 {
                return Err(Error::Budget);
            }
            if now_us < self.last {
                return Err(Error::Clock);
            }
            if now_us - self.origin >= SUITE_US || operations > OPERATIONS - self.operations {
                return Err(Error::Budget);
            }
            self.operations += operations;
            self.active = true;
            self.last = now_us;
            Ok(())
        })();
        if result.is_err() {
            self.closed = true;
        }
        result
    }

    pub fn finish(
        &mut self,
        now_us: u64,
    ) -> Result<(), Error> {
        let result = if self.closed || !self.active {
            Err(Error::Budget)
        } else if now_us < self.last {
            Err(Error::Clock)
        } else if now_us - self.origin > SUITE_US {
            Err(Error::Budget)
        } else {
            Ok(())
        };
        self.active = false;
        if result.is_err() {
            self.closed = true;
        } else {
            self.last = now_us;
        }
        result
    }
}
