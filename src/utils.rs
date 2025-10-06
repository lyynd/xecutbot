use chrono::{NaiveDate, TimeDelta, Utc};

const DAY_ROLLOVER_HOUR: i64 = 5;

const TIMEZONE: chrono_tz::Tz = chrono_tz::Europe::Belgrade;

pub fn now() -> chrono::DateTime<chrono_tz::Tz> {
    Utc::now().with_timezone(&TIMEZONE)
}

pub fn today() -> NaiveDate {
    (Utc::now().with_timezone(&TIMEZONE) - TimeDelta::hours(DAY_ROLLOVER_HOUR)).date_naive()
}
