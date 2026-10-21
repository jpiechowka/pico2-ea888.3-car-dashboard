//! Unix epoch conversion (milliseconds since 1970-01-01 00:00:00 UTC)

#[cfg(any(not(feature = "chrono"), feature = "_rp235x"))]
/// First supported Unix calendar year.
pub(crate) const EPOCH_YEAR: u16 = 1970;

#[cfg(not(feature = "chrono"))]
/// Whole-second Gregorian conversion without Chrono.
mod no_deps {
    use super::super::{DateTime, DayOfWeek, Error};
    use super::EPOCH_YEAR;
    /// Common-year month lengths indexed by month-1.
    const DAYS_IN_MONTH: [u8; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    /// Milliseconds per civil second.
    const MS_PER_SECOND: u64 = 1000;
    /// Milliseconds per civil minute, excluding leap-second extensions.
    const MS_PER_MINUTE: u64 = 60 * MS_PER_SECOND;
    /// Milliseconds per civil hour.
    const MS_PER_HOUR: u64 = 60 * MS_PER_MINUTE;
    /// Milliseconds per 24-hour Unix calendar day.
    const MS_PER_DAY: u64 = 24 * MS_PER_HOUR;
    /// Sunday-zero weekday code for 1970-01-01 (Thursday).
    const EPOCH_DAY_OF_WEEK: u8 = 4; // Thursday

    /// Returns whether Gregorian `year` adds February 29, including century exceptions.
    const fn is_leap_year(year: u16) -> bool { (year % 4 == 0) && ((year % 100 != 0) || (year % 400 == 0)) }

    /// Returns the day count for Gregorian `year` and one-based `month`.
    /// Requires month 1..=12; invalid month can underflow or panic on indexing.
    fn days_in_month(
        year: u16,
        month: u8,
    ) -> u8 {
        if month == 2 && is_leap_year(year) {
            29
        } else {
            DAYS_IN_MONTH[(month - 1) as usize]
        }
    }

    /// Returns days before January 1 of `year`; years<=1970 produce zero.
    /// Linear in elapsed years; all u16 years fit the u32 accumulator.
    const fn days_from_epoch_to_year(year: u16) -> u32 {
        let mut days = 0u32;
        let mut y = EPOCH_YEAR;
        while y < year {
            days += if is_leap_year(y) { 366 } else { 365 };
            y += 1;
        }
        days
    }

    /// Returns Sunday-zero weekday for `days_since_epoch`; input must leave room for adding 4.
    fn day_of_week_from_days(days_since_epoch: u32) -> u8 { ((days_since_epoch + EPOCH_DAY_OF_WEEK as u32) % 7) as u8 }

    /// Returns Unix milliseconds for `dt` after validating supported year/calendar/time fields.
    /// Treats fields as UTC; no-dependency mode ignores the supplied weekday.
    pub(crate) fn datetime_to_millis(dt: &DateTime) -> Result<u64, Error> {
        if dt.year > 4095 {
            return Err(Error::InvalidYear);
        }
        if dt.year < EPOCH_YEAR {
            return Err(Error::OutOfRange);
        }
        if !(1..=12).contains(&dt.month) {
            return Err(Error::InvalidMonth);
        }
        if dt.day == 0 || dt.day > days_in_month(dt.year, dt.month) {
            return Err(Error::InvalidDay);
        }
        if dt.hour > 23 {
            return Err(Error::InvalidHour);
        }
        if dt.minute > 59 {
            return Err(Error::InvalidMinute);
        }
        if dt.second > 59 {
            return Err(Error::InvalidSecond);
        }

        let mut total_days = days_from_epoch_to_year(dt.year);
        for m in 1..dt.month {
            total_days += days_in_month(dt.year, m) as u32;
        }
        total_days += (dt.day - 1) as u32;

        // Convert to u64 only for final millisecond calculation
        let mut millis = total_days as u64 * MS_PER_DAY;
        millis += dt.hour as u64 * MS_PER_HOUR;
        millis += dt.minute as u64 * MS_PER_MINUTE;
        millis += dt.second as u64 * MS_PER_SECOND;

        Ok(millis)
    }

    #[cfg(not(feature = "chrono"))]
    /// Returns calendar fields for UTC epoch `millis`, or an invalid-timestamp/year error.
    /// Non-Chrono mode drops subsecond remainder; Chrono retains milliseconds.
    pub(crate) fn millis_to_datetime(millis: u64) -> Result<DateTime, Error> {
        /// Exclusive supported date bound at 4096-01-01, checked before narrowing the day count.
        const END_DAYS: u64 = days_from_epoch_to_year(4096) as u64;
        if millis / MS_PER_DAY >= END_DAYS {
            return Err(Error::InvalidTimestamp);
        }
        // Only the supported calendar range is narrowed to u32.
        let total_days = (millis / MS_PER_DAY) as u32;
        // remaining_ms is at most MS_PER_DAY - 1 = 86,399,999, fits in u32
        let remaining_ms = (millis % MS_PER_DAY) as u32;

        let hour = (remaining_ms / MS_PER_HOUR as u32) as u8;
        let remaining_ms = remaining_ms % MS_PER_HOUR as u32;
        let minute = (remaining_ms / MS_PER_MINUTE as u32) as u8;
        let second = ((remaining_ms % MS_PER_MINUTE as u32) / MS_PER_SECOND as u32) as u8;

        let day_of_week = match day_of_week_from_days(total_days) {
            0 => DayOfWeek::Sunday,
            1 => DayOfWeek::Monday,
            2 => DayOfWeek::Tuesday,
            3 => DayOfWeek::Wednesday,
            4 => DayOfWeek::Thursday,
            5 => DayOfWeek::Friday,
            6 => DayOfWeek::Saturday,
            _ => unreachable!(),
        };

        let mut year = EPOCH_YEAR;
        let mut days_remaining = total_days;

        loop {
            let days_in_year: u32 = if is_leap_year(year) { 366 } else { 365 };
            if days_remaining < days_in_year {
                break;
            }
            days_remaining -= days_in_year;
            year += 1;

            if year > 4095 {
                return Err(Error::InvalidTimestamp);
            }
        }

        let mut month = 1u8;
        while month <= 12 {
            let days_in_this_month = days_in_month(year, month) as u32;
            if days_remaining < days_in_this_month {
                break;
            }
            days_remaining -= days_in_this_month;
            month += 1;
        }
        let day = (days_remaining + 1) as u8;

        Ok(DateTime {
            year,
            month,
            day,
            day_of_week,
            hour,
            minute,
            second,
        })
    }
}

#[cfg(not(feature = "chrono"))]
pub(crate) use no_deps::{datetime_to_millis, millis_to_datetime};

#[cfg(all(feature = "chrono", feature = "_rp235x"))]
/// Chrono-based conversion preserving millisecond precision for RP235x timers.
mod chrono_rp235x {
    use super::super::{DateTime, Error};
    use super::EPOCH_YEAR;

