// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use futures::stream::FuturesUnordered;
use rand::{rngs::StdRng, SeedableRng};
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress, SUI_ADDRESS_LENGTH},
    crypto::{get_key_pair_from_rng, AccountKeyPair},
    object::Object,
};

#[derive(Clone)]
pub struct Account {
    pub sender: SuiAddress,
    pub keypair: Arc<AccountKeyPair>,
    pub gas_objects: Arc<Vec<ObjectRef>>,
}

/// Generate \num_accounts accounts and for each account generate \gas_object_num_per_account gas objects.
/// Return all accounts along with a flattened list of all gas objects as genesis objects.
pub async fn batch_create_account_and_gas(
    num_accounts: u64,
    gas_object_num_per_account: u64,
) -> (BTreeMap<SuiAddress, Account>, Vec<Object>) {
    let tasks: FuturesUnordered<_> = (0..num_accounts)
        .map(|idx| {
            let starting_id = idx * gas_object_num_per_account;
            tokio::spawn(async move {
                // let (sender, keypair) = get_account_key_pair();
                // Deterministic run with seeded randomness
                let mut rng = StdRng::from_seed([idx as u8; 32]);
                let (sender, keypair) = get_key_pair_from_rng::<AccountKeyPair, _>(&mut rng);
                let objects = (0..gas_object_num_per_account)
                    .map(|i| new_gas_object(starting_id + i, sender))
                    .collect::<Vec<_>>();
                (sender, keypair, objects)
            })
        })
        .collect();
    let mut accounts = BTreeMap::new();
    let mut genesis_gas_objects = vec![];
    for task in tasks {
        let (sender, keypair, gas_objects) = task.await.unwrap();
        let gas_object_refs: Vec<_> = gas_objects
            .iter()
            .map(|o| o.compute_object_reference())
            .collect();
        accounts.insert(
            sender,
            Account {
                sender,
                keypair: Arc::new(keypair),
                gas_objects: Arc::new(gas_object_refs),
            },
        );
        genesis_gas_objects.extend(gas_objects);
    }
    (accounts, genesis_gas_objects)
}

fn new_gas_object(idx: u64, owner: SuiAddress) -> Object {
    // Predictable and cheaper way of generating object IDs for benchmarking.
    let mut id_bytes = [0u8; SUI_ADDRESS_LENGTH];
    let idx_bytes = idx.to_le_bytes();
    id_bytes[0] = 255;
    id_bytes[1..idx_bytes.len() + 1].copy_from_slice(&idx_bytes);
    let object_id = ObjectID::from_bytes(id_bytes).unwrap();
    Object::with_id_owner_for_testing(object_id, owner)
}
