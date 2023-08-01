use chrono::{DateTime, Utc};
use rkyv::{Archive, Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Archive, Serialize, Deserialize)]
pub struct TimeString(pub String);

impl From<&TimeString> for String {
    fn from(time_str: &TimeString) -> Self {
        time_str.0.clone()
    }
}

impl From<SystemTime> for TimeString {
    fn from(sys_time: SystemTime) -> Self {
        let dt: DateTime<Utc> = sys_time.clone().into();
        TimeString(format!("{}", dt.format("%Y-%b-%d %H:%M")))
    }
}

impl From<TimeString> for String {
    fn from(time_str: TimeString) -> Self {
        time_str.0
    }
}
