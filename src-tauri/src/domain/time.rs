use std::time::Instant;

use chrono::{DateTime, Duration, LocalResult, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;

use crate::error::{AppError, AppResult};

pub trait Clock: Send + Sync {
    fn now_utc(&self) -> DateTime<Utc>;
    fn monotonic_now(&self) -> Instant;
}

#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_utc(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn monotonic_now(&self) -> Instant {
        Instant::now()
    }
}

pub trait TimeZoneProvider: Send + Sync {
    fn current(&self) -> AppResult<Tz>;
}

#[derive(Debug, Default)]
pub struct SystemTimeZone;

impl TimeZoneProvider for SystemTimeZone {
    fn current(&self) -> AppResult<Tz> {
        iana_time_zone::get_timezone()
            .map_err(|_| AppError::TimeZone)?
            .parse()
            .map_err(|_| AppError::TimeZone)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDateSlice {
    pub local_date: NaiveDate,
    pub start_utc: DateTime<Utc>,
    pub end_utc: DateTime<Utc>,
}

pub fn split_by_local_date(
    start_utc: DateTime<Utc>,
    end_utc: DateTime<Utc>,
    time_zone: Tz,
) -> AppResult<Vec<LocalDateSlice>> {
    if end_utc < start_utc {
        return Err(AppError::InvalidTimeRange);
    }
    if end_utc == start_utc {
        return Ok(Vec::new());
    }

    let mut slices = Vec::new();
    let mut cursor = start_utc;
    while cursor < end_utc {
        let local_date = cursor.with_timezone(&time_zone).date_naive();
        let next_date = local_date.succ_opt().ok_or(AppError::InvalidTimeRange)?;
        let next_boundary = first_valid_instant(next_date, time_zone)?;
        let slice_end = end_utc.min(next_boundary);
        if slice_end <= cursor {
            return Err(AppError::InvalidTimeRange);
        }
        slices.push(LocalDateSlice {
            local_date,
            start_utc: cursor,
            end_utc: slice_end,
        });
        cursor = slice_end;
    }
    Ok(slices)
}

fn first_valid_instant(date: NaiveDate, time_zone: Tz) -> AppResult<DateTime<Utc>> {
    let midnight = date
        .and_hms_opt(0, 0, 0)
        .ok_or(AppError::InvalidTimeRange)?;
    for minutes in 0..=180 {
        let candidate = midnight + Duration::minutes(minutes);
        match time_zone.from_local_datetime(&candidate) {
            LocalResult::Single(value) | LocalResult::Ambiguous(value, _) => {
                return Ok(value.with_timezone(&Utc));
            }
            LocalResult::None => {}
        }
    }
    Err(AppError::InvalidTimeRange)
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use chrono::{DateTime, TimeZone, Utc};
    use chrono_tz::{America::New_York, Asia::Taipei};

    use super::{Clock, split_by_local_date};

    struct FakeClock {
        now: DateTime<Utc>,
        monotonic: Instant,
    }

    impl Clock for FakeClock {
        fn now_utc(&self) -> DateTime<Utc> {
            self.now
        }

        fn monotonic_now(&self) -> Instant {
            self.monotonic
        }
    }

    #[test]
    fn 假時鐘可固定真實與單調時間() {
        let expected = Utc.with_ymd_and_hms(2026, 7, 23, 8, 0, 0).unwrap();
        let monotonic = Instant::now();
        let clock = FakeClock {
            now: expected,
            monotonic,
        };
        assert_eq!(clock.now_utc(), expected);
        assert_eq!(clock.monotonic_now(), monotonic);
    }

    #[test]
    fn 同一區間切換時區不改變總時間() {
        let start = Utc.with_ymd_and_hms(2026, 7, 23, 3, 30, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 7, 23, 5, 30, 0).unwrap();
        let new_york = split_by_local_date(start, end, New_York).unwrap();
        let taipei = split_by_local_date(start, end, Taipei).unwrap();
        assert_ne!(new_york[0].local_date, taipei[0].local_date);
        let duration = |slices: &[super::LocalDateSlice]| -> i64 {
            slices
                .iter()
                .map(|slice| (slice.end_utc - slice.start_utc).num_seconds())
                .sum()
        };
        assert_eq!(duration(&new_york), (end - start).num_seconds());
        assert_eq!(duration(&taipei), (end - start).num_seconds());
    }
    #[test]
    fn 跨日區間依本地日期切分() {
        let start = Utc.with_ymd_and_hms(2026, 7, 23, 3, 30, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 7, 23, 5, 30, 0).unwrap();
        let slices = split_by_local_date(start, end, New_York).unwrap();
        assert_eq!(slices.len(), 2);
        assert_eq!(slices[0].local_date.to_string(), "2026-07-22");
        assert_eq!(slices[1].local_date.to_string(), "2026-07-23");
    }

    #[test]
    fn 夏令時間日不重複累計() {
        let start = Utc.with_ymd_and_hms(2026, 3, 8, 5, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 3, 9, 4, 0, 0).unwrap();
        let slices = split_by_local_date(start, end, New_York).unwrap();
        let seconds: i64 = slices
            .iter()
            .map(|slice| (slice.end_utc - slice.start_utc).num_seconds())
            .sum();
        assert_eq!(seconds, (end - start).num_seconds());
    }
}
