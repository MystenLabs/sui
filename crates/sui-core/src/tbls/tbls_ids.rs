// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use itertools::Itertools;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::ops::Range;
use std::unreachable;
use sui_types::base_types::AuthorityName;
use sui_types::committee::VoteUnit;

/// Threshold BLS (tBLS) requires unique integer "share IDs". (tBLS will be used soon by the
/// randomness beacon.)
///
/// TBlsIds allocates IDs to validators, proportionally to their stake. E.g., if validator A has
/// stake x and validator B has stake 2x, validator B will receive ~twice than number of IDs A
/// received.
///
/// The maximal number of IDs is fixed. Since we are working with integers, some rounding is used
/// and thus the ratios between the number of IDs allocated to different validators is only a close
/// approximation to the ratios between the stakes of those validators. Also, some validators may
/// not receive any ID (e.g., if their stake is very small compared to the total stake).
///
pub struct TBlsIds {
    name_to_ids: HashMap<AuthorityName, Range<TBlsId>>,
    number_of_ids: u32,
}

type TBlsId = NonZeroU32;
const MAX_NUM_OF_SHARES: u16 = 1000;

impl TBlsIds {
    pub fn new(stakes: &[(AuthorityName, VoteUnit)]) -> Self {
        let total_stake: u64 = stakes.iter().map(|(_name, stake)| stake).sum();
        let deltas = stakes
            .iter()
            .sorted_by_key(|(name, _stake)| name)
            .filter_map(|(name, stake)| {
                // Better to multiply before dividing, to improve precision.
                let delta =
                    (((*stake as f64) * (MAX_NUM_OF_SHARES as f64)) / (total_stake as f64)).floor();
                if delta == 0.0 {
                    return None;
                }
                // delta < max(u32) because stake/total_stake <= 1.0 and MAX_NUM_OF_SHARES is u16.
                Some((*name, delta as u32))
            })
            .collect::<Vec<_>>();

        let number_of_ids = deltas.iter().map(|(_name, delta)| delta).sum();

        let name_to_ids = deltas
            .into_iter()
            .scan(NonZeroU32::new(1).unwrap(), |curr_index, (name, delta)| {
                let final_index = curr_index.checked_add(delta).unwrap_or_else(|| {
                    unreachable!("We should have no overflow because MAX_NUM_OF_SHARES is u16")
                });
                let ids = *curr_index..final_index;
                *curr_index = final_index;
                Some((name, ids))
            })
            .collect();

        TBlsIds {
            name_to_ids,
            number_of_ids,
        }
    }

    pub fn participants(&self) -> Vec<&AuthorityName> {
        self.name_to_ids.keys().into_iter().collect()
    }

    pub fn get_ids(&self, name: &AuthorityName) -> Option<&Range<TBlsId>> {
        self.name_to_ids.get(name)
    }

    pub fn num_of_shares(&self) -> u32 {
        self.number_of_ids
    }
}
