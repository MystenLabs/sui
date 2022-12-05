// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use itertools::Itertools;
use std::collections::HashMap;
use std::num::NonZeroU32;
use sui_types::base_types::AuthorityName;
use sui_types::committee::StakeUnit;

const MAX_NUM_OF_SHARES: u32 = 1000;

type TBlsId = NonZeroU32;

pub struct TBlsIds {
    name_to_ids: HashMap<AuthorityName, Vec<TBlsId>>,
    number_of_ids: u32,
}

impl TBlsIds {
    pub fn new(stakes: &Vec<(AuthorityName, StakeUnit)>) -> Self {
        let total_stake = stakes.into_iter().fold(0, |acc, x| acc + x.1);
        // Indexes start from 1.
        let mut curr_index: u32 = 1;
        let mut result = HashMap::new();
        stakes
            .into_iter()
            .sorted_by(|a, b| Ord::cmp(&a.0, &b.0))
            .map(|(name, stake)| {
                // Better to multiply before dividing, to improve precision.
                let delta = ((*stake as f64) * (MAX_NUM_OF_SHARES as f64)) / (total_stake as f64);
                (name, delta.floor())
            })
            .for_each(|(name, delta)| {
                if delta == 0.0 {
                    return;
                }
                // Next 2 lines are safe since:
                // - delta >= 1.0 because of the above check.
                // - delta < max(u32) because stake/total_stake < 1.0 and MAX_NUM_OF_SHARES is u32.
                let range: Vec<u32> = (curr_index..(curr_index + (delta as u32))).collect();
                curr_index += delta as u32;
                // Next unwrap is safe since we start from curr_index = 1;
                let range = range
                    .into_iter()
                    .map(|i| NonZeroU32::new(i).unwrap())
                    .collect();
                result.insert(name.clone(), range);
            });
        TBlsIds {
            name_to_ids: result,
            number_of_ids: curr_index - 1,
        }
    }

    pub fn participants(&self) -> Vec<&AuthorityName> {
        self.name_to_ids.keys().into_iter().collect()
    }

    pub fn get_ids(&self, name: &AuthorityName) -> Option<&Vec<TBlsId>> {
        self.name_to_ids.get(name)
    }

    pub fn num_of_shares(&self) -> u32 {
        self.number_of_ids
    }
}
