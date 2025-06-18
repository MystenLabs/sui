// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Defines the `MetadataValue` enum which represents common value types which
/// are used to describe metadata.
module sui::metadata_value;

use std::bcs;
use std::string::String;
use std::type_name::{Self, TypeName};

/// Enum which represents different types of metadata values.
public enum MetadataValue has copy, drop, store {
    /// Represents a `bool` value.
    Bool(bool),
    /// Represents a `u8` value.
    U8(u8),
    /// Represents a `u16` value.
    U16(u16),
    /// Represents a `u32` value.
    U32(u32),
    /// Represents a `u64` value.
    U64(u64),
    /// Represents a `u128` value.
    U128(u128),
    /// Represents a `u256` value.
    U256(u256),
    /// Represents an `address` value.
    Address(address),
    /// Represents a `String` value.
    String(String),
    /// Represents an `ID` value.
    ID(ID),
    /// Represents a generic `BCS` value.
    BCS(TypeName, vector<u8>),
}

// === Constructors ===

/// Creates a new `MetadataValue` representing a `bool` value.
public fun new_bool(value: bool): MetadataValue { MetadataValue::Bool(value) }

/// Creates a new `MetadataValue` representing a `u8` value.
public fun new_u8(value: u8): MetadataValue { MetadataValue::U8(value) }

/// Creates a new `MetadataValue` representing a `u16` value.
public fun new_u16(value: u16): MetadataValue { MetadataValue::U16(value) }

/// Creates a new `MetadataValue` representing a `u32` value.
public fun new_u32(value: u32): MetadataValue { MetadataValue::U32(value) }

/// Creates a new `MetadataValue` representing a `u64` value.
public fun new_u64(value: u64): MetadataValue { MetadataValue::U64(value) }

/// Creates a new `MetadataValue` representing a `u128` value.
public fun new_u128(value: u128): MetadataValue { MetadataValue::U128(value) }

/// Creates a new `MetadataValue` representing a `u256` value.
public fun new_u256(value: u256): MetadataValue { MetadataValue::U256(value) }

/// Creates a new `MetadataValue` representing an `address` value.
public fun new_address(value: address): MetadataValue { MetadataValue::Address(value) }

/// Creates a new `MetadataValue` representing a `String` value.
public fun new_string(value: String): MetadataValue { MetadataValue::String(value) }

/// Creates a new `MetadataValue` representing an `ID` value.
public fun new_id(value: ID): MetadataValue { MetadataValue::ID(value) }

/// Creates a new `MetadataValue` representing a generic `BCS` value.
/// Should only be used for values that are not already represented by the other
/// constructors, add challenge in reading the value as well as indexing.
public fun new_bcs<T: copy + drop + store>(value: T): MetadataValue {
    MetadataValue::BCS(type_name::get<T>(), bcs::to_bytes(&value))
}

// === Is Variant ===

/// Checks if the `MetadataValue` is a `bool` value.
public fun is_bool(value: &MetadataValue): bool {
    match (value) {
        MetadataValue::Bool(_) => true,
        _ => false,
    }
}

/// Checks if the `MetadataValue` is a `u8` value.
public fun is_u8(value: &MetadataValue): bool {
    match (value) {
        MetadataValue::U8(_) => true,
        _ => false,
    }
}

/// Checks if the `MetadataValue` is a `u16` value.
public fun is_u16(value: &MetadataValue): bool {
    match (value) {
        MetadataValue::U16(_) => true,
        _ => false,
    }
}

/// Checks if the `MetadataValue` is a `u32` value.
public fun is_u32(value: &MetadataValue): bool {
    match (value) {
        MetadataValue::U32(_) => true,
        _ => false,
    }
}

/// Checks if the `MetadataValue` is a `u64` value.
public fun is_u64(value: &MetadataValue): bool {
    match (value) {
        MetadataValue::U64(_) => true,
        _ => false,
    }
}

/// Checks if the `MetadataValue` is a `u128` value.
public fun is_u128(value: &MetadataValue): bool {
    match (value) {
        MetadataValue::U128(_) => true,
        _ => false,
    }
}

