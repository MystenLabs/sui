// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::blacklisted_name)]

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
