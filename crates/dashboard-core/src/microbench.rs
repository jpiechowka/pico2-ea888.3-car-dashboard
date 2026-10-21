//! Bounded benchmark admission and arithmetic. No guessed zero-cost samples.
#[path = "microbench/workload.rs"]
/// Deterministic production-code kernels with checksums evaluated outside timing.
pub mod workload;
/// Maximum elapsed suite duration in microseconds, including setup between operations.
pub const SUITE_US: u64 = 1_000_000;
/// Timed repetitions per production kernel.
pub const ITERATIONS: u32 = 8;
/// Maximum admitted operations across the four-kernel suite.
pub const OPERATIONS: u32 = 4 * ITERATIONS;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Stable benchmark rejection codes; failure never substitutes a zero-cost observation.
pub enum Error {
    /// Nonpositive, regressed or excessive enclosing wall interval.
    Clock = 1,
    /// CPU frequency unavailable or outside the supported `1..=300MHz` bound.
    Frequency = 2,
    /// DWT cycle counter inaccessible/disabled/prohibited or no cycle progress observed.
    CounterUnavailable = 3,
    /// Cycle delta exceeds the enclosing wall/frequency bound plus timer granularity.
    CounterRange = 4,
    /// Timer not independently running at 1MHz or configuration changed during measurement.
    Timebase = 5,
    /// Original timer debug-pause word could not be verified restored.
    Restore = 6,
    /// Suite time/operation bound or admit/finish state contract was violated.
    Budget = 7,
    /// Target adapter could not establish the required exclusive benchmark context.
    Context = 8,
    /// Target watchdog policy refused the benchmark operation.
    Watchdog = 9,
    /// Workload result was missing, rejected or differed from its expected checksum.
    Checksum = 10,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// DWT observation paired with an enclosing independent monotonic wall-clock sample.
pub struct Counter {
    /// Boot-relative timer microseconds, sampled outside the associated DWT read.
    pub wall_us: u64,
    /// Raw wrapping 32-bit DWT CYCCNT value.
    pub cycles: u32,
    /// Raw DWT CTRL: CYCCNTENA must be set and NOCYCCNT/CYCDISS clear.
    pub control: u32,
    /// Raw DEMCR; TRCENA bit24 must permit trace counters.
    pub demcr: u32,
}
impl Counter {
    /// Returns whether CTRL/DEMCR enable and permit CYCCNT. Does not itself prove
    /// counter progress; see the [Arm DWT register model](https://arm-software.github.io/CMSIS_6/v6.0.0/Core/structDWT__Type.html).
    pub fn available(self) -> bool { self.control & ((1 << 25) | (1 << 23) | 1) == 1 && self.demcr & (1 << 24) != 0 }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Admitted cycle delta within a positive, independently bounded wall-clock interval.
pub struct Measurement {
    /// Start of enclosing timer interval in microseconds.
    pub start_us: u64,
    /// End of enclosing timer interval in microseconds, strictly after start.
    pub end_us: u64,
    /// Positive wrapping CYCCNT difference proven within the supported wall/frequency bound.
    pub cycles: u32,
}
impl Measurement {
    /// Returns a measurement between `start` and `end` at actual `cpu_hz` in
    /// `1..=300_000_000`. Requires enabled counters, wall duration `1..=SUITE_US`
    /// and positive cycles no greater than ceil((wall+1us)*frequency). Returns
    /// Frequency/CounterUnavailable/Clock/CounterRange without mutating observations.
    /// Caller must establish a non-pausing independent timer and enclosing read order;
    /// those assumptions plus the bounds exclude multiple counter wraps.
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
/// Hardware timer admission snapshot; debug pause is modified/restored separately.
pub struct Timebase {
    /// Timer pause status; must be false.
    pub paused: bool,
    /// Timer uses system clock rather than the independent tick; must be false.
    pub system_source: bool,
    /// Independent tick generator is enabled.
    pub tick_enabled: bool,
    /// Tick generator reports running.
    pub tick_running: bool,
    /// Reference clock cycles per timer tick, exactly `reference_hz / 1_000_000`.
    pub tick_cycles: u32,
    /// Actual reference frequency in Hz, at least 1MHz and an exact MHz multiple.
    pub reference_hz: u32,
    /// Raw timer DBGPAUSE register, including unrelated bits preserved by the wrapper.
    pub debug_pause: u32,
}
impl Timebase {
    /// Returns whether this snapshot describes an unpaused independently sourced 1MHz timer.
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
/// Synchronous timer configuration adapter; caller owns exclusive register access for the wrapper duration.
pub trait TimerPort {
    /// Returns current timer/tick/reference/debug-pause evidence without modifying hardware.
    fn read(&self) -> Timebase;
    /// Writes raw debug-pause `value`, preserving no bits implicitly; subsequent readback verifies effect.
    fn write_debug_pause(
        &mut self,
        value: u32,
    );
}

/// Restore the saved debug-pause word after every normal success/error return.
/// A panic or hardware fault still uses watchdog reset, not Rust unwinding.
/// Validates `port`, clears its core-debug pause bits1/2, then calls `run` with
/// exclusive access only after exact readback. Returns `run`'s result unless timer
/// configuration drift produces Timebase or final pause-word restoration produces
/// Restore (which takes precedence). Invalid initial timebase performs no writes.
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

/// Fail-closed suite admission: at most 32 operations within one second, one active batch at a time.
pub struct Budget {
    /// Suite origin in monotonic microseconds.
    origin: u64,
    /// Last accepted admission/completion time in microseconds.
    last: u64,
    /// Cumulative admitted operations, including an unfinished batch.
    operations: u32,
    /// One admitted batch still awaits its finish observation.
    active: bool,
    /// Any previous budget/clock error permanently closed admission.
    closed: bool,
}
impl Budget {
    /// Returns an open, idle suite with microsecond origin `now_us` and zero operations.
    pub const fn new(now_us: u64) -> Self {
        Self {
            origin: now_us,
            last: now_us,
            operations: 0,
            active: false,
            closed: false,
        }
    }

    /// Reserves positive `operations` at monotonic `now_us` microseconds. Returns
    /// success only while idle/open, before the one-second deadline and within32
    /// total operations. Any failure permanently closes the suite; success advances
    /// its clock/count and marks the batch active without refunding on later failure.
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

    /// Completes the active batch at `now_us` microseconds, allowing equality with
    /// the one-second deadline. Returns Clock for regression or Budget for invalid
    /// lifecycle/late completion. Always clears active state; any error permanently
    /// closes admission, while success advances the last clock.
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
