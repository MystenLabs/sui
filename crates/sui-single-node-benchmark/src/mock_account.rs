// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rand::rngs::StdRng;
use rand::SeedableRng;
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress, SUI_ADDRESS_LENGTH};
use sui_types::crypto::{get_key_pair_from_rng, AccountKeyPair};
use sui_types::object::Object;

#[derive(Clone, Debug)]
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
    // let tasks: FuturesUnordered<_> = (0..num_accounts)
    //     .map(|idx| {
    //         let starting_id = idx * gas_object_num_per_account;
    //         tokio::spawn(async move {
    //             let (sender, keypair) = get_account_key_pair();
    //             // let (sender, keypair) = get_key_pair_from_rng(&mut rng);
    //             let objects = (0..gas_object_num_per_account)
    //                 .map(|i| new_gas_object(starting_id + i, sender))
    //                 .collect::<Vec<_>>();
    //             (sender, keypair, objects)
    //         })
    //     })
    //     .collect();

    // deterministically generate accounts and gas
    let mut rng = StdRng::from_seed([0; 32]);
    let mut tasks = vec![];
    // TODO: is there a way to do this in parallel, while maintaining determinism?
    for idx in 0..num_accounts {
        let starting_id = idx * gas_object_num_per_account;
        let (sender, keypair) = get_key_pair_from_rng::<AccountKeyPair, _>(&mut rng);
        let objects = (0..gas_object_num_per_account)
            .map(|i| new_gas_object(starting_id + i, sender))
            .collect::<Vec<_>>();
        tasks.push((sender, keypair, objects));
    }
    let mut accounts = BTreeMap::new();
    let mut genesis_gas_objects = vec![];
    for (sender, keypair, gas_objects) in tasks {
        // let (sender, keypair, gas_objects) = task.await.unwrap();
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

    // // print object reference of each genesis gas object
    // for gas_object in &genesis_gas_objects {
    //     println!(
    //         "gas object reference: {:?}",
    //         gas_object.compute_object_reference()
    //     );
    // }
    // // print account addresses
    // for account in accounts.keys() {
    //     println!("account address: {:?}", account);
    // }
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
