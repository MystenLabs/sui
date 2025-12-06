// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::string_tests;

use std::string;
use std::unit_test::assert_eq;

#[test]
fun valid_utf8() {
    let sparkle_heart = vector[240, 159, 146, 150];
    let s = sparkle_heart.to_string();
    assert_eq!(s.length(), 4);
}

#[test, expected_failure(abort_code = string::EInvalidUTF8)]
fun invalid_utf8() {
    let no_sparkle_heart = vector[0, 159, 146, 150];
    let s = no_sparkle_heart.to_string();
    assert_eq!(s.length(), 1);
}

#[test]
fun substring() {
    let s = b"abcd".to_string();
    let sub = s.substring(2, 4);
    assert_eq!(sub, b"cd".to_string())
}

#[test, expected_failure(abort_code = string::EInvalidIndex)]
fun substring_invalid_boundary() {
    let sparkle_heart = vector[240, 159, 146, 150];
    let s = sparkle_heart.to_string();
    let _sub = s.substring(1, 4);
}

#[test, expected_failure(abort_code = string::EInvalidIndex)]
fun substring_invalid_index() {
    let s = b"abcd".to_string();
    let _sub = s.substring(4, 5);
}

#[test]
fun substring_empty() {
    let s = b"abcd".to_string();
    let sub = s.substring(4, 4);
    assert!(sub.is_empty())
}

#[test]
fun index_of() {
    let s = b"abcd".to_string();
    let r = b"bc".to_string();
    let p = s.index_of(&r);
    assert_eq!(p, 1)
}

#[test]
fun index_of_fail() {
    let s = b"abcd".to_string();
    let r = b"bce".to_string();
    let p = s.index_of(&r);
    assert_eq!(p, 4)
}

#[test]
fun append() {
    let mut s = b"abcd".to_string();
    s.append(b"ef".to_string());
    assert_eq!(s, b"abcdef".to_string())
}

#[test]
fun insert() {
    let mut s = b"abcd".to_string();
    s.insert(1, b"xy".to_string());
    assert_eq!(s, b"axybcd".to_string())
}

#[test]
fun into_bytes() {
    assert_eq!(b"abcd", b"abcd".to_string().into_bytes())
}
