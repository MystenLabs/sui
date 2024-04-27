// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module std::ascii_tests {
    use std::ascii;

    #[test]
    fun test_ascii_chars() {
        let mut i = 0;
        let end = 128;
        let mut vec = vector[];

        while (i < end) {
            assert!(ascii::is_valid_char(i), 0);
            vec.push_back(i);
            i = i + 1;
        };

        let str = vec.to_ascii_string();
        assert!(str.as_bytes().length() == 128, 0);
        assert!(!str.all_characters_printable(), 1);
        assert!(str.into_bytes().length() == 128, 2);
    }

    #[test]
    fun test_ascii_push_chars() {
        let mut i = 0;
        let end = 128;
        let mut str = vector[].to_ascii_string();

        while (i < end) {
            str.push_char(ascii::char(i));
            i = i + 1;
        };

        assert!(str.as_bytes().length() == 128, 0);
        assert!(str.length() == 128, 0);
        assert!(!str.all_characters_printable(), 1);
    }

    #[test]
    fun test_ascii_push_char_pop_char() {
        let mut i = 0;
        let end = 128;
        let mut str = vector[].to_ascii_string();

        while (i < end) {
            str.push_char(ascii::char(i));
            i = i + 1;
        };

        while (i > 0) {
            let char = str.pop_char();
            assert!(ascii::byte(char) == i - 1, 0);
            i = i - 1;
        };

        assert!(str.as_bytes().length() == 0, 0);
        assert!(str.length() == 0, 0);
        assert!(str.all_characters_printable(), 1);
    }

    #[test]
    fun test_printable_chars() {
        let mut i = 0x20;
        let end = 0x7E;
        let mut vec = vector[];

        while (i <= end) {
            assert!(ascii::is_printable_char(i), 0);
            vec.push_back(i);
            i = i + 1;
        };

        let str = vec.to_ascii_string();
        assert!(str.all_characters_printable(), 0);
    }

    #[test]
    fun printable_chars_dont_allow_tab() {
        let str = vector[0x09].to_ascii_string();
        assert!(!str.all_characters_printable(), 0);
    }

    #[test]
    fun printable_chars_dont_allow_newline() {
        let str = vector[0x0A].to_ascii_string();
        assert!(!str.all_characters_printable(), 0);
    }

    #[test]
    fun test_invalid_ascii_characters() {
        let mut i = 128u8;
        let end = 255u8;
        while (i < end) {
            let try_str = vector[i].try_to_ascii_string();
            assert!(try_str.is_none(), 0);
            i = i + 1;
        };
    }

    #[test]
    fun test_nonvisible_chars() {
        let mut i = 0;
        let end = 0x09;
        while (i < end) {
            let str = vector[i].to_ascii_string();
            assert!(!str.all_characters_printable(), 0);
            i = i + 1;
        };

        let mut i = 0x0B;
        let end = 0x0F;
        while (i <= end) {
            let str = vector[i].to_ascii_string();
            assert!(!str.all_characters_printable(), 0);
            i = i + 1;
        };
    }

    #[test]
    fun test_append() {
        let mut str = b"hello".to_ascii_string();
        str.append(b" world".to_ascii_string());

        assert!(str == b"hello world".to_ascii_string(), 0);
    }

    #[test]
    fun test_is_alphanumeric() {
        assert!(b"ahelloz".to_ascii_string().is_alphanumeric(), 0);
        assert!(b"AHELLOZ".to_ascii_string().is_alphanumeric(), 3);
        assert!(b"0123".to_ascii_string().is_alphanumeric(), 4);
        assert!(!b"!".to_ascii_string().is_alphanumeric(), 2);
        assert!(!b" ".to_ascii_string().is_alphanumeric(), 1);
    }

    #[test]
    fun test_sub_string() {
        let str = b"hello world".to_ascii_string();
        assert!(str.sub_string(0, 5) == b"hello".to_ascii_string(), 0);
        assert!(str.sub_string(6, 11) == b"world".to_ascii_string(), 1);
    }

    #[test]
    fun test_sub_string_len_one() {
        let str = b"hello world".to_ascii_string();
        assert!(str.sub_string(0, 1) == b"h".to_ascii_string(), 0);
        assert!(str.sub_string(6, 7) == b"w".to_ascii_string(), 1);
    }

    #[test]
    fun test_sub_string_len_zero() {
        let str = b"hello world".to_ascii_string();
        assert!(str.sub_string(0, 0).is_empty(), 0);
    }

    #[test, expected_failure(abort_code = ascii::EInvalidIndex)]
    fun test_sub_string_i_out_of_bounds_fail() {
        let str = b"hello world".to_ascii_string();
        str.sub_string(12, 13);
    }

    #[test, expected_failure(abort_code = ascii::EInvalidIndex)]
    fun test_sub_string_j_lt_i_fail() {
        let str = b"hello world".to_ascii_string();
        str.sub_string(9, 8);
    }

    #[test, expected_failure(abort_code = ascii::EInvalidIndex)]
    fun test_sub_string_j_out_of_bounds_fail() {
        let str = b"hello world".to_ascii_string();
        str.sub_string(9, 13);
    }
}
