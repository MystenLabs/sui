// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
use std::time::SystemTime;
use sui_json_rpc::name_service;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    collection_types::VecMap,
};

#[test]
fn test_parent_extraction() {
    let mut name = name_service::Domain::from_str("leaf.node.test.sui").unwrap();

    assert_eq!(name.parent().to_string(), "node.test.sui");

    name = name_service::Domain::from_str("node.test.sui").unwrap();

    assert_eq!(name.parent().to_string(), "test.sui");
}

#[test]
fn test_sld_extraction() {
    let name = name_service::Domain::from_str("nested.leaf.node.test.sui").unwrap();

    assert_eq!(name.sld().to_string(), "test.sui");
}

#[test]
fn test_expirations() {
    let expiration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let mut name = name_service::NameRecord {
        nft_id: sui_types::id::ID::new(ObjectID::random()),
        data: VecMap { contents: vec![] },
        target_address: Some(SuiAddress::random_for_testing_only()),
        expiration_timestamp_ms: expiration + 1_000_000,
    };

    assert!(!name.is_expired());

    name.expiration_timestamp_ms = expiration - 1_000_000;

    assert!(name.is_expired());
}

// TODO: Add more test cases here for expiration checks on SubNameRecords (node/leafs).
