// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::tbls::tbls_ids::TBlsIds;
use fastcrypto::traits::{ToFromBytes, VerifyingKey};
use std::num::NonZeroU32;
use std::ops::Range;
use sui_types::base_types::AuthorityName;
use sui_types::committee::VoteUnit;
use sui_types::crypto::{AuthorityPublicKey, AuthorityPublicKeyBytes};

fn get_key(id: u16) -> AuthorityName {
    let mut buffer = [0u8; AuthorityPublicKey::LENGTH];
    buffer[0] = (id >> 8) as u8;
    buffer[1] = (id % 0x100) as u8;
    AuthorityPublicKeyBytes::from_bytes(&buffer).unwrap()
}

fn get_range(begin: u16, end: u16) -> Range<NonZeroU32> {
    let first = NonZeroU32::new(begin as u32).unwrap();
    let last = NonZeroU32::new(end as u32).unwrap();
    first..last
}

#[test]
fn test_1000_validators_with_1000_stake() {
    let stakes: Vec<(AuthorityName, VoteUnit)> =
        (1..=1000).into_iter().map(|i| (get_key(i), 1)).collect();

    let tbls_ids = TBlsIds::new(&stakes);
    for i in 1..=1000 {
        assert_eq!(*tbls_ids.get_ids(&get_key(i)).unwrap(), get_range(i, i + 1));
    }
    assert_eq!(tbls_ids.participants().len(), 1000);
    assert_eq!(tbls_ids.num_of_shares(), 1000);
}

#[test]
fn test_1000_validators_with_100000_stake() {
    let stakes: Vec<(AuthorityName, VoteUnit)> =
        (1..=1000).into_iter().map(|i| (get_key(i), 100)).collect();

    let tbls_ids = TBlsIds::new(&stakes);
    for i in 1..=1000 {
        assert_eq!(*tbls_ids.get_ids(&get_key(i)).unwrap(), get_range(i, i + 1));
    }
}

#[test]
fn test_100_validators_one_with_large_stake() {
    let mut stakes: Vec<(AuthorityName, VoteUnit)> =
        (1..=100).into_iter().map(|i| (get_key(i), 1)).collect();
    stakes.get_mut(0).unwrap().1 = 900;

    let tbls_ids = TBlsIds::new(&stakes);
    assert_eq!(*tbls_ids.get_ids(&get_key(1)).unwrap(), get_range(1, 901));
    assert_eq!(*tbls_ids.get_ids(&get_key(2)).unwrap(), get_range(901, 902));
}

#[test]
fn test_unsorted_100_validators_with_1000_stake() {
    let mut stakes: Vec<(AuthorityName, VoteUnit)> = (1..=100)
        .into_iter()
        .map(|i| (get_key(101 - i), 1))
        .collect();
    stakes.get_mut(0).unwrap().1 = 900;

    let tbls_ids = TBlsIds::new(&stakes);
    assert_eq!(
        *tbls_ids.get_ids(&get_key(100)).unwrap(),
        get_range(100, 1000)
    );
    assert_eq!(*tbls_ids.get_ids(&get_key(1)).unwrap(), get_range(1, 2));
}

#[test]
fn test_validator_without_shares() {
    let mut stakes: Vec<(AuthorityName, VoteUnit)> =
        (1..=10).into_iter().map(|i| (get_key(i), 100)).collect();
    // The next validator should not receive any id.
    stakes.push((get_key(11), 1));

    // Validators with stake of 100 should get floor(100*1000/1001) = 99.
    let tbls_ids = TBlsIds::new(&stakes);
    assert_eq!(*tbls_ids.get_ids(&get_key(1)).unwrap(), get_range(1, 100));
    assert!(tbls_ids.get_ids(&get_key(11)).is_none());

    assert_eq!(tbls_ids.participants().len(), 10);
    assert_eq!(tbls_ids.num_of_shares(), 990);
}
