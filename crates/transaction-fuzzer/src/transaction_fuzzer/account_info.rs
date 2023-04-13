// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use indexmap::{IndexMap, IndexSet};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    crypto::{get_key_pair, AccountKeyPair},
};

pub struct AccountInfo {
    pub addr: SuiAddress,
    pub key: AccountKeyPair,
    pub gas_object_id: ObjectID,
    // address to stakedSui IDs
    pub staked_with: IndexMap<SuiAddress, IndexSet<ObjectID>>,
    pub staking_info: BTreeMap<ObjectID, (u64, u64)>,
    pub objects: BTreeSet<ObjectID>,
}

impl Default for AccountInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl AccountInfo {
    pub fn new() -> Self {
        let (addr, key): (_, AccountKeyPair) = get_key_pair();
        let gas_object_id = ObjectID::random();
        Self {
            addr,
            key,
            gas_object_id,
            staked_with: IndexMap::new(),
            staking_info: BTreeMap::new(),
            objects: BTreeSet::new(),
        }
    }

    pub fn add_stake(
        &mut self,
        staked_with: SuiAddress,
        stake_id: ObjectID,
        stake_amount: u64,
        current_epoch: u64,
    ) {
        let stakes = self
            .staked_with
            .entry(staked_with)
            .or_insert_with(IndexSet::new);
        stakes.insert(stake_id);
        self.staking_info
            .insert(stake_id, (stake_amount, current_epoch));
        self.objects.insert(stake_id);
    }

    pub fn remove_stake(&mut self, staked_with: SuiAddress, stake_id: ObjectID) {
        let stakes_with_validator = self.staked_with.get_mut(&staked_with).unwrap();
        self.objects.remove(&stake_id);
        stakes_with_validator.remove(&stake_id);
        // should we remove this? Seems like it would be nice to keep it. But it could grow large.
        // self.staking_info.remove(&stake_id);
        if stakes_with_validator.is_empty() {
            self.staked_with.remove(&staked_with);
        }
    }
}
