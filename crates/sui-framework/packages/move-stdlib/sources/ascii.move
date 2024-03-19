// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// The `ASCII` module defines basic string and char newtypes in Move that verify
/// that characters are valid ASCII, and that strings consist of only valid ASCII characters.
module std::ascii {
    use std::vector;
    use std::option::{Self, Option};

    /// An invalid ASCII character was encountered when creating an ASCII string.
    const EINVALID_ASCII_CHARACTER: u64 = 0x10000;

    /// The `String` struct holds a vector of bytes that all represent
    /// valid ASCII characters. Note that these ASCII characters may not all
    /// be printable. To determine if a `String` contains only "printable"
    /// characters you should use the `all_characters_printable` predicate
    /// defined in this module.
    struct String has copy, drop, store {
        bytes: vector<u8>,
    }

    /// An ASCII character.
    struct Char has copy, drop, store {
        byte: u8,
    }

    /// Convert a `byte` into a `Char` that is checked to make sure it is valid ASCII.
    public fun char(byte: u8): Char {
        assert!(is_valid_char(byte), EINVALID_ASCII_CHARACTER);
        Char { byte }
    }

    /// Convert a vector of bytes `bytes` into an `String`. Aborts if
    /// `bytes` contains non-ASCII characters.
    public fun string(bytes: vector<u8>): String {
       let x = try_string(bytes);
       assert!(
            option::is_some(&x),
            EINVALID_ASCII_CHARACTER
       );
       option::destroy_some(x)
    }

    /// Convert a vector of bytes `bytes` into an `String`. Returns
    /// `Some(<ascii_string>)` if the `bytes` contains all valid ASCII
    /// characters. Otherwise returns `None`.
    public fun try_string(bytes: vector<u8>): Option<String> {
        let len = vector::length(&bytes);
        let i = 0;
        while (i < len) {
            let possible_byte = *vector::borrow(&bytes, i);
            if (!is_valid_char(possible_byte)) return option::none();
            i = i + 1;
        };
        option::some(String { bytes })
    }

    /// Returns `true` if all characters in `string` are printable characters
    /// Returns `false` otherwise. Not all `String`s are printable strings.
    public fun all_characters_printable(string: &String): bool {
        let len = vector::length(&string.bytes);
        let i = 0;
        while (i < len) {
            let byte = *vector::borrow(&string.bytes, i);
            if (!is_printable_char(byte)) return false;
            i = i + 1;
        };
        true
    }

    public fun push_char(string: &mut String, char: Char) {
        vector::push_back(&mut string.bytes, char.byte);
    }

    public fun pop_char(string: &mut String): Char {
        Char { byte: vector::pop_back(&mut string.bytes) }
    }

    public fun length(string: &String): u64 {
        vector::length(as_bytes(string))
    }

    /// Get the inner bytes of the `string` as a reference
    public fun as_bytes(string: &String): &vector<u8> {
       &string.bytes
    }

    /// Unpack the `string` to get its backing bytes
    public fun into_bytes(string: String): vector<u8> {
       let String { bytes } = string;
       bytes
    }

    /// Unpack the `char` into its underlying byte.
    public fun byte(char: Char): u8 {
       let Char { byte } = char;
       byte
    }

    /// Returns `true` if `b` is a valid ASCII character. Returns `false` otherwise.
    public fun is_valid_char(b: u8): bool {
       b <= 0x7F
    }

    /// Returns `true` if `byte` is an printable ASCII character. Returns `false` otherwise.
    public fun is_printable_char(byte: u8): bool {
       byte >= 0x20 && // Disallow metacharacters
       byte <= 0x7E // Don't allow DEL metacharacter
    }
}
