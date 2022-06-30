// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    committee::Committee,
    crypto::{KeyPair},
};
use std::collections::BTreeMap;

pub fn make_committee_key<R>(rand: &mut R) -> (Vec<KeyPair>, Committee)
where
    R: rand::CryptoRng + rand::RngCore,
{
    make_committee_key_num(4, rand)
}

pub fn make_committee_key_num<R>(num: usize, rand: &mut R) -> (Vec<KeyPair>, Committee)
where
    R: rand::CryptoRng + rand::RngCore,
{
    let mut authorities = BTreeMap::new();
    let mut keys = Vec::new();

    for _ in 0..num {
        let (_, inner_authority_key) = KeyPair::get_key_pair_from_rng(rand);
        authorities.insert(
            /* address */ *inner_authority_key.public_key_bytes(),
            /* voting right */ 1,
        );
        keys.push(inner_authority_key);
    }

    let committee = Committee::new(0, authorities).unwrap();
    (keys, committee)
}
