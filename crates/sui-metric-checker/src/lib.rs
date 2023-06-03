// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::anyhow;
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use humantime::parse_duration;
use serde::Deserialize;
use strum_macros::Display;

pub mod query;

#[derive(Debug, Display, Deserialize)]
pub enum QueryType {
    Instant,
    Range,
}

#[derive(Debug, Display, Deserialize)]
pub enum ErrorCondition {
    Greater,
    Equal,
    Less,
}

#[derive(Debug, Deserialize)]
pub struct QueryResultValidation {
    // Threshold to report error on
    pub threshold: f64,
    // Program will report error if threshold violates condition specifed by this
    // field.
    pub condition: ErrorCondition,
}

#[derive(Debug, Deserialize)]
pub struct Query {
    // PromQL query to exeute
    pub query: String,
    // Type of query to execute
    #[serde(rename = "type")]
    pub query_type: QueryType,
    pub validate_result: Option<QueryResultValidation>,
    // Both start & end accepts formats
    //  - %Y-%m-%d %H:%M:%S (UTC)
    //  - now
    //  - relative time offset before now, i.e. 1h 30m 10s
    pub start: Option<String>,
    pub end: Option<String>,
    // Query resolution step width as float number of seconds
    pub step: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub queries: Vec<Query>,
}

pub fn unix_seconds_to_timestamp(unix_seconds: i64) -> String {
    let datetime = NaiveDateTime::from_timestamp_opt(unix_seconds, 0);
    let timestamp = DateTime::<Utc>::from_utc(datetime.unwrap(), Utc);
    timestamp.to_string()
}

pub fn timestamp_to_unix_seconds(timestamp: &str) -> Result<i64, anyhow::Error> {
    match timestamp {
        "now" => Ok(Utc::now().timestamp()),
        _ => {
            let parsed_timestamp = NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S");
            if let Ok(datetime) = parsed_timestamp {
                let utc_datetime = DateTime::<Utc>::from_utc(datetime, Utc);
                Ok(utc_datetime.timestamp())
            } else {
                // Parse timestamp from relative time offset, i.e. "1h 30m 10s"
                let duration = parse_duration(timestamp)?;
                let now = Utc::now();
                let new_datetime = now.checked_sub_signed(Duration::from_std(duration).unwrap());

                if let Some(datetime) = new_datetime {
                    Ok(datetime.timestamp())
                } else {
                    Err(anyhow!("Unable calculate time offset"))
                }
            }
        }
    }
}

pub fn fails_threshold_condition(
    queried_value: f64,
    threshold: f64,
    error_condition: &ErrorCondition,
) -> Result<bool, anyhow::Error> {
    match error_condition {
        ErrorCondition::Greater => Ok(queried_value > threshold),
        ErrorCondition::Equal => Ok(queried_value == threshold),
        ErrorCondition::Less => Ok(queried_value < threshold),
    }
}
