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
use bincode::{error::DecodeError, Decode, Encode};

pub(crate) fn encode<T: Encode>(x: &T) -> Vec<u8> {
    let config = bincode::config::standard()
        .with_big_endian()
        .with_fixed_int_encoding();

    bincode::encode_to_vec(x, config).expect("failed to serialize key")
}

pub(crate) fn decode<T: Decode<()>>(b: &[u8]) -> Result<T, DecodeError> {
    let config = bincode::config::standard()
        .with_big_endian()
        .with_fixed_int_encoding();

    Ok(bincode::decode_from_slice(b, config)?.0)
}

/// Modify the key `bs` in place to be the lexicographically next key.
///
/// Returns a boolean indicating whether the increment succeeded without overflow or not. If the
/// increment did result in an overflow, the key is reset to all zeros.
pub(crate) fn next(bs: &mut [u8]) -> bool {
    for b in bs.iter_mut().rev() {
        *b = b.wrapping_add(1);
        if *b != 0 {
            return true;
        }
    }

    false
}