    /// Returns Unix milliseconds for `dt` after validating supported year/calendar/time fields.
    /// Treats fields as UTC; no-dependency mode ignores the supplied weekday.
    pub(crate) fn datetime_to_millis(dt: &DateTime) -> Result<u64, Error> {
        use chrono::Datelike;

        if dt.year() < EPOCH_YEAR as i32 {
            return Err(Error::OutOfRange);
        }
        if dt.year() > 4095 {
            return Err(Error::InvalidYear);
        }

        let timestamp_millis = dt.and_utc().timestamp_millis();
        if timestamp_millis < 0 {
            return Err(Error::OutOfRange);
        }

        Ok(timestamp_millis as u64)
    }

    /// Returns calendar fields for UTC epoch `millis`, or an invalid-timestamp/year error.
    /// Non-Chrono mode drops subsecond remainder; Chrono retains milliseconds.
    pub(crate) fn millis_to_datetime(millis: u64) -> Result<DateTime, Error> {
        use chrono::Datelike;

        let millis_i64 = millis.try_into().map_err(|_| Error::InvalidTimestamp)?;
        let dt = chrono::DateTime::from_timestamp_millis(millis_i64)
            .ok_or(Error::InvalidTimestamp)?
            .naive_utc();

        if dt.year() < 0 || dt.year() > 4095 {
            return Err(Error::InvalidYear);
        }

        Ok(dt)
    }
}

#[cfg(all(feature = "chrono", feature = "_rp235x"))]
pub(crate) use chrono_rp235x::{datetime_to_millis, millis_to_datetime};

#[cfg(all(test, not(feature = "chrono")))]
/// Boundary/calendar regressions for the independent integer conversion implementation.
mod tests {
    use super::*;
    use crate::datetime::{DateTime, DayOfWeek, Error};

