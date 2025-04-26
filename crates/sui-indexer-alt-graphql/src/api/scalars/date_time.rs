// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use anyhow::Context as _;
use async_graphql::*;
use chrono::TimeZone;

use crate::error::RpcError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DateTime(chrono::DateTime<chrono::Utc>);

impl DateTime {
    /// Takes a timestamp since the unix epoch in milliseconds and wraps it in the scalar.
    pub(crate) fn from_ms(timestamp_ms: i64) -> Result<Self, RpcError> {
        Ok(Self(
            chrono::Utc
                .timestamp_millis_opt(timestamp_ms)
                .single()
                .context("Cannot convert timestamp into DateTime")?,
        ))
    }
}

/// ISO-8601 Date and Time: RFC3339 in UTC with format: YYYY-MM-DDTHH:MM:SS.mmmZ. Note that the milliseconds part is optional, and it may be omitted if its value is 0.
#[Scalar]
impl ScalarType for DateTime {
    fn parse(value: Value) -> InputValueResult<Self> {
        let Value::String(s) = value else {
            return Err(InputValueError::expected_type(value));
        };

        DateTime::from_str(&s)
    }

    fn to_value(&self) -> Value {
        // Debug format for chrono::DateTime is YYYY-MM-DDTHH:MM:SS.mmmZ
        Value::String(format!("{:?}", self.0))
    }
}

impl FromStr for DateTime {
    type Err = InputValueError<Self>;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(DateTime(s.parse().map_err(|e| {
            InputValueError::custom(format!("Error parsing DateTime: {e}"))
        })?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        let dt: &str = "2023-08-19T15:37:24.761850Z";
        let date_time = DateTime::from_str(dt).unwrap();
        let Value::String(s) = ScalarType::to_value(&date_time) else {
            panic!("Invalid date time scalar");
        };
        assert_eq!(dt, s);

        let dt: &str = "2023-08-19T15:37:24.700Z";
        let date_time = DateTime::from_str(dt).unwrap();
        let Value::String(s) = ScalarType::to_value(&date_time) else {
            panic!("Invalid date time scalar");
        };
        assert_eq!(dt, s);

        let dt: &str = "2023-08-";
        assert!(DateTime::from_str(dt).is_err());
    }
}
