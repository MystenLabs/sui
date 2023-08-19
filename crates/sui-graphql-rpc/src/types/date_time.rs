// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{fmt::Display, str::FromStr};

use async_graphql::*;
use chrono::{
    prelude::{DateTime as ChronoDateTime, Utc as ChronoUtc},
    ParseError as ChronoParseError, TimeZone,
};

// ISO-8601 Date and Time: RFC3339 in UTC
// YYYY-MM-DDTHH:MM:SS.mmmZ
// Encoded as a 64-bit unix timestamp nanoseconds
struct DateTime(i64);

#[Scalar]
impl ScalarType for DateTime {
    fn parse(value: Value) -> InputValueResult<Self> {
        match value {
            Value::String(s) => DateTime::from_str(&s)
                .map_err(|e| InputValueError::custom(format!("Error parsing DateTime: {}", e))),
            _ => Err(InputValueError::expected_type(value)),
        }
    }

    fn to_value(&self) -> Value {
        Value::String(format!("{}", self))
    }
}

impl FromStr for DateTime {
    type Err = ChronoParseError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(DateTime(
            s.parse::<ChronoDateTime<ChronoUtc>>()?.timestamp_nanos(),
        ))
    }
}

impl Display for DateTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&format!("{:?}", ChronoUtc.timestamp_nanos(self.0)), f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        let dt: &str = "2023-08-19T15:37:24.761850Z";
        let date_time = DateTime::from_str(dt).unwrap();
        assert_eq!(dt, &format!("{}", date_time));

        let dt: &str = "2023-08-19T15:37:24.700Z";
        let date_time = DateTime::from_str(dt).unwrap();
        assert_eq!(dt, &format!("{}", date_time));

        let dt: &str = "2023-08-";
        assert!(DateTime::from_str(dt).is_err());
    }
}