    /// Returns 1970-01-01 Thursday midnight as a valid mutation baseline.
    fn epoch() -> DateTime {
        DateTime {
            year: 1970,
            month: 1,
            day: 1,
            day_of_week: DayOfWeek::Thursday,
            hour: 0,
            minute: 0,
            second: 0,
        }
    }

    #[test]
    /// Rejects bad month/day/time fields, including non-leap century February 29.
    fn invalid_calendar_fields_return_errors() {
        for month in [0, 13, 255] {
            let mut dt = epoch();
            dt.month = month;
            assert_eq!(datetime_to_millis(&dt), Err(Error::InvalidMonth));
        }
        for (year, month, day) in [(1970, 1, 0), (2023, 2, 29), (2100, 2, 29), (2024, 4, 31)] {
            let mut dt = epoch();
            dt.year = year;
            dt.month = month;
            dt.day = day;
            assert_eq!(datetime_to_millis(&dt), Err(Error::InvalidDay));
        }
        for (hour, minute, second, error) in [
            (24, 0, 0, Error::InvalidHour),
            (0, 60, 0, Error::InvalidMinute),
            (0, 0, 60, Error::InvalidSecond),
        ] {
            let mut dt = epoch();
            dt.hour = hour;
            dt.minute = minute;
            dt.second = second;
            assert_eq!(datetime_to_millis(&dt), Err(error));
        }
    }

    #[test]
    /// Large timestamp/day counts cannot wrap into an accepted date; years stay bounded.
    fn timestamp_bounds_are_checked_before_narrowing() {
        for millis in [u64::MAX, (1u64 << 32) * 86_400_000] {
            assert!(millis_to_datetime(millis).is_err());
        }
        let mut dt = epoch();
        dt.year = 4096;
        assert_eq!(datetime_to_millis(&dt), Err(Error::InvalidYear));
        dt.year = 1969;
        assert_eq!(datetime_to_millis(&dt), Err(Error::OutOfRange));
    }

    #[test]
    /// Covers epoch, leap 2000, final supported second and deliberate subsecond truncation.
    fn leap_day_and_last_supported_second_round_trip() {
        assert_eq!(datetime_to_millis(&epoch()), Ok(0));
        let leap = millis_to_datetime(951_782_400_000).unwrap();
        assert_eq!((leap.year, leap.month, leap.day), (2000, 2, 29));
        assert_eq!(leap.day_of_week, DayOfWeek::Tuesday);
        assert_eq!(datetime_to_millis(&leap), Ok(951_782_400_000));
        let mut last = epoch();
        last.year = 4095;
        last.month = 12;
        last.day = 31;
        last.hour = 23;
        last.minute = 59;
        last.second = 59;
        let millis = datetime_to_millis(&last).unwrap();
        let decoded = millis_to_datetime(millis + 999).unwrap();
        assert_eq!(datetime_to_millis(&decoded), Ok(millis));
        assert!(millis_to_datetime(millis + 1000).is_err());
    }
}
