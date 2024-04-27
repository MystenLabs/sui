// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// The `ASCII` module defines basic string and char newtypes in Move that verify
/// that characters are valid ASCII, and that strings consist of only valid ASCII characters.
module std::ascii {
    // Allows calling `.to_string()` to convert an `ascii::String` into as `string::String`
    public use fun std::string::from_ascii as String.to_string;

    /// An invalid ASCII character was encountered when creating an ASCII string.
    const EInvalidASCIICharacter: u64 = 0x10000;
    /// An invalid index was encountered when creating a substring.
    const EInvalidIndex: u64 = 0x10001;

    /// The `String` struct holds a vector of bytes that all represent
    /// valid ASCII characters. Note that these ASCII characters may not all
    /// be printable. To determine if a `String` contains only "printable"
    /// characters you should use the `all_characters_printable` predicate
    /// defined in this module.
    public struct String has copy, drop, store {
        bytes: vector<u8>,
    }

    /// An ASCII character.
    public struct Char has copy, drop, store {
        byte: u8,
    }

    /// Convert a `byte` into a `Char` that is checked to make sure it is valid ASCII.
    public fun char(byte: u8): Char {
        assert!(is_valid_char(byte), EInvalidASCIICharacter);
        Char { byte }
    }

    /// Convert a vector of bytes `bytes` into an `String`. Aborts if
    /// `bytes` contains non-ASCII characters.
    public fun string(bytes: vector<u8>): String {
       let x = try_string(bytes);
       assert!(x.is_some(), EInvalidASCIICharacter);
       x.destroy_some()
    }

    /// Convert a vector of bytes `bytes` into an `String`. Returns
    /// `Some(<ascii_string>)` if the `bytes` contains all valid ASCII
    /// characters. Otherwise returns `None`.
    public fun try_string(bytes: vector<u8>): Option<String> {
        let len = bytes.length();
        let mut i = 0;
        while (i < len) {
            let possible_byte = bytes[i];
            if (!is_valid_char(possible_byte)) return option::none();
            i = i + 1;
        };
        option::some(String { bytes })
    }

    /// Returns `true` if all characters in `string` are printable characters
    /// Returns `false` otherwise. Not all `String`s are printable strings.
    public fun all_characters_printable(string: &String): bool {
        let len = string.bytes.length();
        let mut i = 0;
        while (i < len) {
            let byte = string.bytes[i];
            if (!is_printable_char(byte)) return false;
            i = i + 1;
        };
        true
    }

    /// Push a `Char` to the end of the `string`.
    public fun push_char(string: &mut String, char: Char) {
        string.bytes.push_back(char.byte);
    }

    /// Pop a `Char` from the end of the `string`.
    public fun pop_char(string: &mut String): Char {
        Char { byte: string.bytes.pop_back() }
    }

    /// Returns the length of the `string` in bytes.
    public fun length(string: &String): u64 {
        string.as_bytes().length()
    }

    /// Append the `other` string to the end of `string`.
    public fun append(string: &mut String, other: String) {
        string.bytes.append(other.bytes)
    }

    /// Copy the slice of the `string` from `i` to `j` into a new `String`.
    public fun sub_string(string: &String, mut i: u64, j: u64): String {
        assert!(i <= string.length() && j <= string.length() && i <= j, EInvalidIndex);
        let mut bytes = vector[];
        while (i < j) {
            bytes.push_back(string.bytes[i]);
            i = i + 1;
        };
        String { bytes }
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

    /// Returns `true` if `b` is a valid ASCII character.
    /// Returns `false` otherwise.
    public fun is_valid_char(b: u8): bool {
       b <= 0x7F
    }

    /// Returns `true` if `byte` is an printable ASCII character.
    /// Returns `false` otherwise.
    public fun is_printable_char(byte: u8): bool {
       byte >= 0x20 && // Disallow metacharacters
       byte <= 0x7E // Don't allow DEL metacharacter
    }

    /// Returns `true` if `string` is empty.
    public fun is_empty(string: &String): bool {
        string.bytes.is_empty()
    }

    /// Returns `true` if `string` is an alphanumeric ASCII string.
    /// Returns `false` otherwise.
    public fun is_alphanumeric(string: &String): bool {
        let (mut i, len) = (0, string.length());
        while (i < len) {
            let byte = string.bytes[i];
            let is_alphanumeric =
                (byte >= 0x41 && byte <= 0x5A) || // A-Z
                (byte >= 0x61 && byte <= 0x7A) || // a-z
                (byte >= 0x30 && byte <= 0x39); // 0-9
            if (!is_alphanumeric) return false;
            i = i + 1;
        };

        true
    }
}
