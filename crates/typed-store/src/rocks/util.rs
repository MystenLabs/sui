// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rocksdb::{CompactionDecision, MergeOperands};
use std::cmp::Ordering;

/// custom rocksdb merge operator used for storing objects with reference counts
/// important: reference count field must be 64-bit integer and must be last in struct declaration
/// should be used with immutable objects only
pub fn reference_count_merge_operator(
    _key: &[u8],
    stored_value: Option<&[u8]>,
    operands: &MergeOperands,
) -> Option<Vec<u8>> {
    let (mut value, mut ref_count) = stored_value.map_or((None, 0), deserialize_ref_count_value);

    for operand in operands {
        if !operand.is_empty() {
            let (new_value, delta) = deserialize_ref_count_value(operand);
            // assure original value is immutable
            assert!(value.is_none() || new_value.is_none() || value == new_value);
            if value.is_none() && new_value.is_some() {
                value = new_value;
            }
            ref_count += delta;
        }
    }
    match ref_count.cmp(&0) {
        Ordering::Greater => Some([value.unwrap_or(b""), &ref_count.to_le_bytes()].concat()),
        Ordering::Equal => Some(vec![]),
        Ordering::Less => Some(ref_count.to_le_bytes().to_vec()),
    }
}

pub fn empty_compaction_filter(_level: u32, _key: &[u8], value: &[u8]) -> CompactionDecision {
    if value.is_empty() {
        CompactionDecision::Remove
    } else {
        CompactionDecision::Keep
    }
}

pub fn is_ref_count_value(value: &[u8]) -> bool {
    value.is_empty() || value.len() == 8
}

fn deserialize_ref_count_value(bytes: &[u8]) -> (Option<&[u8]>, i64) {
    assert!(bytes.len() >= 8);
    let (value, rc_bytes) = bytes.split_at(bytes.len() - 8);
    let ref_count = i64::from_le_bytes(rc_bytes.try_into().unwrap());
    (if value.is_empty() { None } else { Some(value) }, ref_count)
}
