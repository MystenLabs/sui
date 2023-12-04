// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::ed25519::Ed25519KeyPair;
use futures::stream::FuturesUnordered;
use rand::rngs::StdRng;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress, SUI_ADDRESS_LENGTH};
use sui_types::crypto::{get_key_pair_from_rng, AccountKeyPair};
use sui_types::object::Object;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub sender: SuiAddress,
    pub keypair: Arc<AccountKeyPair>,
    pub gas_objects: Arc<Vec<ObjectRef>>,
}

pub async fn batch_create_account_and_gas(
    num_accounts: u64,
    gas_object_num_per_account: u64,
) -> (BTreeMap<SuiAddress, Account>, Vec<Object>) {
    batch_parallel_create_account_and_gas(num_accounts, gas_object_num_per_account, 1).await
}

/// Generate \num_accounts accounts and for each account generate \gas_object_num_per_account gas objects.
/// Return all accounts along with a flattened list of all gas objects as genesis objects.
pub async fn batch_parallel_create_account_and_gas(
    num_accounts: u64,
    gas_object_num_per_account: u64,
    parallel_chunks: u64,
) -> (BTreeMap<SuiAddress, Account>, Vec<Object>) {
    let accounts_per_chunk = num_accounts / parallel_chunks;
    let tasks: FuturesUnordered<_> = (0..parallel_chunks)
        .map(|chunk_id| {
            let starting_id = chunk_id * accounts_per_chunk;
            tokio::spawn(async move {
                // let (sender, keypair) = get_account_key_pair();
                let chunk_acct_count = if chunk_id == parallel_chunks - 1 {
                    num_accounts - starting_id
                } else {
                    accounts_per_chunk
                };
                let mut rng = StdRng::from_seed([chunk_id as u8; 32]);
                (0..chunk_acct_count)
                    .map(|idx| {
                        let acct_id = starting_id + idx;
                        let (sender, keypair) =
                            get_key_pair_from_rng::<AccountKeyPair, _>(&mut rng);
                        let objects = (0..gas_object_num_per_account)
                            .map(|i| {
                                new_gas_object(acct_id * gas_object_num_per_account + i, sender)
                            })
                            .collect::<Vec<_>>();
                        (sender, keypair, objects)
                    })
                    .collect::<Vec<(SuiAddress, Ed25519KeyPair, Vec<_>)>>()
            })
        })
        .collect::<FuturesUnordered<_>>();
    let mut accounts = BTreeMap::new();
    let mut genesis_gas_objects = vec![];
    for task in tasks {
        let vec: Vec<(SuiAddress, Ed25519KeyPair, Vec<_>)> = task.await.unwrap();
        for (sender, keypair, gas_objects) in vec {
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
