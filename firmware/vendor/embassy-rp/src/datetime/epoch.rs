//! Unix epoch conversion (milliseconds since 1970-01-01 00:00:00 UTC)

#[cfg(any(not(feature = "chrono"), feature = "_rp235x"))]
pub(crate) const EPOCH_YEAR: u16 = 1970;

#[cfg(not(feature = "chrono"))]
mod no_deps {
    use super::super::{DateTime, DayOfWeek, Error};
    use super::EPOCH_YEAR;
    const DAYS_IN_MONTH: [u8; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    const MS_PER_SECOND: u64 = 1000;
    const MS_PER_MINUTE: u64 = 60 * MS_PER_SECOND;
    const MS_PER_HOUR: u64 = 60 * MS_PER_MINUTE;
    const MS_PER_DAY: u64 = 24 * MS_PER_HOUR;
    const EPOCH_DAY_OF_WEEK: u8 = 4; // Thursday

    const fn is_leap_year(year: u16) -> bool {
        (year % 4 == 0) && ((year % 100 != 0) || (year % 400 == 0))
    }

    fn days_in_month(year: u16, month: u8) -> u8 {
        if month == 2 && is_leap_year(year) {
            29
        } else {
            DAYS_IN_MONTH[(month - 1) as usize]
        }
    }

    const fn days_from_epoch_to_year(year: u16) -> u32 {
        let mut days = 0u32;
        let mut y = EPOCH_YEAR;
        while y < year {
            days += if is_leap_year(y) { 366 } else { 365 };
            y += 1;
        }
        days
    }

    fn day_of_week_from_days(days_since_epoch: u32) -> u8 {
        ((days_since_epoch + EPOCH_DAY_OF_WEEK as u32) % 7) as u8
    }

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
    pub(crate) fn millis_to_datetime(millis: u64) -> Result<DateTime, Error> {
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
mod chrono_rp235x {
    use super::super::{DateTime, Error};
    use super::EPOCH_YEAR;

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
mod tests {
    use super::*;
    use crate::datetime::{DateTime, DayOfWeek, Error};

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
