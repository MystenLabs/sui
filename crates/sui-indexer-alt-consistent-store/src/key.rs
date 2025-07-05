// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The database uses bincode, with big-endian and fixint encoding as the serialization format for
//! database keys.
//!
//! This means that the lexicographic ordering of keys has the following properties:
//!
//! - Integers are generally compared in ascending order, although negative integers compare
//!   greater than positive integers.
//! - Structs are compared by lexicographically, as a tuple of their fields.
//! - Collections are first ordered by size, then by their elements in lexicographic order.
use bincode::Options;
use serde::{de::DeserializeOwned, Serialize};

#[inline]
pub(crate) fn encode<T: Serialize>(x: &T) -> Vec<u8> {
    bincode::DefaultOptions::new()
        .with_big_endian()
        .with_fixint_encoding()
        .serialize(x)
        .expect("failed to serialize key")
}

#[allow(dead_code)]
#[inline]
pub(crate) fn decode<T: DeserializeOwned>(b: &[u8]) -> Result<T, bincode::Error> {
    bincode::DefaultOptions::new()
        .with_big_endian()
        .with_fixint_encoding()
        .deserialize(b)
}

/// Modify the key `bs` in place to be the lexicographically next key.
///
/// Returns a boolean indicating whether the increment succeeded without overflow or not. If the
/// increment did result in an overflow, the key is reset to all zeros.
#[allow(dead_code)]
#[inline]
pub(crate) fn next(bs: &mut [u8]) -> bool {
    for b in bs.iter_mut().rev() {
        *b = b.wrapping_add(1);
        if *b != 0 {
            return true;
        }
    }

    false
}
