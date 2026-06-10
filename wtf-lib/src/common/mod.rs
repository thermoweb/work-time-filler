use chrono::{DateTime, Local, Utc};

pub struct Common;

impl Common {
    pub fn format_date_time(date: &DateTime<Utc>) -> String {
        date.with_timezone(&Local)
            .format("%d-%m-%Y %H:%M")
            .to_string()
    }

    pub fn readable_time_spent(time_spent_seconds: i64) -> String {
        if time_spent_seconds < 3_600 {
            format!("{}m", time_spent_seconds / 60)
        } else {
            format!("{:.1}h", time_spent_seconds as f64 / 3_600.0)
        }
    }
}
