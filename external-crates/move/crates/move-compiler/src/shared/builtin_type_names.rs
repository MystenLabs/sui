// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub const ADDRESS: &str = "address";
pub const SIGNER: &str = "signer";
pub const U_8: &str = "u8";
pub const U_16: &str = "u16";
pub const U_32: &str = "u32";
pub const U_64: &str = "u64";
pub const U_128: &str = "u128";
pub const U_256: &str = "u256";
pub const I_8: &str = "i8";
pub const I_16: &str = "i16";
pub const I_32: &str = "i32";
pub const I_64: &str = "i64";
pub const I_128: &str = "i128";
pub const BOOL: &str = "bool";
pub const VECTOR: &str = "vector";

pub const UNSIGNED_INT_SUFFIXES: &[&str] = &[U_8, U_16, U_32, U_64, U_128, U_256];
pub const SIGNED_INT_SUFFIXES: &[&str] = &[I_8, I_16, I_32, I_64, I_128];

pub const PRIMITIVE_TYPES: &[&str] = &[U_8, U_16, U_32, U_64, U_128, U_256, BOOL, VECTOR];
