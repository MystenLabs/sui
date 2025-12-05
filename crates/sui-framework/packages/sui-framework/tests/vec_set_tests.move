// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::vec_set_tests;

use std::unit_test::assert_eq;
use sui::vec_set;

#[test, expected_failure(abort_code = vec_set::EKeyAlreadyExists)]
fun duplicate_key_abort() {
    let mut m = vec_set::empty();
    m.insert(1u64);
    m.insert(1);
}

#[test, expected_failure(abort_code = vec_set::EKeyDoesNotExist)]
fun nonexistent_key_remove() {
    let mut m = vec_set::empty();
    m.insert(1u64);
    let k = 2;
    m.remove(&k);
}

#[test]
fun smoke() {
    let mut m = vec_set::empty();
    10u64.do!(|i| {
        let k = i + 2;
        m.insert(k);
    });
    assert!(!m.is_empty());
    assert_eq!(m.length(), 10);
    // make sure the elements are as expected in all of the getter APIs we expose
    10u64.do!(|i| {
        let k = i + 2;
        assert!(m.contains(&k));
    });
    // remove all the elements
    let keys = (copy m).into_keys();
    10u64.do!(|i| {
        let k = i + 2;
        m.remove(&k);
        assert_eq!(keys[i], k);
    });
}

#[test]
fun test_keys() {
    let mut m = vec_set::empty();
    m.insert(1u64);
    m.insert(2);
    m.insert(3);

    assert_eq!(m.length(), 3);
    assert_eq!(*m.keys(), vector[1, 2, 3]);
}

#[test, allow(lint(collection_equality))]
fun round_trip() {
    let mut s = vec_set::empty();
    assert_eq!(s, vec_set::from_keys(vector[]));
    50u64.do!(|i| {
        let k = i + 2;
        s.insert(k);
        let s2 = vec_set::from_keys(s.into_keys());
        assert_eq!(s, s2);
    });
}

#[test, expected_failure(abort_code = vec_set::EKeyAlreadyExists)]
fun from_keys_values_duplicate_key_abort() {
    vec_set::from_keys<u64>(vector[1, 0, 1]);
}
