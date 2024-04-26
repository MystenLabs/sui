// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses V0=0x0 V1=0x0 V2=0x0 V3=0x0 V4=0x0 --accounts A

//# publish --upgradeable --sender A
module V0::ascii {
    const EINVALID_ASCII_CHARACTER: u64 = 0x10000;
    public struct String has copy, drop, store { bytes: vector<u8>, }
    public struct Char has copy, drop, store { byte: u8, }
    public fun char(byte: u8): Char {
        assert!(is_valid_char(byte), EINVALID_ASCII_CHARACTER);
        Char { byte }
    }
    public fun string(bytes: vector<u8>): String {
       let x = try_string(bytes);
       assert!(
            option::is_some(&x),
            EINVALID_ASCII_CHARACTER
       );
       option::destroy_some(x)
    }
    public fun try_string(bytes: vector<u8>): Option<String> {
       let len = vector::length(&bytes);
       let mut i = 0;
       while (i < len) {
           let possible_byte = *vector::borrow(&bytes, i);
           if (!is_valid_char(possible_byte)) return option::none();
           i = i + 1;
       };
       option::some(String { bytes })
    }
    public fun all_characters_printable(string: &String): bool {
       let len = vector::length(&string.bytes);
       let mut i = 0;
       while (i < len) {
           let byte = *vector::borrow(&string.bytes, i);
           if (!is_printable_char(byte)) return false;
           i = i + 1;
       };
       true
    }
    public fun push_char(string: &mut String, char: Char) { vector::push_back(&mut string.bytes, char.byte); }
    public fun pop_char(string: &mut String): Char { Char { byte: vector::pop_back(&mut string.bytes) } }
    public fun length(string: &String): u64 { vector::length(as_bytes(string)) }
    public fun as_bytes(string: &String): &vector<u8> { &string.bytes }
    public fun into_bytes(string: String): vector<u8> { let String { bytes } = string; bytes }
    public fun byte(char: Char): u8 { let Char { byte } = char; byte }
    public fun is_valid_char(b: u8): bool { b <= 0x7F }
    public fun is_printable_char(byte: u8): bool {
       byte >= 0x20 && // Disallow metacharacters
       byte <= 0x7E // Don't allow DEL metacharacter
    }
}

// Bytecode has changed so should fail in dep only mode
//# upgrade --package V0 --upgrade-capability 1,1 --sender A --policy dep_only
module V1::ascii {
    const EINVALID_ASCII_CHARACTER: u64 = 0x10000;
    public struct String has copy, drop, store { bytes: vector<u8>, }
    public struct Char has copy, drop, store { byte: u8, }
    public fun char(byte: u8): Char {
        assert!(is_valid_char(byte), EINVALID_ASCII_CHARACTER);
        Char { byte }
    }
    public fun string(bytes: vector<u8>): String {
       let x = try_string(bytes);
       assert!(
            option::is_some(&x),
            EINVALID_ASCII_CHARACTER
       );
       option::destroy_some(x)
    }
    public fun try_string(bytes: vector<u8>): Option<String> {
       let len = vector::length(&bytes);
       let mut i = 0;
       while (i < len) {
           let possible_byte = *vector::borrow(&bytes, i);
           if (!is_valid_char(possible_byte)) return option::none();
           i = i + 2; // <<<< CHANGED FROM 1 to 2 here
       };
       option::some(String { bytes })
    }
    public fun all_characters_printable(string: &String): bool {
       let len = vector::length(&string.bytes);
       let mut i = 0;
       while (i < len) {
           let byte = *vector::borrow(&string.bytes, i);
           if (!is_printable_char(byte)) return false;
           i = i + 1;
       };
       true
    }
    public fun push_char(string: &mut String, char: Char) { vector::push_back(&mut string.bytes, char.byte); }
    public fun pop_char(string: &mut String): Char { Char { byte: vector::pop_back(&mut string.bytes) } }
    public fun length(string: &String): u64 { vector::length(as_bytes(string)) }
    public fun as_bytes(string: &String): &vector<u8> { &string.bytes }
    public fun into_bytes(string: String): vector<u8> { let String { bytes } = string; bytes }
    public fun byte(char: Char): u8 { let Char { byte } = char; byte }
    public fun is_valid_char(b: u8): bool { b <= 0x7F }
    public fun is_printable_char(byte: u8): bool {
       byte >= 0x20 && // Disallow metacharacters
       byte <= 0x7E // Don't allow DEL metacharacter
    }
}

