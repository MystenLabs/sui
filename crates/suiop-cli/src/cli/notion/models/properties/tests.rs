// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use chrono::NaiveDate;

use crate::cli::notion::models::properties::{DateOrDateTime, RollupPropertyValue, RollupValue};

use super::PropertyValue;

#[test]
fn verify_date_parsing() {
    let date = NaiveDate::from_ymd_opt(2021, 1, 2).unwrap();
    let result = serde_json::to_string(&DateOrDateTime::Date(date)).unwrap();
    let parsed: DateOrDateTime = serde_json::from_str(&result).unwrap();
    println!("{:?}", parsed);
}

#[test]
fn parse_date_property() {
    let _property: PropertyValue =
        serde_json::from_str(include_str!("tests/date_property.json")).unwrap();
}

#[test]
fn parse_null_select_property() {
    let _property: PropertyValue =
        serde_json::from_str(include_str!("tests/null_select_property.json")).unwrap();
}

#[test]
fn parse_select_property() {
    let _property: PropertyValue =
        serde_json::from_str(include_str!("tests/select_property.json")).unwrap();
}

#[test]
fn parse_text_property_with_link() {
    let _property: PropertyValue =
        serde_json::from_str(include_str!("tests/text_with_link.json")).unwrap();
}

#[test]
fn parse_rollup_property() {
    let property: PropertyValue =
        serde_json::from_str(include_str!("tests/rollup_property.json")).unwrap();

    assert!(matches!(
        property,
        PropertyValue::Rollup {
            rollup: Some(RollupValue::Array { .. }),
            ..
        }
    ));

    if let PropertyValue::Rollup {
        rollup: Some(RollupValue::Array { array }),
        ..
    } = property
    {
        assert!(matches!(array[0], RollupPropertyValue::Text { .. }))
    }
}
