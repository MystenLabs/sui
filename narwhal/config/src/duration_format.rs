// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Allow us to deserialize Duration values in a more human friendly format
//! (e.x in json files). The deserialization supports to time units:
//! * miliseconds
//! * seconds
//!
//! To identify miliseconds then a string of the following format should be
//! provided: [number]ms , for example "20ms", or "2_000ms".
//!
//! To identify seconds, then the following format should be used:
//! [number]s, for example "20s", or "10_000s".
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::time::Duration;

pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;

    if let Some(milis) = s.strip_suffix("ms") {
        return milis
            .replace('_', "")
            .parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|e| serde::de::Error::custom(e.to_string()));
    } else if let Some(seconds) = s.strip_suffix('s') {
        return seconds
            .replace('_', "")
            .parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|e| serde::de::Error::custom(e.to_string()));
    }

    Err(serde::de::Error::custom(format!(
        "Wrong format detected: {s}. It should be number in miliseconds, e.x 10ms"
    )))
}

pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    format!("{}ms", duration.as_millis()).serialize(serializer)
}

#[cfg(test)]
mod tests {
    use crate::duration_format;
    use serde::{Deserialize, Serialize};
    use std::time::Duration;

    #[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
    struct MockProperties {
        #[serde(with = "duration_format")]
        property_1: Duration,
        #[serde(with = "duration_format")]
        property_2: Duration,
        #[serde(with = "duration_format")]
        property_3: Duration,
        #[serde(with = "duration_format")]
        property_4: Duration,
    }

    #[test]
    fn parse_miliseconds_and_seconds() {
        // GIVEN
        let input = r#"{
             "property_1": "1_000ms",
             "property_2": "2ms",
             "property_3": "8s",
             "property_4": "5_000s"
          }"#;

        // WHEN
        let result: MockProperties =
            serde_json::from_str(input).expect("Couldn't deserialize string");

        // THEN
        assert_eq!(result.property_1.as_millis(), 1_000);
        assert_eq!(result.property_2.as_millis(), 2);
        assert_eq!(result.property_3.as_secs(), 8);
        assert_eq!(result.property_4.as_secs(), 5_000);
    }

    #[test]
    fn roundtrip() {
        // GIVEN
        let input = r#"{
             "property_1": "1_000ms",
             "property_2": "2ms",
             "property_3": "8s",
             "property_4": "5_000s"
          }"#;

        // WHEN
        let result: MockProperties =
            serde_json::from_str(input).expect("Couldn't deserialize string");

        // THEN
        let serialized = serde_json::to_string(&result).unwrap();
        let deserialized = serde_json::from_str(&serialized).unwrap();
        assert_eq!(result, deserialized);
    }

    #[test]
    fn parse_error() {
        // GIVEN
        let input = r#"{
             "property_1": "1000 ms",
             "property_2": "8seconds"
          }"#;

        // WHEN
        let result = serde_json::from_str::<MockProperties>(input);

        // THEN
        assert!(result.is_err());
    }
}