// NB: the previous upgrade failed, so even though it was in a stricter mode,
// we can still upgrade in a less strict mode since it wasn't committed.
//# upgrade --package V0 --upgrade-capability 1,1 --sender A --policy additive
module V1::ascii {
    const EINVALID_ASCII_CHARACTER: u64 = 0x10000;
    public struct String has copy, drop, store { bytes: vector<u8>, }
    public struct Char has copy, drop, store { byte: u8, }
    public fun char(byte: u8): Char {
        assert!(is_valid_char(byte), EINVALID_ASCII_CHARACTER);
        Char { byte }
    }
    public fun string(bytes: vector<u8>): String {
       let x = try_string(bytes);
       assert!(
            option::is_some(&x),
            EINVALID_ASCII_CHARACTER
       );
       option::destroy_some(x)
    }
    public fun try_string(bytes: vector<u8>): Option<String> {
       // CHANGED: Swapped the order of these two let declarations.
       let mut i = 0;
       let len = vector::length(&bytes);
       while (i < len) {
           let possible_byte = *vector::borrow(&bytes, i);
           if (!is_valid_char(possible_byte)) return option::none();
           i = i + 1;
       };
       option::some(String { bytes })
    }
    public fun all_characters_printable(string: &String): bool {
       let len = vector::length(&string.bytes);
       let mut i = 0;
       while (i < len) {
           let byte = *vector::borrow(&string.bytes, i);
           if (!is_printable_char(byte)) return false;
           i = i + 1;
       };
       true
    }
    public fun push_char(string: &mut String, char: Char) { vector::push_back(&mut string.bytes, char.byte); }
    public fun pop_char(string: &mut String): Char { Char { byte: vector::pop_back(&mut string.bytes) } }
    public fun length(string: &String): u64 { vector::length(as_bytes(string)) }
    public fun as_bytes(string: &String): &vector<u8> { &string.bytes }
    public fun into_bytes(string: String): vector<u8> { let String { bytes } = string; bytes }
    public fun byte(char: Char): u8 { let Char { byte } = char; byte }
    public fun is_valid_char(b: u8): bool { b <= 0x7F }
    public fun is_printable_char(byte: u8): bool {
       byte >= 0x20 && // Disallow metacharacters
       byte <= 0x7E // Don't allow DEL metacharacter
    }
}

// Shuffle them around -- should succeed
//# upgrade --package V0 --upgrade-capability 1,1 --sender A --policy additive
module V1::ascii {
    const EINVALID_ASCII_CHARACTER: u64 = 0x10000;
    public struct Char has copy, drop, store { byte: u8, }
    public struct String has copy, drop, store { bytes: vector<u8>, }
    public fun string(bytes: vector<u8>): String {
       let x = try_string(bytes);
       assert!(
            option::is_some(&x),
            EINVALID_ASCII_CHARACTER
       );
       option::destroy_some(x)
    }
    public fun char(byte: u8): Char {
        assert!(is_valid_char(byte), EINVALID_ASCII_CHARACTER);
        Char { byte }
    }
    public fun try_string(bytes: vector<u8>): Option<String> {
       let len = vector::length(&bytes);
       let mut i = 0;
       while (i < len) {
           let possible_byte = *vector::borrow(&bytes, i);
           if (!is_valid_char(possible_byte)) return option::none();
           i = i + 1;
       };
       option::some(String { bytes })
    }
    public fun all_characters_printable(string: &String): bool {
       let len = vector::length(&string.bytes);
       let mut i = 0;
       while (i < len) {
           let byte = *vector::borrow(&string.bytes, i);
           if (!is_printable_char(byte)) return false;
           i = i + 1;
       };
       true
    }
    public fun pop_char(string: &mut String): Char { Char { byte: vector::pop_back(&mut string.bytes) } }
    public fun push_char(string: &mut String, char: Char) { vector::push_back(&mut string.bytes, char.byte); }
    public fun as_bytes(string: &String): &vector<u8> { &string.bytes }
    public fun length(string: &String): u64 { vector::length(as_bytes(string)) }
    public fun into_bytes(string: String): vector<u8> { let String { bytes } = string; bytes }
    public fun byte(char: Char): u8 { let Char { byte } = char; byte }
    public fun is_printable_char(byte: u8): bool {
       byte >= 0x20 && // Disallow metacharacters
       byte <= 0x7E // Don't allow DEL metacharacter
    }
    public fun is_valid_char(b: u8): bool { b <= 0x7F }
}

