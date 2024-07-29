// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::anyhow;
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use humantime::parse_duration;
use serde::Deserialize;
use strum_macros::Display;

pub mod query;

#[derive(Debug, Display, Deserialize, PartialEq)]
pub enum QueryType {
    // Checks the last instant value of the query.
    Instant,
    // Checks the median value of the query over time.
    Range {
        // Both start & end accepts specific time formats
        //  - "%Y-%m-%d %H:%M:%S" (UTC)
        // Or relative time + offset, i.e.
        //  - "now"
        //  - "now-1h"
        //  - "now-30m 10s"
        start: String,
        end: String,
        // Query resolution step width as float number of seconds
        step: f64,
        // The result of the query is the percentile of the data points.
        // Valid values are [1, 100].
        percentile: u8,
    },
}

#[derive(Debug, Display, Deserialize, PartialEq)]
pub enum Condition {
    Greater,
    Equal,
    Less,
}

// Used to specify validation rules for query result e.g.
//
// validate_result:
//   threshold: 10
//   failure_condition: Greater
//
// Program will report error if queried value is greater than 10, otherwise
// no error will be reported.
#[derive(Debug, Deserialize, PartialEq)]
pub struct QueryResultValidation {
    // Threshold to report error on
    pub threshold: f64,
    // Program will report error if threshold violates condition specifed by this
    // field.
    pub failure_condition: Condition,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct Query {
    // PromQL query to exeute
    pub query: String,
    // Type of query to execute - Instant or Range.
    #[serde(rename = "type")]
    pub query_type: QueryType,
    // Optional validation rules for the query result, otherwise the query result
    // is just to be printed in debug logs.
    pub validate_result: Option<QueryResultValidation>,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub queries: Vec<Query>,
}

// Used to  mock now() in tests and use consistent now() return values across
// queries in performance checks.
pub trait NowProvider {
    fn now() -> DateTime<Utc>;
}

pub struct UtcNowProvider;

// Basic implementation of NowProvider that returns current time in UTC.
impl NowProvider for UtcNowProvider {
    fn now() -> DateTime<Utc> {
        Utc::now()
    }
}

// Convert timestamp string to unix seconds.
// Accepts the following time formats
//  - "%Y-%m-%d %H:%M:%S" (UTC)
// Or relative time + offset, i.e.
//  - "now"
//  - "now-1h"
//  - "now-30m 10s"
pub fn timestamp_string_to_unix_seconds<N: NowProvider>(
    timestamp: &str,
) -> Result<i64, anyhow::Error> {
    if timestamp.starts_with("now") {
        if let Some(relative_timestamp) = timestamp.strip_prefix("now-") {
            let duration = parse_duration(relative_timestamp)?;
            let now = N::now();
            let new_datetime = now.checked_sub_signed(Duration::from_std(duration)?);

            if let Some(datetime) = new_datetime {
                return Ok(datetime.timestamp());
            } else {
                return Err(anyhow!("Unable to calculate time offset"));
            }
        }

        return Ok(N::now().timestamp());
    }

    if let Ok(datetime) = NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S") {
        let utc_datetime: DateTime<Utc> = DateTime::from_naive_utc_and_offset(datetime, Utc);
        Ok(utc_datetime.timestamp())
    } else {
        Err(anyhow!("Invalid timestamp format"))
    }
}

pub fn fails_threshold_condition(
    queried_value: f64,
    threshold: f64,
    failure_condition: &Condition,
) -> bool {
    match failure_condition {
        Condition::Greater => queried_value > threshold,
        Condition::Equal => queried_value == threshold,
        Condition::Less => queried_value < threshold,
    }
}

fn unix_seconds_to_timestamp_string(unix_seconds: i64) -> String {
    DateTime::from_timestamp(unix_seconds, 0)
        .unwrap()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    struct MockNowProvider;

    impl NowProvider for MockNowProvider {
        fn now() -> DateTime<Utc> {
            Utc.timestamp_opt(1628553600, 0).unwrap()
        }
    }

    #[test]
    fn test_parse_timestamp_string_to_unix_seconds() {
        let timestamp = "2021-08-10 00:00:00";
        let unix_seconds = timestamp_string_to_unix_seconds::<MockNowProvider>(timestamp).unwrap();
        assert_eq!(unix_seconds, 1628553600);

        let timestamp = "now";
        let unix_seconds = timestamp_string_to_unix_seconds::<MockNowProvider>(timestamp).unwrap();
        assert_eq!(unix_seconds, 1628553600);

        let timestamp = "now-1h";
        let unix_seconds = timestamp_string_to_unix_seconds::<MockNowProvider>(timestamp).unwrap();
        assert_eq!(unix_seconds, 1628553600 - 3600);

        let timestamp = "now-30m 10s";
        let unix_seconds = timestamp_string_to_unix_seconds::<MockNowProvider>(timestamp).unwrap();
        assert_eq!(unix_seconds, 1628553600 - 1810);
    }

    #[test]
    fn test_unix_seconds_to_timestamp_string() {
        let unix_seconds = 1628534400;
        let timestamp = unix_seconds_to_timestamp_string(unix_seconds);
        assert_eq!(timestamp, "2021-08-09 18:40:00 UTC");
    }

    #[test]
    fn test_parse_config() {
        let config = r#"
            queries:
              - query: 'max(current_epoch{network="testnet"})'
                type: Instant

              - query: 'histogram_quantile(0.50, sum by(le) (rate(round_latency{network="testnet"}[15m])))'
                type: !Range 
                  start: "now-1h"
                  end: "now"
                  step: 60.0
                  percentile: 50
                validate_result:
                  threshold: 3.0
                  failure_condition: Greater
        "#;

        let config: Config = serde_yaml::from_str(config).unwrap();

        let expected_range_query = Query {
            query: "histogram_quantile(0.50, sum by(le) (rate(round_latency{network=\"testnet\"}[15m])))".to_string(),
            query_type: QueryType::Range {
                start: "now-1h".to_string(),
                end: "now".to_string(),
                step: 60.0,
                percentile: 50,
            },
            validate_result: Some(QueryResultValidation {
                threshold: 3.0,
                failure_condition: Condition::Greater,
            }),
        };

        let expected_instant_query = Query {
            query: "max(current_epoch{network=\"testnet\"})".to_string(),
            query_type: QueryType::Instant,
            validate_result: None,
        };

        let expected_queries = vec![expected_instant_query, expected_range_query];

        assert_eq!(config.queries, expected_queries);
    }
}
