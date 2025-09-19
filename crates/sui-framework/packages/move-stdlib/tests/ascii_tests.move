// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::ascii_tests;

use std::ascii;
use std::unit_test::assert_eq;

#[test]
fun ascii_chars() {
    let vec = vector::tabulate!(128, |i| {
        assert!(ascii::is_valid_char(i as u8));
        i as u8
    });

    let str = vec.to_ascii_string();
    assert_eq!(str.as_bytes().length(), 128);
    assert!(!str.all_characters_printable());
    assert_eq!(str.into_bytes().length(), 128);
}

#[test]
fun ascii_push_chars() {
    let mut str = vector[].to_ascii_string();
    128u8.do!(|i| {
        str.push_char(ascii::char(i));
    });

    assert_eq!(str.as_bytes().length(), 128);
    assert_eq!(str.length(), 128);
    assert!(!str.all_characters_printable());
}

#[test]
fun ascii_push_char_pop_char() {
    let mut i = 0;
    let end = 128;
    let mut str = vector[].to_ascii_string();

    while (i < end) {
        str.push_char(ascii::char(i));
        i = i + 1;
    };

    while (i > 0) {
        let char = str.pop_char();
        assert_eq!(ascii::byte(char), i - 1);
        i = i - 1;
    };

    assert_eq!(str.as_bytes().length(), 0);
    assert_eq!(str.length(), 0);
    assert!(str.all_characters_printable());
}

#[test]
fun printable_chars() {
    let mut i = 0x20;
    let end = 0x7E;
    let mut vec = vector[];

    while (i <= end) {
        assert!(ascii::is_printable_char(i));
        vec.push_back(i);
        i = i + 1;
    };

    let str = vec.to_ascii_string();
    assert!(str.all_characters_printable());
}

#[test]
fun printable_chars_dont_allow_tab() {
    let str = vector[0x09].to_ascii_string();
    assert!(!str.all_characters_printable());
}

#[test]
fun printable_chars_dont_allow_newline() {
    let str = vector[0x0A].to_ascii_string();
    assert!(!str.all_characters_printable());
}

#[test]
fun invalid_ascii_characters() {
    let mut i = 128u8;
    let end = 255u8;
    while (i < end) {
        let try_str = vector[i].try_to_ascii_string();
        assert!(try_str.is_none());
        i = i + 1;
    };
}

#[test]
fun nonvisible_chars() {
    let mut i = 0;
    let end = 0x09;
    while (i < end) {
        let str = vector[i].to_ascii_string();
        assert!(!str.all_characters_printable());
        i = i + 1;
    };

    let mut i = 0x0B;
    let end = 0x0F;
    while (i <= end) {
        let str = vector[i].to_ascii_string();
        assert!(!str.all_characters_printable());
        i = i + 1;
    };
}

#[test]
fun append() {
    let mut str = b"hello".to_ascii_string();
    str.append(b" world".to_ascii_string());

    assert_eq!(str, b"hello world".to_ascii_string());
}

#[test]
fun to_uppercase() {
    let str = b"azhello_world_!".to_ascii_string();
    assert_eq!(str.to_uppercase(), b"AZHELLO_WORLD_!".to_ascii_string());
}

#[test]
fun to_lowercase() {
    let str = b"AZHELLO_WORLD_!".to_ascii_string();
    assert_eq!(str.to_lowercase(), b"azhello_world_!".to_ascii_string());
}

#[test]
fun substring() {
    let str = b"hello world".to_ascii_string();
    assert_eq!(str.substring(0, 5), b"hello".to_ascii_string());
    assert_eq!(str.substring(6, 11), b"world".to_ascii_string());
}

#[test]
fun substring_len_one() {
    let str = b"hello world".to_ascii_string();
    assert_eq!(str.substring(0, 1), b"h".to_ascii_string());
    assert_eq!(str.substring(6, 7), b"w".to_ascii_string());
}

#[test]
fun substring_len_zero() {
    let str = b"hello world".to_ascii_string();
    assert!(str.substring(0, 0).is_empty());
}

#[test]
fun index_of() {
    let str = b"hello world orwell".to_ascii_string();
    assert_eq!(str.index_of(&b"hello".to_ascii_string()), 0);
    assert_eq!(str.index_of(&b"world".to_ascii_string()), 6);
    assert_eq!(str.index_of(&b"o".to_ascii_string()), 4);
    assert_eq!(str.index_of(&b"z".to_ascii_string()), str.length());
    assert_eq!(str.index_of(&b"o ".to_ascii_string()), 4);
    assert_eq!(str.index_of(&b"or".to_ascii_string()), 7);
    assert_eq!(str.index_of(&b"".to_ascii_string()), 0);
    assert_eq!(str.index_of(&b"orwell".to_ascii_string()), 12);
    assert_eq!(b"ororwell".to_ascii_string().index_of(&b"orwell".to_ascii_string()), 2);
}

#[test, expected_failure(abort_code = ascii::EInvalidIndex)]
fun substring_i_out_of_bounds_fail() {
    let str = b"hello world".to_ascii_string();
    str.substring(12, 13);
}

#[test, expected_failure(abort_code = ascii::EInvalidIndex)]
fun substring_j_lt_i_fail() {
    let str = b"hello world".to_ascii_string();
    str.substring(9, 8);
}

#[test, expected_failure(abort_code = ascii::EInvalidIndex)]
fun substring_j_out_of_bounds_fail() {
    let str = b"hello world".to_ascii_string();
    str.substring(9, 13);
}

#[test]
fun insert() {
    let mut str = b"hello".to_ascii_string();
    str.insert(5, b" world".to_ascii_string());
    assert_eq!(str, b"hello world".to_ascii_string());

    str.insert(5, b" cruel".to_ascii_string());
    assert_eq!(str, b"hello cruel world".to_ascii_string());
}

#[test]
fun insert_empty() {
    let mut str = b"hello".to_ascii_string();
    str.insert(5, b"".to_ascii_string());
    assert_eq!(str, b"hello".to_ascii_string());
}

#[test, expected_failure(abort_code = ascii::EInvalidIndex)]
fun insert_out_of_bounds_fail() {
    let mut str = b"hello".to_ascii_string();
    str.insert(6, b" world".to_ascii_string());
}