// Add new things to the module -- the first one should fail in dep_only mode
//# upgrade --package V1 --upgrade-capability 1,1 --sender A --policy dep_only
module V2::ascii {
    const EINVALID_ASCII_CHARACTER: u64 = 0x10000;
    public struct Char has copy, drop, store { byte: u8, }
    public struct NewStruct { }
    public struct String has copy, drop, store { bytes: vector<u8>, }
    public fun string(bytes: vector<u8>): String {
       let x = try_string(bytes);
       assert!(
            option::is_some(&x),
            EINVALID_ASCII_CHARACTER
       );
       option::destroy_some(x)
    }
    public fun char(byte: u8): Char {
        assert!(is_valid_char(byte), EINVALID_ASCII_CHARACTER);
        Char { byte }
    }
    public fun try_string(bytes: vector<u8>): Option<String> {
       let len = vector::length(&bytes);
       let mut i = 0;
       while (i < len) {
           let possible_byte = *vector::borrow(&bytes, i);
           if (!is_valid_char(possible_byte)) return option::none();
           i = i + 1;
       };
       option::some(String { bytes })
    }
    public fun all_characters_printable(string: &String): bool {
       let len = vector::length(&string.bytes);
       let mut i = 0;
       while (i < len) {
           let byte = *vector::borrow(&string.bytes, i);
           if (!is_printable_char(byte)) return false;
           i = i + 1;
       };
       true
    }
    public fun pop_char(string: &mut String): Char { Char { byte: vector::pop_back(&mut string.bytes) } }
    public fun push_char(string: &mut String, char: Char) { vector::push_back(&mut string.bytes, char.byte); }
    public fun as_bytes(string: &String): &vector<u8> { &string.bytes }
    public fun new_function(_x: u64) { } // <<<<<< NEW FUNCTION
    public fun length(string: &String): u64 { vector::length(as_bytes(string)) }
    public fun into_bytes(string: String): vector<u8> { let String { bytes } = string; bytes }
    public fun byte(char: Char): u8 { let Char { byte } = char; byte }
    public fun is_printable_char(byte: u8): bool {
       byte >= 0x20 && // Disallow metacharacters
       byte <= 0x7E // Don't allow DEL metacharacter
    }
    public fun is_valid_char(b: u8): bool { b <= 0x7F }
}

// This one should succeed since we're in additive mode
//# upgrade --package V1 --upgrade-capability 1,1 --sender A --policy additive
module V2::ascii {
    const EINVALID_ASCII_CHARACTER: u64 = 0x10000;
    public struct Char has copy, drop, store { byte: u8, }
    public struct NewStruct { } // <<<<<<<<< NEW STRUCT
    public struct String has copy, drop, store { bytes: vector<u8>, }
    public fun string(bytes: vector<u8>): String {
       let x = try_string(bytes);
       assert!(
            option::is_some(&x),
            EINVALID_ASCII_CHARACTER
       );
       option::destroy_some(x)
    }
    public fun char(byte: u8): Char {
        assert!(is_valid_char(byte), EINVALID_ASCII_CHARACTER);
        Char { byte }
    }
    public fun try_string(bytes: vector<u8>): Option<String> {
       let len = vector::length(&bytes);
       let mut i = 0;
       while (i < len) {
           let possible_byte = *vector::borrow(&bytes, i);
           if (!is_valid_char(possible_byte)) return option::none();
           i = i + 1;
       };
       option::some(String { bytes })
    }
    public fun all_characters_printable(string: &String): bool {
       let len = vector::length(&string.bytes);
       let mut i = 0;
       while (i < len) {
           let byte = *vector::borrow(&string.bytes, i);
           if (!is_printable_char(byte)) return false;
           i = i + 1;
       };
       true
    }
    public fun pop_char(string: &mut String): Char { Char { byte: vector::pop_back(&mut string.bytes) } }
    public fun push_char(string: &mut String, char: Char) { vector::push_back(&mut string.bytes, char.byte); }
    public fun as_bytes(string: &String): &vector<u8> { &string.bytes }
    public fun new_function(_x: u64) { } // <<<<<< NEW FUNCTION
    public fun length(string: &String): u64 { vector::length(as_bytes(string)) }
    public fun into_bytes(string: String): vector<u8> { let String { bytes } = string; bytes }
    public fun byte(char: Char): u8 { let Char { byte } = char; byte }
    public fun is_printable_char(byte: u8): bool {
       byte >= 0x20 && // Disallow metacharacters
       byte <= 0x7E // Don't allow DEL metacharacter
    }
    public fun is_valid_char(b: u8): bool { b <= 0x7F }
}

