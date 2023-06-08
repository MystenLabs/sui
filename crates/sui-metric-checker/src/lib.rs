// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::anyhow;
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use humantime::parse_duration;
use regex::Regex;
use serde::Deserialize;
use strum_macros::Display;

pub mod query;

#[derive(Debug, Display, Deserialize, PartialEq)]
pub enum QueryType {
    Instant,
    Range,
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
    // Type of query to execute
    #[serde(rename = "type")]
    pub query_type: QueryType,
    pub validate_result: Option<QueryResultValidation>,
    // Both start & end accepts specific time formats
    //  - "%Y-%m-%d %H:%M:%S" (UTC)
    // Or relative time + offset, i.e.
    //  - "now"
    //  - "now-1h"
    //  - "now-30m 10s"
    pub start: Option<String>,
    pub end: Option<String>,
    // Query resolution step width as float number of seconds
    pub step: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub queries: Vec<Query>,
}

// Convert timestamp string to unix seconds.
// Accepts the following time formats
//  - "%Y-%m-%d %H:%M:%S" (UTC)
// Or relative time + offset, i.e.
//  - "now"
//  - "now-1h"
//  - "now-30m 10s"
pub fn timestamp_string_to_unix_seconds(timestamp: &str) -> Result<i64, anyhow::Error> {
    let now_regex = Regex::new(r"^now(-.*)?$").unwrap();
    let relative_time_regex = Regex::new(r"^now-([\dsmh ]+)$").unwrap();

    if now_regex.is_match(timestamp) {
        if let Some(capture) = relative_time_regex.captures(timestamp) {
            if let Some(relative_timestamp) = capture.get(1) {
                let duration = parse_duration(relative_timestamp.as_str())?;
                let now = Utc::now();
                let new_datetime = now.checked_sub_signed(Duration::from_std(duration)?);

                if let Some(datetime) = new_datetime {
                    return Ok(datetime.timestamp());
                } else {
                    return Err(anyhow!("Unable to calculate time offset"));
                }
            }
        }

        return Ok(Utc::now().timestamp());
    }

    if let Ok(datetime) = NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S") {
        let utc_datetime = DateTime::<Utc>::from_utc(datetime, Utc);
        Ok(utc_datetime.timestamp())
    } else {
        Err(anyhow!("Invalid timestamp format"))
    }
}

pub fn fails_threshold_condition(
    queried_value: f64,
    threshold: f64,
    failure_condition: &Condition,
) -> Result<bool, anyhow::Error> {
    match failure_condition {
        Condition::Greater => Ok(queried_value > threshold),
        Condition::Equal => Ok(queried_value == threshold),
        Condition::Less => Ok(queried_value < threshold),
    }
}

fn unix_seconds_to_timestamp_string(unix_seconds: i64) -> String {
    let datetime = NaiveDateTime::from_timestamp_opt(unix_seconds, 0);
    let timestamp = DateTime::<Utc>::from_utc(datetime.unwrap(), Utc);
    timestamp.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_timestamp_string_to_unix_seconds() {
        let timestamp = "2021-08-10 00:00:00";
        let unix_seconds = timestamp_string_to_unix_seconds(timestamp).unwrap();
        assert_eq!(unix_seconds, 1628553600);

        let timestamp = "now";
        let unix_seconds = timestamp_string_to_unix_seconds(timestamp).unwrap();
        assert_eq!(unix_seconds, Utc::now().timestamp());

        let timestamp = "now-1h";
        let unix_seconds = timestamp_string_to_unix_seconds(timestamp).unwrap();
        assert_eq!(unix_seconds, Utc::now().timestamp() - 3600);

        let timestamp = "now-30m 10s";
        let unix_seconds = timestamp_string_to_unix_seconds(timestamp).unwrap();
        assert_eq!(unix_seconds, Utc::now().timestamp() - 1810);
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
                type: "Instant"

              - query: 'histogram_quantile(0.50, sum by(le) (rate(round_latency{network="testnet"}[15m])))'
                type: "Range"
                validate_result:
                  threshold: 3.0
                  failure_condition: Greater
                start: "now-1h"
                end: "now"
                step: 60.0
        "#;

        let config: Config = serde_yaml::from_str(config).unwrap();

        let expected_range_query = Query {
            query: "histogram_quantile(0.50, sum by(le) (rate(round_latency{network=\"testnet\"}[15m])))".to_string(),
            query_type: QueryType::Range,
            validate_result: Some(QueryResultValidation {
                threshold: 3.0,
                failure_condition: Condition::Greater,
            }),
            start: Some("now-1h".to_string()),
            end: Some("now".to_string()),
            step: Some(60.0),
        };

        let expected_instant_query = Query {
            query: "max(current_epoch{network=\"testnet\"})".to_string(),
            query_type: QueryType::Instant,
            validate_result: None,
            start: None,
            end: None,
            step: None,
        };

        let expected_queries = vec![expected_instant_query, expected_range_query];

        assert_eq!(config.queries, expected_queries);
    }
}
