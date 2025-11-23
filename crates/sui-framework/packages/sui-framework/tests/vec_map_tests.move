// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::vec_map_tests;

use std::unit_test::assert_eq;
use sui::vec_map::{Self, VecMap};

#[test, expected_failure(abort_code = vec_map::EKeyAlreadyExists)]
fun duplicate_key_abort() {
    let mut m = vec_map::empty();
    m.insert(1u64, true);
    m.insert(1, false);
}

#[test, expected_failure(abort_code = vec_map::EKeyDoesNotExist)]
fun nonexistent_key_get() {
    let mut m = vec_map::empty();
    m.insert(1u64, true);
    let k = 2;
    let _v = &m[&k];
}

#[test, expected_failure(abort_code = vec_map::EKeyDoesNotExist)]
fun nonexistent_key_get_idx_or_abort() {
    let mut m = vec_map::empty();
    m.insert(1, true);
    let k = 2u64;
    let _idx = m.get_idx(&k);
}

#[test, expected_failure(abort_code = vec_map::EIndexOutOfBounds)]
fun out_of_bounds_get_entry_by_idx() {
    let mut m = vec_map::empty();
    m.insert(1u64, true);
    let idx = 1;
    let (_key, _val) = m.get_entry_by_idx(idx);
}

#[test, expected_failure(abort_code = vec_map::EIndexOutOfBounds)]
fun out_of_bounds_remove_entry_by_idx() {
    let mut m = vec_map::empty();
    m.insert(10u64, true);
    let idx = 1;
    let (_key, _val) = m.remove_entry_by_idx(idx);
}

#[test]
fun remove_entry_by_idx() {
    let mut m = vec_map::empty();
    m.insert(5u64, 50u64);
    m.insert(6, 60);
    m.insert(7, 70);

    let (key, val) = m.remove_entry_by_idx(0);
    assert_eq!(key, 5);
    assert_eq!(val, 50);
    assert_eq!(m.length(), 2);

    let (key, val) = m.remove_entry_by_idx(1);
    assert_eq!(key, 7);
    assert_eq!(val, 70);
    assert_eq!(m.length(), 1);
}

#[test, expected_failure(abort_code = vec_map::EMapNotEmpty)]
fun destroy_non_empty() {
    let mut m = vec_map::empty();
    m.insert(1u64, true);
    m.destroy_empty()
}

#[test]
fun destroy_empty() {
    let m: VecMap<u64, u64> = vec_map::empty();
    assert!(m.is_empty());
    m.destroy_empty()
}

#[test]
fun smoke() {
    let mut m = vec_map::empty();
    10u64.do!(|i| {
        let k = i + 2;
        let v = i + 5;
        m.insert(k, v);
    });
    assert!(!m.is_empty());
    assert_eq!(m.length(), 10);
    // make sure the elements are as expected in all of the getter APIs we expose
    10u64.do!(|i| {
        let k = i + 2;
        assert!(m.contains(&k));
        let v = m[&k];
        assert_eq!(v, i + 5);
        assert_eq!(m.get_idx(&k), i);
        let (other_k, other_v) = m.get_entry_by_idx(i);
        assert_eq!(*other_k, k);
        assert_eq!(*other_v, v);
    });
    // remove all the elements
    let (keys, values) = (copy m).into_keys_values();
    10u64.do!(|i| {
        let k = i + 2;
        let (other_k, v) = m.remove(&k);
        assert_eq!(k, other_k);
        assert_eq!(v, i + 5);
        assert_eq!(keys[i], k);
        assert_eq!(values[i], v);
    });
}

#[test]
fun return_list_of_keys() {
    let mut m = vec_map::empty();

    assert_eq!(m.keys(), vector[]);

    m.insert(1u64, true);
    m.insert(5, false);

    assert_eq!(m.keys(), vector[1, 5]);
}

#[test, allow(lint(collection_equality))]
fun round_trip() {
    let mut m = vec_map::empty();
    assert_eq!(m, vec_map::from_keys_values(vector[], vector[]));
    50u64.do!(|i| {
        let mut s = b"";
        let k = i + 2;
        s.append(b"x");
        m.insert(k, s);
        let (keys, values) = m.into_keys_values();
        let m2 = vec_map::from_keys_values(keys, values);
        assert_eq!(m, m2);
    });
}

#[test, expected_failure(abort_code = vec_map::EUnequalLengths)]
fun mismatched_key_values_1() {
    let keys = vector[1];
    let values = vector[];
    vec_map::from_keys_values<u64, u64>(keys, values);
}

#[test, expected_failure(abort_code = vec_map::EUnequalLengths)]
fun mismatched_key_values_2() {
    let keys = vector[];
    let values = vector[1];
    vec_map::from_keys_values<u64, u64>(keys, values);
}

#[test, expected_failure(abort_code = vec_map::EUnequalLengths)]
fun mismatched_key_values_3() {
    let keys = vector[1, 2, 3, 4, 5, 6];
    let values = {
        let mut v = keys;
        v.pop_back();
        v
    };
    vec_map::from_keys_values<u64, u64>(keys, values);
}

#[test, expected_failure(abort_code = vec_map::EKeyAlreadyExists)]
fun from_keys_values_duplicate_key_abort() {
    vec_map::from_keys_values<u64, address>(vector[1, 0, 1], vector[@0, @1, @2]);
}