// This one should succeed since we just changed it above
//# upgrade --package V2 --upgrade-capability 1,1 --sender A --policy dep_only
module V3::ascii {
    const EINVALID_ASCII_CHARACTER: u64 = 0x10000;
    public struct Char has copy, drop, store { byte: u8, }
    public struct NewStruct { } // <<<<<<<<< NEW STRUCT
    public struct String has copy, drop, store { bytes: vector<u8>, }
    public fun string(bytes: vector<u8>): String {
       let x = try_string(bytes);
       assert!(
            option::is_some(&x),
            EINVALID_ASCII_CHARACTER
       );
       option::destroy_some(x)
    }
    public fun char(byte: u8): Char {
        assert!(is_valid_char(byte), EINVALID_ASCII_CHARACTER);
        Char { byte }
    }
    public fun try_string(bytes: vector<u8>): Option<String> {
       let len = vector::length(&bytes);
       let mut i = 0;
       while (i < len) {
           let possible_byte = *vector::borrow(&bytes, i);
           if (!is_valid_char(possible_byte)) return option::none();
           i = i + 1;
       };
       option::some(String { bytes })
    }
    public fun all_characters_printable(string: &String): bool {
       let len = vector::length(&string.bytes);
       let mut i = 0;
       while (i < len) {
           let byte = *vector::borrow(&string.bytes, i);
           if (!is_printable_char(byte)) return false;
           i = i + 1;
       };
       true
    }
    public fun pop_char(string: &mut String): Char { Char { byte: vector::pop_back(&mut string.bytes) } }
    public fun push_char(string: &mut String, char: Char) { vector::push_back(&mut string.bytes, char.byte); }
    public fun as_bytes(string: &String): &vector<u8> { &string.bytes }
    public fun new_function(_x: u64) { } // <<<<<< NEW FUNCTION
    public fun length(string: &String): u64 { vector::length(as_bytes(string)) }
    public fun into_bytes(string: String): vector<u8> { let String { bytes } = string; bytes }
    public fun byte(char: Char): u8 { let Char { byte } = char; byte }
    public fun is_printable_char(byte: u8): bool {
       byte >= 0x20 && // Disallow metacharacters
       byte <= 0x7E // Don't allow DEL metacharacter
    }
    public fun is_valid_char(b: u8): bool { b <= 0x7F }
}

// This should now fail since we succeeded with `dep_only` -- we can no longer go back down the policy ordering.
//# upgrade --package V3 --upgrade-capability 1,1 --sender A --policy additive
module V4::ascii {
    const EINVALID_ASCII_CHARACTER: u64 = 0x10000;
    public struct Char has copy, drop, store { byte: u8, }
    public struct NewStruct { } // <<<<<<<<< NEW STRUCT
    public struct String has copy, drop, store { bytes: vector<u8>, }
    public fun string(bytes: vector<u8>): String {
       let x = try_string(bytes);
       assert!(
            option::is_some(&x),
            EINVALID_ASCII_CHARACTER
       );
       option::destroy_some(x)
    }
    public fun char(byte: u8): Char {
        assert!(is_valid_char(byte), EINVALID_ASCII_CHARACTER);
        Char { byte }
    }
    public fun try_string(bytes: vector<u8>): Option<String> {
       let len = vector::length(&bytes);
       let mut i = 0;
       while (i < len) {
           let possible_byte = *vector::borrow(&bytes, i);
           if (!is_valid_char(possible_byte)) return option::none();
           i = i + 1;
       };
       option::some(String { bytes })
    }
    public fun all_characters_printable(string: &String): bool {
       let len = vector::length(&string.bytes);
       let mut i = 0;
       while (i < len) {
           let byte = *vector::borrow(&string.bytes, i);
           if (!is_printable_char(byte)) return false;
           i = i + 1;
       };
       true
    }
    public fun pop_char(string: &mut String): Char { Char { byte: vector::pop_back(&mut string.bytes) } }
    public fun push_char(string: &mut String, char: Char) { vector::push_back(&mut string.bytes, char.byte); }
    public fun as_bytes(string: &String): &vector<u8> { &string.bytes }
    public fun new_function(_x: u64) { } // <<<<<< NEW FUNCTION
    public fun length(string: &String): u64 { vector::length(as_bytes(string)) }
    public fun into_bytes(string: String): vector<u8> { let String { bytes } = string; bytes }
    public fun byte(char: Char): u8 { let Char { byte } = char; byte }
    public fun is_printable_char(byte: u8): bool {
       byte >= 0x20 && // Disallow metacharacters
       byte <= 0x7E // Don't allow DEL metacharacter
    }
    public fun is_valid_char(b: u8): bool { b <= 0x7F }
}
