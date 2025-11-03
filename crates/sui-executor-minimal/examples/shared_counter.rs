// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use anyhow::Result;
use sui_executor_minimal::{InMemoryObjectStore, MinimalExecutor};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::{
    base_types::{SuiAddress, random_object_ref},
    crypto::{AccountKeyPair, get_key_pair},
    effects::TransactionEffectsAPI,
    object::{Object, Owner},
    transaction::DEFAULT_VALIDATOR_GAS_PRICE,
};

fn main() -> Result<()> {
    println!("=== Sui Minimal Executor: Shared Counter Example ===\n");

    let sender = SuiAddress::random_for_testing_only();
    let (_, sender_keypair): (_, AccountKeyPair) = get_key_pair();

    let gas_object_id = random_object_ref().0;
    let gas_object = Object::with_id_owner_gas_for_testing(gas_object_id, sender, 10_000_000_000);
    let mut gas_object_ref = gas_object.compute_object_reference();

    println!("Created sender: {}", sender);
    println!("Created gas object: {:?}\n", gas_object_ref);

    let store = InMemoryObjectStore::new_with_genesis_packages(
        vec![(gas_object.id(), gas_object)].into_iter().collect(),
    );
    println!("Created store with framework packages loaded\n");

    println!("Initializing executor...");
    let executor = MinimalExecutor::new_for_testing()?;
    println!(
        "Executor initialized with protocol version: {:?}\n",
        executor.protocol_config().version
    );

    println!("Step 1: Publishing the basics package...");
    let tx_builder =
        TestTransactionBuilder::new(sender, gas_object_ref, DEFAULT_VALIDATOR_GAS_PRICE)
            .publish_examples("basics");
    let publish_tx = tx_builder.build_and_sign(&sender_keypair);

    let publish_result =
        executor.execute_transaction(&store, publish_tx, 0, 0, &BTreeMap::new())?;

    println!("Package published!");
    println!("  Status: {:?}", publish_result.effects.status());
    println!(
        "  Gas used: {:?}",
        publish_result.effects.gas_cost_summary()
    );

    let package_id = publish_result
        .effects
        .created()
        .iter()
        .find(|(_, owner)| matches!(owner, Owner::Immutable))
        .map(|(obj_ref, _)| obj_ref.0)
        .ok_or_else(|| anyhow::anyhow!("No package object found in created objects"))?;

    println!("  Package ID: {}\n", package_id);

    gas_object_ref = publish_result
        .effects
        .mutated()
        .iter()
        .find(|(obj_ref, _)| obj_ref.0 == gas_object_id)
        .map(|(obj_ref, _)| *obj_ref)
        .ok_or_else(|| anyhow::anyhow!("Gas object not found in mutated objects"))?;

    store.commit_objects(publish_result.inner_temp_store);

    println!("Step 2: Creating a shared counter...");
    let tx_builder =
        TestTransactionBuilder::new(sender, gas_object_ref, DEFAULT_VALIDATOR_GAS_PRICE)
            .call_counter_create(package_id);
    let create_tx = tx_builder.build_and_sign(&sender_keypair);

    let create_result = executor.execute_transaction(&store, create_tx, 0, 0, &BTreeMap::new())?;

    println!("Counter created!");
    println!("  Status: {:?}", create_result.effects.status());
    println!("  Gas used: {:?}", create_result.effects.gas_cost_summary());

    let (counter_id, initial_shared_version) = create_result
        .effects
        .created()
        .iter()
        .find_map(|(obj_ref, owner)| match owner {
            Owner::Shared {
                initial_shared_version,
            } => Some((obj_ref.0, *initial_shared_version)),
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("No shared counter object found in created objects"))?;

    println!("  Counter ID: {}", counter_id);
    println!("  Initial shared version: {}\n", initial_shared_version);

    gas_object_ref = create_result
        .effects
        .mutated()
        .iter()
        .find(|(obj_ref, _)| obj_ref.0 == gas_object_id)
        .map(|(obj_ref, _)| *obj_ref)
        .ok_or_else(|| anyhow::anyhow!("Gas object not found in mutated objects"))?;

    store.commit_objects(create_result.inner_temp_store);

    let mut current_counter_version = initial_shared_version;

    println!("Step 3: Incrementing the counter 3 times...\n");
    for i in 1..=3 {
        println!("Transaction {}: Incrementing counter...", i);

        let mut shared_version_assignments = BTreeMap::new();
        shared_version_assignments.insert(
            (counter_id, initial_shared_version),
            current_counter_version,
        );

        let tx_builder =
            TestTransactionBuilder::new(sender, gas_object_ref, DEFAULT_VALIDATOR_GAS_PRICE)
                .call_counter_increment(package_id, counter_id, initial_shared_version);
        let increment_tx = tx_builder.build_and_sign(&sender_keypair);

        let increment_result = executor.execute_transaction(
            &store,
            increment_tx,
            0,
            0,
            &shared_version_assignments,
        )?;

        println!("  Status: {:?}", increment_result.effects.status());
        println!(
            "  Gas used: {:?}",
            increment_result.effects.gas_cost_summary()
        );

        let new_counter_version = increment_result
            .effects
            .mutated()
            .iter()
            .find(|(obj_ref, _)| obj_ref.0 == counter_id)
            .map(|(obj_ref, _)| obj_ref.1)
            .ok_or_else(|| anyhow::anyhow!("Counter not found in mutated objects"))?;

        println!(
            "  Counter version: {} -> {}",
            current_counter_version, new_counter_version
        );

        current_counter_version = new_counter_version;

        gas_object_ref = increment_result
            .effects
            .mutated()
            .iter()
            .find(|(obj_ref, _)| obj_ref.0 == gas_object_id)
            .map(|(obj_ref, _)| *obj_ref)
            .ok_or_else(|| anyhow::anyhow!("Gas object not found in mutated objects"))?;

        store.commit_objects(increment_result.inner_temp_store);
        println!();
    }

    println!("Final counter state:");
    println!("  Counter ID: {}", counter_id);
    println!("  Initial shared version: {}", initial_shared_version);
    println!("  Current version: {}", current_counter_version);
    println!("  Expected value: 3 (incremented 3 times from 0)");
    println!(
        "\nTotal objects read from store: {}",
        store.get_num_object_reads()
    );

    Ok(())
}
