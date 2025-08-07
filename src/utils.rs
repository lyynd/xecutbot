use chrono::{Local, NaiveDate, TimeDelta};

const DAY_ROLLOVER_HOUR: i64 = 5;

pub fn today() -> NaiveDate {
    (Local::now() - TimeDelta::hours(DAY_ROLLOVER_HOUR)).date_naive()
}
