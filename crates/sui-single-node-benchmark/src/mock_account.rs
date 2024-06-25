// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::stream::FuturesUnordered;
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::crypto::{get_account_key_pair, AccountKeyPair};
use sui_types::object::Object;

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
        .map(|_| {
            tokio::spawn(async move {
                let (sender, keypair) = get_account_key_pair();
                let objects = (0..gas_object_num_per_account)
                    .map(|_| Object::with_owner_for_testing(sender))
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
