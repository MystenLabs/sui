// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    committee::Committee,
    crypto::{get_key_pair, KeyPair},
};
use std::collections::BTreeMap;

pub fn make_committee_key() -> (Vec<KeyPair>, Committee) {
    make_committee_key_num(4)
}

pub fn make_committee_key_num(num: usize) -> (Vec<KeyPair>, Committee) {
    let mut authorities = BTreeMap::new();
    let mut keys = Vec::new();

    for _ in 0..num {
        let (_, inner_authority_key) = get_key_pair();
        authorities.insert(
            /* address */ *inner_authority_key.public_key_bytes(),
            /* voting right */ 1,
        );
        keys.push(inner_authority_key);
    }

    let committee = Committee::new(0, authorities);
    (keys, committee)
}
