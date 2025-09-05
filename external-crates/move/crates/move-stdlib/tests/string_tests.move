// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::string_tests;

use std::string;

#[test]
fun test_valid_utf8() {
    let sparkle_heart = vector[240, 159, 146, 150];
    let s = sparkle_heart.to_string();
    assert!(s.length() == 4);
}

#[test]
#[expected_failure(abort_code = string::EInvalidUTF8)]
fun test_invalid_utf8() {
    let no_sparkle_heart = vector[0, 159, 146, 150];
    let s = no_sparkle_heart.to_string();
    assert!(s.length() == 1);
}

#[test]
fun test_substring() {
    let s = b"abcd".to_string();
    let sub = s.substring(2, 4);
    assert!(sub == b"cd".to_string())
}

#[test]
#[expected_failure(abort_code = string::EInvalidIndex)]
fun test_substring_invalid_boundary() {
    let sparkle_heart = vector[240, 159, 146, 150];
    let s = sparkle_heart.to_string();
    let _sub = s.substring(1, 4);
}

#[test]
#[expected_failure(abort_code = string::EInvalidIndex)]
fun test_substring_invalid_index() {
    let s = b"abcd".to_string();
    let _sub = s.substring(4, 5);
}

#[test]
fun test_substring_empty() {
    let s = b"abcd".to_string();
    let sub = s.substring(4, 4);
    assert!(sub.is_empty())
}

#[test]
fun test_index_of() {
    let s = b"abcd".to_string();
    let r = b"bc".to_string();
    let p = s.index_of(&r);
    assert!(p == 1)
}

#[test]
fun test_index_of_fail() {
    let s = b"abcd".to_string();
    let r = b"bce".to_string();
    let p = s.index_of(&r);
    assert!(p == 4)
}

#[test]
fun test_append() {
    let mut s = b"abcd".to_string();
    s.append(b"ef".to_string());
    assert!(s == b"abcdef".to_string())
}

#[test]
fun test_insert() {
    let mut s = b"abcd".to_string();
    s.insert(1, b"xy".to_string());
    assert!(s == b"axybcd".to_string())
}

#[test]
fun test_into_bytes() {
    assert!(b"abcd" == b"abcd".to_string().into_bytes())
}
