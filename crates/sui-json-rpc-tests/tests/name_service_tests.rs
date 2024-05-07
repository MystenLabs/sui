// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
use sui_json_rpc::name_service::{self, Domain};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    collection_types::VecMap,
};

#[test]
fn test_parent_extraction() {
    let mut name = Domain::from_str("leaf.node.test.sui").unwrap();

    assert_eq!(name.parent().to_string(), "node.test.sui");

    name = Domain::from_str("node.test.sui").unwrap();

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

#[test]
fn test_name_service_outputs() {
    assert_eq!("@test".parse::<Domain>().unwrap().to_string(), "test.sui");
    assert_eq!(
        "test.sui".parse::<Domain>().unwrap().to_string(),
        "test.sui"
    );
    assert_eq!(
        "test@sld".parse::<Domain>().unwrap().to_string(),
        "test.sld.sui"
    );
    assert_eq!(
        "test.test@example".parse::<Domain>().unwrap().to_string(),
        "test.test.example.sui"
    );
    assert_eq!(
        "sui@sui".parse::<Domain>().unwrap().to_string(),
        "sui.sui.sui"
    );

    assert_eq!("@sui".parse::<Domain>().unwrap().to_string(), "sui.sui");

    assert_eq!(
        "test*test@test".parse::<Domain>().unwrap().to_string(),
        "test.test.test.sui"
    );
    assert_eq!(
        "test.test.sui".parse::<Domain>().unwrap().to_string(),
        "test.test.sui"
    );
    assert_eq!(
        "test.test.test.sui".parse::<Domain>().unwrap().to_string(),
        "test.test.test.sui"
    );
}

#[test]
fn test_different_wildcard() {
    assert_eq!("test.sui".parse::<Domain>(), "test*sui".parse::<Domain>(),);

    assert_eq!("@test".parse::<Domain>(), "test*sui".parse::<Domain>(),);
}

#[test]
fn test_invalid_inputs() {
    assert!("*".parse::<Domain>().is_err());
    assert!(".".parse::<Domain>().is_err());
    assert!("@".parse::<Domain>().is_err());
    assert!("@inner.sui".parse::<Domain>().is_err());
    assert!("@inner*sui".parse::<Domain>().is_err());
    assert!("test@".parse::<Domain>().is_err());
    assert!("sui".parse::<Domain>().is_err());
    assert!("test.test@example.sui".parse::<Domain>().is_err());
    assert!("test@test@example".parse::<Domain>().is_err());
}

#[test]
fn output_tests() {
    let mut domain = "test.sui".parse::<Domain>().unwrap();
    assert!(domain.format(name_service::DomainFormat::Dot) == "test.sui");
    assert!(domain.format(name_service::DomainFormat::At) == "@test");

    domain = "test.test.sui".parse::<Domain>().unwrap();
    assert!(domain.format(name_service::DomainFormat::Dot) == "test.test.sui");
    assert!(domain.format(name_service::DomainFormat::At) == "test@test");

    domain = "test.test.test.sui".parse::<Domain>().unwrap();
    assert!(domain.format(name_service::DomainFormat::Dot) == "test.test.test.sui");
    assert!(domain.format(name_service::DomainFormat::At) == "test.test@test");

    domain = "test.test.test.test.sui".parse::<Domain>().unwrap();
    assert!(domain.format(name_service::DomainFormat::Dot) == "test.test.test.test.sui");
    assert!(domain.format(name_service::DomainFormat::At) == "test.test.test@test");
}
