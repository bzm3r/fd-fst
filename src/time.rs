use std::time::SystemTime;

pub struct TimeString(pub String);

impl From<&TimeString> for String {
    fn from(time: &TimeString) -> Self {
        time.0.clone()
    }
}

impl From<SystemTime> for TimeString {
    fn from(time: SystemTime) -> Self {
        let dt: DateTime<Utc> = time.clone().into();
        TimeString(format!("{}", dt.format("%Y-%b-%d %H:%M")))
    }
}
