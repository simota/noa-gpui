//! Proleptic Gregorian dates with a time of day to the minute, without time zones.

/// 24-hour clock. Orders chronologically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DateTime {
    pub year: i32,
    /// `1..=12`
    pub month: u32,
    /// `1..=days_in_month`
    pub day: u32,
    /// `0..=23`
    pub hour: u32,
    /// `0..=59`
    pub minute: u32,
}

pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Number of days in `month` (`1..=12`) of `year`.
pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = i64::from(year) - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
    (year, month, day)
}

impl DateTime {
    /// `None` if the date or time is invalid.
    pub fn new(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> Option<Self> {
        let valid = (1..=12).contains(&month)
            && day >= 1
            && day <= days_in_month(year, month)
            && hour < 24
            && minute < 60;
        valid.then_some(Self {
            year,
            month,
            day,
            hour,
            minute,
        })
    }

    pub fn date(year: i32, month: u32, day: u32) -> Option<Self> {
        Self::new(year, month, day, 0, 0)
    }

    /// Day of the week, `0` = Sunday.
    pub fn weekday(&self) -> u32 {
        // 1970-01-01 was a Thursday.
        (days_from_civil(self.year, self.month, self.day) + 4).rem_euclid(7) as u32
    }

    /// Negative `days` go back.
    pub fn add_days(self, days: i64) -> Self {
        let (year, month, day) =
            civil_from_days(days_from_civil(self.year, self.month, self.day) + days);
        Self {
            year,
            month,
            day,
            ..self
        }
    }

    /// Clamped to the month's length: January 31 plus one month is February 28 or 29.
    /// Negative `months` go back.
    pub fn add_months(self, months: i32) -> Self {
        let index = self.year * 12 + self.month as i32 - 1 + months;
        let year = index.div_euclid(12);
        let month = index.rem_euclid(12) as u32 + 1;
        Self {
            year,
            month,
            day: self.day.min(days_in_month(year, month)),
            ..self
        }
    }

    pub fn first_of_month(self) -> Self {
        Self { day: 1, ..self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekdays_and_leap_years() {
        assert_eq!(DateTime::date(1970, 1, 1).unwrap().weekday(), 4);
        assert_eq!(DateTime::date(2026, 11, 12).unwrap().weekday(), 4);
        assert_eq!(DateTime::date(2000, 2, 29).unwrap().weekday(), 2);
        assert!(DateTime::date(1900, 2, 29).is_none());
        assert!(DateTime::date(2024, 2, 29).is_some());
    }

    #[test]
    fn adding_days_and_months_crosses_boundaries() {
        let d = DateTime::new(2024, 12, 31, 9, 30).unwrap();
        assert_eq!(d.add_days(1), DateTime::new(2025, 1, 1, 9, 30).unwrap());
        assert_eq!(
            d.add_days(-366),
            DateTime::new(2023, 12, 31, 9, 30).unwrap()
        );
        let jan31 = DateTime::date(2024, 1, 31).unwrap();
        assert_eq!(jan31.add_months(1), DateTime::date(2024, 2, 29).unwrap());
        assert_eq!(jan31.add_months(-2), DateTime::date(2023, 11, 30).unwrap());
        assert_eq!(jan31.add_months(13), DateTime::date(2025, 2, 28).unwrap());
    }
}
