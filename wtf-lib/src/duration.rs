use chrono::Duration;
use std::num::ParseIntError;

pub enum DurationParserError {
    InvalidFormat,
    ParseError(ParseIntError),
}

impl From<ParseIntError> for DurationParserError {
    fn from(e: ParseIntError) -> Self {
        DurationParserError::ParseError(e)
    }
}

pub fn parse_duration(time_str: &str) -> Result<Duration, DurationParserError> {
    let (num_str, unit) = time_str.split_at(time_str.len() - 1);
    let num = num_str.parse::<i64>()?;

    match unit {
        "h" => Ok(Duration::hours(num)),
        "m" => Ok(Duration::minutes(num)),
        "s" => Ok(Duration::seconds(num)),
        "d" => Ok(Duration::days(num)),
        "w" => Ok(Duration::weeks(num)),
        _ => Err(DurationParserError::InvalidFormat),
    }
}
