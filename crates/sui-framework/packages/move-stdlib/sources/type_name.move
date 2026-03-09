// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Functionality for converting Move types into values. Use with care!
module std::type_name;

use std::address;
use std::ascii::{Self, String};

/// ASCII Character code for the `:` (colon) symbol.
const ASCII_COLON: u8 = 58;
/// ASCII Character code for the `<` (less than) symbol.
const ASCII_LESS_THAN: u8 = 60;

/// ASCII Character code for the `v` (lowercase v) symbol.
const ASCII_V: u8 = 118;
/// ASCII Character code for the `e` (lowercase e) symbol.
const ASCII_E: u8 = 101;
/// ASCII Character code for the `c` (lowercase c) symbol.
const ASCII_C: u8 = 99;
/// ASCII Character code for the `t` (lowercase t) symbol.
const ASCII_T: u8 = 116;
/// ASCII Character code for the `o` (lowercase o) symbol.
const ASCII_O: u8 = 111;
/// ASCII Character code for the `r` (lowercase r) symbol.
const ASCII_R: u8 = 114;

/// The type is not from a package/module. It is a primitive type.
const ENonModuleType: u64 = 0;

public struct TypeName has copy, drop, store {
    /// String representation of the type. All types are represented
    /// using their source syntax:
    /// "u8", "u64", "bool", "address", "vector", and so on for primitive types.
    /// Struct types are represented as fully qualified type names; e.g.
    /// `00000000000000000000000000000001::string::String` or
    /// `0000000000000000000000000000000a::module_name1::type_name1<0000000000000000000000000000000a::module_name2::type_name2<u64>>`
    /// Addresses are hex-encoded lowercase values of length ADDRESS_LENGTH (16, 20, or 32 depending on the Move platform)
    name: String,
}

/// Return a value representation of the type `T`. Package IDs that appear in fully qualified type
/// names in the output from this function are defining IDs (the ID of the package in storage that
/// first introduced the type).
public native fun with_defining_ids<T>(): TypeName;

/// Return a value representation of the type `T`. Package IDs that appear in fully qualified type
/// names in the output from this function are original IDs (the ID of the first version of
/// the package, even if the type in question was introduced in a later upgrade).
public native fun with_original_ids<T>(): TypeName;

/// Like `with_defining_ids`, this accesses the package ID that original defined the type `T`.
public native fun defining_id<T>(): address;

/// Like `with_original_ids`, this accesses the original ID of the package that defines type `T`,
/// even if the type was introduced in a later version of the package.
public native fun original_id<T>(): address;

/// Returns true iff the TypeName represents a primitive type, i.e. one of
/// u8, u16, u32, u64, u128, u256, bool, address, vector.
public fun is_primitive(self: &TypeName): bool {
    let bytes = self.name.as_bytes();
    bytes == &b"bool" ||
        bytes == &b"u8" ||
        bytes == &b"u16" ||
        bytes == &b"u32" ||
        bytes == &b"u64" ||
        bytes == &b"u128" ||
        bytes == &b"u256" ||
        bytes == &b"address" ||
        (
            bytes.length() >= 6 &&
            bytes[0] == ASCII_V &&
            bytes[1] == ASCII_E &&
            bytes[2] == ASCII_C &&
            bytes[3] == ASCII_T &&
            bytes[4] == ASCII_O &&
            bytes[5] == ASCII_R,
        )
}

/// Get the String representation of `self`
public fun as_string(self: &TypeName): &String {
    &self.name
}

/// Get Address string (Base16 encoded), first part of the TypeName.
/// Aborts if given a primitive type.
public fun address_string(self: &TypeName): String {
    assert!(!self.is_primitive(), ENonModuleType);

    // Base16 (string) representation of an address has 2 symbols per byte.
    let len = address::length() * 2;
    let str_bytes = self.name.as_bytes();
    let mut addr_bytes = vector[];
    let mut i = 0;

    // Read `len` bytes from the type name and push them to addr_bytes.
    while (i < len) {
        addr_bytes.push_back(str_bytes[i]);
        i = i + 1;
    };

    ascii::string(addr_bytes)
}

/// Get name of the module.
/// Aborts if given a primitive type.
public fun module_string(self: &TypeName): String {
    assert!(!self.is_primitive(), ENonModuleType);

    // Starts after address and a double colon: `<addr as HEX>::`
    let mut i = address::length() * 2 + 2;
    let str_bytes = self.name.as_bytes();
    let mut module_name = vector[];
    let colon = ASCII_COLON;
    loop {
        let char = &str_bytes[i];
        if (char != &colon) {
            module_name.push_back(*char);
            i = i + 1;
        } else {
            break
        }
    };

    ascii::string(module_name)
}

/// Get name of the datatype (struct or enum).
/// Aborts if given a primitive type.
public fun datatype_string(self: &TypeName): String {
    assert!(!self.is_primitive(), ENonModuleType);

    // Starts after address and a double colon: `<addr as HEX>::`
    let mut i = address::length() * 2 + 2;
    let str_bytes = self.name.as_bytes();
    let str_bytes_len = str_bytes.length();
    let colon = ASCII_COLON;

    // Skip past the `<module_name>::` to the datatype name.
    // The asserts should never fail, since all non-primitive types should have two colons.
    while (&str_bytes[i] != &colon) i = i + 1;
    i = i + 1;
    assert!(&str_bytes[i] == &colon);
    i = i + 1;
    assert!(&str_bytes[i] != &colon);

    // Take all characters until the type parameters start at `<`, or until the end of the string.
    let mut datatype_name = vector[];
    let lt = ASCII_LESS_THAN;
    loop {
        let char = &str_bytes[i];
        if (char == &lt) break;

        datatype_name.push_back(*char);
        i = i + 1;
        if (i >= str_bytes_len) break;
    };

    ascii::string(datatype_name)
}

/// Convert `self` into its inner String
public fun into_string(self: TypeName): String {
    self.name
}

// === Deprecated ===

#[deprecated(note = b"Renamed to `with_defining_ids` for clarity.")]
public fun get<T>(): TypeName {
    with_defining_ids<T>()
}

#[deprecated(note = b"Renamed to `with_original_ids` for clarity.")]
public fun get_with_original_ids<T>(): TypeName {
    with_original_ids<T>()
}

#[deprecated(note = b"Renamed to `as_string` for consistency.")]
public fun borrow_string(self: &TypeName): &String {
    self.as_string()
}

#[deprecated(note = b"Renamed to `address_string` for consistency.")]
public fun get_address(self: &TypeName): String {
    self.address_string()
}

#[deprecated(note = b"Renamed to `module_string` for consistency.")]
public fun get_module(self: &TypeName): String {
    self.module_string()
}
