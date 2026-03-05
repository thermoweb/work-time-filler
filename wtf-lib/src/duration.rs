use chrono::Duration;
use std::num::ParseIntError;

#[derive(Debug)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hours() {
        assert_eq!(parse_duration("2h").unwrap(), Duration::hours(2));
    }

    #[test]
    fn test_parse_minutes() {
        assert_eq!(parse_duration("30m").unwrap(), Duration::minutes(30));
    }

    #[test]
    fn test_parse_seconds() {
        assert_eq!(parse_duration("45s").unwrap(), Duration::seconds(45));
    }

    #[test]
    fn test_parse_days() {
        assert_eq!(parse_duration("3d").unwrap(), Duration::days(3));
    }

    #[test]
    fn test_parse_weeks() {
        assert_eq!(parse_duration("1w").unwrap(), Duration::weeks(1));
    }

    #[test]
    fn test_invalid_unit() {
        assert!(matches!(
            parse_duration("5x"),
            Err(DurationParserError::InvalidFormat)
        ));
    }

    #[test]
    fn test_invalid_number() {
        assert!(matches!(
            parse_duration("abch"),
            Err(DurationParserError::ParseError(_))
        ));
    }
}
