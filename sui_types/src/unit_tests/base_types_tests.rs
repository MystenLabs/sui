// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::blacklisted_name)]

use crate::{gas_coin::GasCoin, crypto::{BcsSignable, get_key_pair, Signature}};

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
    let max = SequenceNumber::max();
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
