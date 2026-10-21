//! Optional RTC match fields; absent fields are wildcards, not disabled alarms.
use super::DayOfWeek;
use crate::pac::rtc::regs::{IrqSetup0, IrqSetup1};

/// Conjunctive field filter for [`super::Rtc::schedule_alarm`]; None fields are ignored.
/// Setters do not validate ranges. All-None matches without any calendar/time restriction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct DateTimeFilter {
    /// Required year (0..=4095); None ignores this field.
    pub year: Option<u16>,
    /// Required month (1..=12); None ignores this field.
    pub month: Option<u8>,
    /// Required day (1..=31); None ignores this field.
    pub day: Option<u8>,
    /// Required weekday; None ignores this field.
    pub day_of_week: Option<DayOfWeek>,
    /// Required hour (0..=23); None ignores this field.
    pub hour: Option<u8>,
    /// Required minute (0..=59); None ignores this field.
    pub minute: Option<u8>,
    /// Required second (0..=59); None ignores this field.
    pub second: Option<u8>,
}

impl DateTimeFilter {
    /// Returns this filter with `year` selected; expected range 0..=4095 is not checked.
    pub fn year(
        mut self,
        year: u16,
    ) -> Self {
        self.year = Some(year);
        self
    }

    /// Returns this filter with `month` selected; expected range 1..=12 is not checked.
    pub fn month(
        mut self,
        month: u8,
    ) -> Self {
        self.month = Some(month);
        self
    }

    /// Returns this filter with `day` selected; expected range 1..=31 is not checked.
    pub fn day(
        mut self,
        day: u8,
    ) -> Self {
        self.day = Some(day);
        self
    }

    /// Returns this filter with `day_of_week` selected, preserving other fields.
    pub fn day_of_week(
        mut self,
        day_of_week: DayOfWeek,
    ) -> Self {
        self.day_of_week = Some(day_of_week);
        self
    }

    /// Returns this filter with `hour` selected; expected range 0..=23 is not checked.
    pub fn hour(
        mut self,
        hour: u8,
    ) -> Self {
        self.hour = Some(hour);
        self
    }

    /// Returns this filter with `minute` selected; expected range 0..=59 is not checked.
    pub fn minute(
        mut self,
        minute: u8,
    ) -> Self {
        self.minute = Some(minute);
        self
    }

    /// Returns this filter with `second` selected; expected range 0..=59 is not checked.
    pub fn second(
        mut self,
        second: u8,
    ) -> Self {
        self.second = Some(second);
        self
    }
}

// register helper functions
impl DateTimeFilter {
    /// Enables/writes selected calendar fields in `w`; absent fields leave existing bits unchanged.
    pub(super) fn write_setup_0(
        &self,
        w: &mut IrqSetup0,
    ) {
        if let Some(year) = self.year {
            w.set_year_ena(true);

            w.set_year(year);
        }
        if let Some(month) = self.month {
            w.set_month_ena(true);
            w.set_month(month);
        }
        if let Some(day) = self.day {
            w.set_day_ena(true);
            w.set_day(day);
        }
    }

    /// Enables/writes selected weekday/time fields in `w`; absent fields leave existing bits unchanged.
    pub(super) fn write_setup_1(
        &self,
        w: &mut IrqSetup1,
    ) {
        if let Some(day_of_week) = self.day_of_week {
            w.set_dotw_ena(true);
            let bits = super::conversions::day_of_week_to_u8(day_of_week);

            w.set_dotw(bits);
        }
        if let Some(hour) = self.hour {
            w.set_hour_ena(true);
            w.set_hour(hour);
        }
        if let Some(minute) = self.minute {
            w.set_min_ena(true);
            w.set_min(minute);
        }
        if let Some(second) = self.second {
            w.set_sec_ena(true);
            w.set_sec(second);
        }
    }
}
