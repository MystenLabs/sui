// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::blacklisted_name)]

use crate::{
    crypto::{get_key_pair, BcsSignable, Signature},
    gas_coin::GasCoin,
};
use std::str::FromStr;

use super::*;

#[derive(Serialize, Deserialize)]
struct Foo(String);

impl BcsSignable for Foo {}

#[derive(Serialize, Deserialize)]
struct Bar(String);

impl BcsSignable for Bar {}

#[test]
fn test_signatures() {
    let (addr1, sec1) = get_key_pair();
    let (addr2, _sec2) = get_key_pair();

    let foo = Foo("hello".into());
    let foox = Foo("hellox".into());
    let bar = Bar("hello".into());

    let s = Signature::new(&foo, &sec1);
    assert!(s.check(&foo, addr1).is_ok());
    assert!(s.check(&foo, addr2).is_err());
    assert!(s.check(&foox, addr1).is_err());
    assert!(s.check(&bar, addr1).is_err());
}

#[test]
fn test_max_sequence_number() {
    let max = SequenceNumber::MAX;
    assert_eq!(max.0 * 2 + 1, std::u64::MAX);
}

#[test]
fn test_gas_coin_ser_deser_roundtrip() {
    let id = ObjectID::random();
    let coin = GasCoin::new(id, SequenceNumber::new(), 10);
    let coin_bytes = coin.to_bcs_bytes();

    let deserialized_coin: GasCoin = bcs::from_bytes(&coin_bytes).unwrap();
    assert_eq!(deserialized_coin.id(), coin.id());
    assert_eq!(deserialized_coin.value(), coin.value());
    assert_eq!(deserialized_coin.version(), coin.version());
}

#[test]
fn test_increment_version() {
    let id = ObjectID::random();
    let version = SequenceNumber::from(257);
    let value = 10;
    let coin = GasCoin::new(id, version, value);
    assert_eq!(coin.id(), &id);
    assert_eq!(coin.value(), value);
    assert_eq!(coin.version(), version);

    let mut coin_obj = coin.to_object();
    assert_eq!(&coin_obj.id(), coin.id());
    assert_eq!(coin_obj.version(), coin.version());

    // update contents, which should increase sequence number, but leave
    // everything else the same
    let old_contents = coin_obj.contents().to_vec();
    let old_type_specific_contents = coin_obj.type_specific_contents().to_vec();
    coin_obj.update_contents(old_contents);
    assert_eq!(coin_obj.version(), version.increment());
    assert_eq!(&coin_obj.id(), coin.id());
    assert_eq!(
        coin_obj.type_specific_contents(),
        old_type_specific_contents
    );
    assert!(GasCoin::try_from(&coin_obj).unwrap().value() == coin.value());
}

#[test]
fn test_object_id_conversions() {}

#[test]
fn test_object_id_display() {
    let hex = "ca843279e3427144cead5e4d5999a3d05999a3d0";
    let upper_hex = "CA843279E3427144CEAD5E4D5999A3D05999A3D0";

    let id = ObjectID::from_hex(hex).unwrap();

    assert_eq!(format!("{}", id), upper_hex);
    assert_eq!(format!("{:?}", id), upper_hex);
    assert_eq!(format!("{:X}", id), upper_hex);
    assert_eq!(format!("{:x}", id), hex);
    assert_eq!(format!("{:#x}", id), format!("0x{}", hex));
    assert_eq!(format!("{:#X}", id), format!("0x{}", upper_hex));
}

#[test]
fn test_object_id_str_lossless() {
    let id = ObjectID::from_hex("0000000000c0f1f95c5b1c5f0eda533eff269000").unwrap();
    let id_empty = ObjectID::from_hex("0000000000000000000000000000000000000000").unwrap();
    let id_one = ObjectID::from_hex("0000000000000000000000000000000000000001").unwrap();

    assert_eq!(id.short_str_lossless(), "c0f1f95c5b1c5f0eda533eff269000",);
    assert_eq!(id_empty.short_str_lossless(), "0",);
    assert_eq!(id_one.short_str_lossless(), "1",);
}

#[test]
fn test_object_id_from_hex_literal() {
    let hex_literal = "0x1";
    let hex = "0000000000000000000000000000000000000001";

    let obj_id_from_literal = ObjectID::from_hex_literal(hex_literal).unwrap();
    let obj_id = ObjectID::from_hex(hex).unwrap();

    assert_eq!(obj_id_from_literal, obj_id);
    assert_eq!(hex_literal, obj_id.to_hex_literal());

    // Missing '0x'
    ObjectID::from_hex_literal(hex).unwrap_err();
    // Too long
    ObjectID::from_hex_literal("0x10000000000000000000000000000000000000000000000000000000001")
        .unwrap_err();
}

#[test]
fn test_object_id_ref() {
    let obj_id = ObjectID::new([1u8; ObjectID::LENGTH]);
    let _: &[u8] = obj_id.as_ref();
}

#[test]
fn test_object_id_from_proto_invalid_length() {
    let bytes = vec![1; 123];
    ObjectID::from_bytes(bytes).unwrap_err();
}

#[test]
fn test_object_id_deserialize_from_json_value() {
    let obj_id = ObjectID::random();
    let json_value = serde_json::to_value(obj_id).expect("serde_json::to_value fail.");
    let obj_id2: ObjectID =
        serde_json::from_value(json_value).expect("serde_json::from_value fail.");
    assert_eq!(obj_id, obj_id2)
}

#[test]
fn test_object_id_serde_json() {
    let hex = "ca843279e342714123456784cead5e4d5999a3d0";
    let json_hex = "\"ca843279e342714123456784cead5e4d5999a3d0\"";

    let obj_id = ObjectID::from_hex(hex).unwrap();

    let json = serde_json::to_string(&obj_id).unwrap();
    let json_obj_id: ObjectID = serde_json::from_str(json_hex).unwrap();

    assert_eq!(json, json_hex);
    assert_eq!(obj_id, json_obj_id);
}

#[test]
fn test_object_id_from_empty_string() {
    assert!(ObjectID::try_from("".to_string()).is_err());
    assert!(ObjectID::from_str("").is_err());
}
