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
        let (new_value, delta) = deserialize_ref_count_value(operand);
        assert!(value.is_none() || new_value.is_none() || value == new_value);
        if value.is_none() && new_value.is_some() {
            value = new_value;
        }
        ref_count += delta;
    }
    match ref_count.cmp(&0) {
        Ordering::Greater => Some([value.unwrap_or(b""), &ref_count.to_le_bytes()].concat()),
        Ordering::Equal => Some(vec![]),
        Ordering::Less => Some(ref_count.to_le_bytes().to_vec()),
    }
}

pub fn balance_merge_operator(
    _key: &[u8],
    stored_value: Option<&[u8]>,
    operands: &MergeOperands,
) -> Option<Vec<u8>> {
    let (mut balance, mut count) = stored_value.map_or((0, 0), deserialize_balance);
    for operand in operands {
        let (delta_balance, delta_count) = deserialize_balance(operand);
        balance += delta_balance;
        count += delta_count;
    }
    Some(serialize_balance_tuple(balance, count))
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
    if bytes.is_empty() {
        return (None, 0);
    }
    assert!(bytes.len() >= 8);
    let (value, rc_bytes) = bytes.split_at(bytes.len() - 8);
    let ref_count = i64::from_le_bytes(rc_bytes.try_into().unwrap());
    (if value.is_empty() { None } else { Some(value) }, ref_count)
}

fn deserialize_balance(bytes: &[u8]) -> (i64, i64) {
    if bytes.is_empty() {
        return (0, 0);
    }
    assert_eq!(bytes.len(), 16);
    let (balance_bytes, count_bytes) = bytes.split_at(8);
    let balance = i64::from_le_bytes(balance_bytes.try_into().unwrap());
    let count = i64::from_le_bytes(count_bytes.try_into().unwrap());
    (balance, count)
}

pub fn serialize_balance_tuple(balance: i64, count: i64) -> Vec<u8> {
    [balance.to_le_bytes().to_vec(), count.to_le_bytes().to_vec()].concat()
}

#[cfg(test)]
mod tests {
    use super::deserialize_ref_count_value;

    #[test]
    fn deserialize_ref_count_value_test() {
        assert_eq!(deserialize_ref_count_value(&[]), (None, 0));
        assert_eq!(
            deserialize_ref_count_value(b"\x01\0\0\0\0\0\0\0"),
            (None, 1)
        );
        assert_eq!(
            deserialize_ref_count_value(b"\xff\xff\xff\xff\xff\xff\xff\xff"),
            (None, -1)
        );
        assert_eq!(
            deserialize_ref_count_value(b"\xfe\xff\xff\xff\xff\xff\xff\xff"),
            (None, -2)
        );
        assert_eq!(
            deserialize_ref_count_value(b"test\x04\0\0\0\0\0\0\0"),
            (Some(b"test".as_ref()), 4)
        );
    }
}
