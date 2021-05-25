// Copyright (c) Facebook Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn test_fake_signatures() {
    let (addr1, sec1) = get_key_pair();
    let (addr2, _sec2) = get_key_pair();

    let s = Signature::new(b"hello", &sec1);
    assert!(s.check(b"hello", addr1).is_ok());
    assert!(s.check(b"hello", addr2).is_err());
    assert!(s.check(b"hellx", addr1).is_err());
}

#[test]
fn test_max_sequence_number() {
    let max = SequenceNumber::max();
    assert_eq!(max.0 * 2 + 1, std::u64::MAX);
}
