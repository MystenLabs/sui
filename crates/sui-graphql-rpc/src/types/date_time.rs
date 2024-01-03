// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use async_graphql::*;
use chrono::{
    prelude::{DateTime as ChronoDateTime, TimeZone, Utc as ChronoUtc},
    ParseError as ChronoParseError,
};

use crate::error::Error;

/// The DateTime in UTC format. The milliseconds part is optional,
/// and it will be omitted if the ms value is zero.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DateTime(ChronoDateTime<ChronoUtc>);

impl DateTime {
    pub fn from_ms(timestamp_ms: i64) -> Result<Self, Error> {
        ChronoUtc
            .timestamp_millis_opt(timestamp_ms)
            .single()
            .ok_or_else(|| Error::Internal("Cannot convert timestamp into DateTime".to_string()))
            .map(Self)
    }
}

/// The DateTime in UTC format. The milliseconds part is optional,
/// and it will be omitted if the ms value is zero.
#[Scalar(use_type_description = true)]
impl ScalarType for DateTime {
    fn parse(value: Value) -> InputValueResult<Self> {
        match value {
            Value::String(s) => DateTime::from_str(&s)
                .map_err(|e| InputValueError::custom(format!("Error parsing DateTime: {}", e))),
            _ => Err(InputValueError::expected_type(value)),
        }
    }

    fn to_value(&self) -> Value {
        // Debug format for chrono::DateTime is YYYY-MM-DDTHH:MM:SS.mmmZ
        Value::String(format!("{:?}", self.0))
    }
}

impl Description for DateTime {
    fn description() -> &'static str {
        "ISO-8601 Date and Time: RFC3339 in UTC with format: YYYY-MM-DDTHH:MM:SS.mmmZ. Note that the milliseconds part is optional, and it may be omitted if its value is 0."
    }
}

impl FromStr for DateTime {
    type Err = ChronoParseError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(DateTime(s.parse::<ChronoDateTime<ChronoUtc>>()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        let dt: &str = "2023-08-19T15:37:24.761850Z";
        let date_time = DateTime::from_str(dt).unwrap();
        let Value::String(s) = async_graphql::ScalarType::to_value(&date_time) else {
            panic!("Invalid date time scalar");
        };
        assert_eq!(dt, s);

        let dt: &str = "2023-08-19T15:37:24.700Z";
        let date_time = DateTime::from_str(dt).unwrap();
        let Value::String(s) = async_graphql::ScalarType::to_value(&date_time) else {
            panic!("Invalid date time scalar");
        };
        assert_eq!(dt, s);

        let dt: &str = "2023-08-";
        assert!(DateTime::from_str(dt).is_err());
    }
}