/// Checks if the `MetadataValue` is a `u256` value.
public fun is_u256(value: &MetadataValue): bool {
    match (value) {
        MetadataValue::U256(_) => true,
        _ => false,
    }
}

/// Checks if the `MetadataValue` is an `address` value.
public fun is_address(value: &MetadataValue): bool {
    match (value) {
        MetadataValue::Address(_) => true,
        _ => false,
    }
}

/// Checks if the `MetadataValue` is a `String` value.
public fun is_string(value: &MetadataValue): bool {
    match (value) {
        MetadataValue::String(_) => true,
        _ => false,
    }
}

/// Checks if the `MetadataValue` is an `ID` value.
public fun is_id(value: &MetadataValue): bool {
    match (value) {
        MetadataValue::ID(_) => true,
        _ => false,
    }
}

/// Checks if the `MetadataValue` is a generic `BCS` value.
public fun is_bcs(value: &MetadataValue): bool {
    match (value) {
        MetadataValue::BCS(_, _) => true,
        _ => false,
    }
}

// === Getters ===

/// Reads the `bool` value from the `MetadataValue`.
/// Aborts if the value is not a `bool`.
public fun read_bool(value: &MetadataValue): bool {
    match (value) {
        MetadataValue::Bool(value) => *value,
        _ => abort,
    }
}

/// Reads the `u8` value from the `MetadataValue`.
/// Aborts if the value is not a `u8`.
public fun read_u8(value: &MetadataValue): u8 {
    match (value) {
        MetadataValue::U8(value) => *value,
        _ => abort,
    }
}

/// Reads the `u16` value from the `MetadataValue`.
/// Aborts if the value is not a `u16`.
public fun read_u16(value: &MetadataValue): u16 {
    match (value) {
        MetadataValue::U16(value) => *value,
        _ => abort,
    }
}

/// Reads the `u32` value from the `MetadataValue`.
/// Aborts if the value is not a `u32`.
public fun read_u32(value: &MetadataValue): u32 {
    match (value) {
        MetadataValue::U32(value) => *value,
        _ => abort,
    }
}

/// Reads the `u64` value from the `MetadataValue`.
/// Aborts if the value is not a `u64`.
public fun read_u64(value: &MetadataValue): u64 {
    match (value) {
        MetadataValue::U64(value) => *value,
        _ => abort,
    }
}

/// Reads the `u128` value from the `MetadataValue`.
/// Aborts if the value is not a `u128`.
public fun read_u128(value: &MetadataValue): u128 {
    match (value) {
        MetadataValue::U128(value) => *value,
        _ => abort,
    }
}

/// Reads the `u256` value from the `MetadataValue`.
/// Aborts if the value is not a `u256`.
public fun read_u256(value: &MetadataValue): u256 {
    match (value) {
        MetadataValue::U256(value) => *value,
        _ => abort,
    }
}

/// Reads the `address` value from the `MetadataValue`.
/// Aborts if the value is not an `address`.
public fun read_address(value: &MetadataValue): address {
    match (value) {
        MetadataValue::Address(value) => *value,
        _ => abort,
    }
}

/// Reads the `String` value from the `MetadataValue`.
/// Aborts if the value is not a `String`.
public fun read_string(value: &MetadataValue): String {
    match (value) {
        MetadataValue::String(value) => *value,
        _ => abort,
    }
}

/// Reads the `ID` value from the `MetadataValue`.
/// Aborts if the value is not an `ID`.
public fun read_id(value: &MetadataValue): ID {
    match (value) {
        MetadataValue::ID(value) => *value,
        _ => abort,
    }
}

/// Reads the `BCS` value from the `MetadataValue`.
/// Aborts if the value is not a `BCS` value.
public fun read_bcs(value: &MetadataValue): (TypeName, vector<u8>) {
    match (value) {
        MetadataValue::BCS(type_name, value) => (*type_name, *value),
        _ => abort,
    }
}
