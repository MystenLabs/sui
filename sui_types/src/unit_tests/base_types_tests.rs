// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::blacklisted_name)]
use super::*;
use crate::crypto::AuthoritySignature;

use move_binary_format::file_format;

use crate::{
    crypto::{get_key_pair, BcsSignable, Signature},
    gas_coin::GasCoin,
    object::Object,
};
use std::str::FromStr;

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

    assert_eq!(format!("{id}"), upper_hex);
    assert_eq!(format!("{:?}", id), upper_hex);
    assert_eq!(format!("{:X}", id), upper_hex);
    assert_eq!(format!("{:x}", id), hex);
    assert_eq!(format!("{:#x}", id), format!("0x{hex}"));
    assert_eq!(format!("{:#X}", id), format!("0x{upper_hex}"));
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
fn test_object_id_serde_not_human_readable() {
    let obj_id = ObjectID::random();
    let serialized = bincode::serialize(&obj_id).unwrap();
    assert_eq!(obj_id.0.to_vec(), serialized);
    let deserialized: ObjectID = bincode::deserialize(&serialized).unwrap();
    assert_eq!(deserialized, obj_id);
}

#[test]
fn test_address_serde_not_human_readable() {
    let address = SuiAddress::random_for_testing_only();
    let serialized = bincode::serialize(&address).unwrap();
    let bcs_serialized = bcs::to_bytes(&address).unwrap();
    // bincode use 8 bytes for BYTES len and bcs use 1 byte
    assert_eq!(serialized[8..], bcs_serialized[1..]);
    assert_eq!(address.0, serialized[8..]);
    let deserialized: SuiAddress = bincode::deserialize(&serialized).unwrap();
    assert_eq!(deserialized, address);
}

#[test]
fn test_address_serde_human_readable() {
    let address = SuiAddress::random_for_testing_only();
    let serialized = serde_json::to_string(&address).unwrap();
    assert_eq!(format!("\"{}\"", hex::encode(address)), serialized);
    let deserialized: SuiAddress = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, address);
}

#[test]
fn test_transaction_digest_serde_not_human_readable() {
    let digest = TransactionDigest::random();
    let serialized = bincode::serialize(&digest).unwrap();
    let bcs_serialized = bcs::to_bytes(&digest).unwrap();
    // bincode use 8 bytes for BYTES len and bcs use 1 byte
    assert_eq!(serialized[8..], bcs_serialized[1..]);
    assert_eq!(digest.0.to_vec(), serialized[8..]);
    let deserialized: TransactionDigest = bincode::deserialize(&serialized).unwrap();
    assert_eq!(deserialized, digest);
}

#[test]
fn test_transaction_digest_serde_human_readable() {
    let digest = TransactionDigest::random();
    let serialized = serde_json::to_string(&digest).unwrap();
    assert_eq!(format!("\"{}\"", base64::encode(digest.0)), serialized);
    let deserialized: TransactionDigest = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, digest);
}

#[test]
fn test_signature_serde_not_human_readable() {
    let (_, key) = get_key_pair();
    let sig = AuthoritySignature::new(&Foo("some data".to_string()), &key);
    let serialized = bincode::serialize(&sig).unwrap();
    let bcs_serialized = bcs::to_bytes(&sig).unwrap();

    // bincode use 8 bytes for BYTES len and bcs use 1 byte
    assert_eq!(serialized[8..], bcs_serialized[1..]);
    assert_eq!(sig.0.to_bytes(), serialized[8..]);
    let deserialized: AuthoritySignature = bincode::deserialize(&serialized).unwrap();
    assert_eq!(deserialized, sig);
}

#[test]
fn test_signature_serde_human_readable() {
    let (_, key) = get_key_pair();
    let sig = AuthoritySignature::new(&Foo("some data".to_string()), &key);
    let serialized = serde_json::to_string(&sig).unwrap();
    assert_eq!(format!("\"{}\"", base64::encode(sig)), serialized);
    let deserialized: AuthoritySignature = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized, sig);
}

#[test]
fn test_object_id_from_empty_string() {
    assert!(ObjectID::try_from("".to_string()).is_err());
    assert!(ObjectID::from_str("").is_err());
}

#[test]
fn test_move_object_size_for_gas_metering() {
    let object = Object::with_id_owner_for_testing(
        ObjectID::random(),
        SuiAddress::random_for_testing_only(),
    );
    let size = object.object_size_for_gas_metering();
    let serialized = bcs::to_bytes(&object).unwrap();
    // The result of object_size_for_gas_metering() will be smaller due to not including
    // all the metadata data needed for serializing various types.
    // If the following assertion breaks, it's likely you have changed MoveObject's fields.
    // Make sure to adjust `object_size_for_gas_metering()` to include those changes.
    assert_eq!(size + 16, serialized.len());
}

#[test]
fn test_move_package_size_for_gas_metering() {
    let module = file_format::empty_module();
    let package = Object::new_package(vec![module], TransactionDigest::genesis());
    let size = package.object_size_for_gas_metering();
    let serialized = bcs::to_bytes(&package).unwrap();
    // If the following assertion breaks, it's likely you have changed MovePackage's fields.
    // Make sure to adjust `object_size_for_gas_metering()` to include those changes.
    assert_eq!(size + 5, serialized.len());
}
