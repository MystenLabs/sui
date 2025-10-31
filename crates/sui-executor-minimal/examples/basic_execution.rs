// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use sui_executor_minimal::{InMemoryObjectStore, MinimalExecutor};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{random_object_ref, SuiAddress};
use sui_types::crypto::{get_key_pair, AccountKeyPair};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Object;
use sui_types::transaction::DEFAULT_VALIDATOR_GAS_PRICE;

fn main() -> Result<()> {
    println!("=== Sui Minimal Executor Example ===\n");

    let sender = SuiAddress::random_for_testing_only();
    let (_, sender_keypair): (_, AccountKeyPair) = get_key_pair();

    let gas_object_id = random_object_ref().0;
    let gas_object = Object::with_id_owner_gas_for_testing(gas_object_id, sender, 1_000_000_000);
    let gas_object_ref = gas_object.compute_object_reference();

    println!("Created sender: {}", sender);
    println!("Created gas object: {:?}\n", gas_object_ref);

    let store = InMemoryObjectStore::new_with_genesis_packages(
        vec![(gas_object.id(), gas_object)].into_iter().collect()
    );
    println!("Created store with framework packages loaded\n");

    println!("Initializing executor...");
    let executor = MinimalExecutor::new_for_testing()?;
    println!("Executor initialized with protocol version: {:?}\n", executor.protocol_config().version);

    let recipient = SuiAddress::random_for_testing_only();
    let amount = 1000;

    println!("Building transfer transaction:");
    println!("  From: {}", sender);
    println!("  To: {}", recipient);
    println!("  Amount: {}\n", amount);

    let tx_builder = TestTransactionBuilder::new(sender, gas_object_ref, DEFAULT_VALIDATOR_GAS_PRICE)
        .transfer_sui(Some(amount), recipient);
    let transaction = tx_builder.build_and_sign(&sender_keypair);

    println!("Executing transaction...");
    let result = executor.execute_transaction(
        &store,
        transaction,
        0,
        0,
    )?;

    println!("\nExecution completed!");
    println!("Transaction digest: {:?}", result.effects.transaction_digest());
    println!("Status: {:?}", result.effects.status());
    println!("Gas used: {:?}", result.effects.gas_cost_summary());
    println!("Modified objects: {}", result.effects.modified_at_versions().len());
    println!("Created objects: {}", result.effects.created().len());
    println!("\nObjects read from store: {}", store.get_num_object_reads());

    store.commit_objects(result.inner_temp_store);
    println!("\nCommitted transaction results to store");

    Ok(())
}
