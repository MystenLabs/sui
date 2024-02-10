// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
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
fn test_expirations() {
    let system_time: u64 = 100;

    let mut name = name_service::NameRecord {
        nft_id: sui_types::id::ID::new(ObjectID::random()),
        data: VecMap { contents: vec![] },
        target_address: Some(SuiAddress::random_for_testing_only()),
        expiration_timestamp_ms: system_time + 10,
    };

    assert!(!name.is_node_expired(system_time));

    name.expiration_timestamp_ms = system_time - 10;

    assert!(name.is_node_expired(system_time));
}
