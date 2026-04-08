// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests for the dynamic RPC validation system.
//!
//! These tests verify that validators can load validation libraries and apply
//! custom validation rules to incoming transactions.

use std::path::PathBuf;
use std::sync::Arc;
use sui_config::dynamic_rpc_validator_config::ValidatorInfo;
use sui_macros::sim_test;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{AccountKeyPair, get_key_pair};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::transaction::Transaction;
use test_cluster::TestClusterBuilder;

/// Helper to get the path to the reject_zero_sender shared library.
/// Returns None if the library hasn't been built.
fn get_reject_zero_sender_library_path() -> Option<PathBuf> {
    // Try to find the library in the build directory
    // The library is built with: cargo build --example reject_zero_sender --release --features parsing
    let possible_paths = [
        // Release build
        "target/release/examples/libreject_zero_sender.dylib",
        "target/release/examples/libreject_zero_sender.so",
        "target/release/examples/reject_zero_sender.dll",
        // Debug build
        "target/debug/examples/libreject_zero_sender.dylib",
        "target/debug/examples/libreject_zero_sender.so",
        "target/debug/examples/reject_zero_sender.dll",
    ];

    for path in &possible_paths {
        let full_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(path);
        if full_path.exists() {
            return Some(full_path);
        }
    }
    None
}

/// Helper to check if an address ends in zero (last byte is 0x00).
fn address_ends_in_zero(address: &SuiAddress) -> bool {
    let bytes = address.to_inner();
    bytes[31] == 0x00
}

/// Helper to generate an address that ends in zero.
fn generate_address_ending_in_zero() -> (SuiAddress, AccountKeyPair) {
    loop {
        let (address, keypair): (SuiAddress, AccountKeyPair) = get_key_pair();
        if address_ends_in_zero(&address) {
            return (address, keypair);
        }
    }
}

/// Helper to generate an address that does NOT end in zero.
fn generate_address_not_ending_in_zero() -> (SuiAddress, AccountKeyPair) {
    loop {
        let (address, keypair): (SuiAddress, AccountKeyPair) = get_key_pair();
        if !address_ends_in_zero(&address) {
            return (address, keypair);
        }
    }
}

/// Test that transactions succeed when no validators are using a validation library.
#[sim_test]
async fn test_no_validator_library() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(4)
        .build()
        .await;

    // Execute a simple transaction - should succeed
    let tx = test_cluster
        .test_transaction_builder()
        .await
        .transfer_sui(None, SuiAddress::ZERO)
        .build();

    let effects = test_cluster.sign_and_execute_transaction(&tx).await.effects;
    assert!(effects.status().is_ok(), "Transaction should succeed");
}

/// Test that transactions from senders NOT ending in zero succeed when all validators
/// are using the reject_zero_sender library.
#[sim_test]
async fn test_all_validators_with_library_accepts_normal_sender() {
    let library_path = match get_reject_zero_sender_library_path() {
        Some(path) => path,
        None => {
            println!("Skipping test: reject_zero_sender library not found");
            println!(
                "Build it with: cargo build --example reject_zero_sender --release -p sui-dynamic-rpc-validator --features parsing"
            );
            return;
        }
    };

    // Set up the callback to provide the library to all validators
    let library_path_clone = library_path.clone();
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(4)
        .with_validator_library_callback(Arc::new(move |_info: &ValidatorInfo| {
            Some(library_path_clone.clone())
        }))
        .build()
        .await;

    // Generate a sender that does NOT end in zero
    let (sender, keypair) = generate_address_not_ending_in_zero();
    println!("Using sender address: {:?}", sender);
    assert!(
        !address_ends_in_zero(&sender),
        "Sender should NOT end in zero"
    );

    // Fund the sender address
    let rgp = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(10_000_000_000), sender)
        .await;

    // Build and sign the transaction with the custom keypair
    let tx_data = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui(Some(1_000_000), SuiAddress::ZERO)
        .build();
    let tx = Transaction::from_data_and_signer(tx_data, vec![&keypair]);

    // Execute the transaction - should succeed because sender doesn't end in zero
    let result = test_cluster.wallet.execute_transaction_may_fail(tx).await;

    assert!(
        result.is_ok(),
        "Transaction from normal sender should succeed, got error: {:?}",
        result.err()
    );
    let response = result.unwrap();
    assert!(
        response.effects.status().is_ok(),
        "Transaction effects should indicate success"
    );
    println!(
        "Transaction succeeded as expected with library path: {:?}",
        library_path
    );
}

/// Test that transactions from senders ending in zero are rejected when all validators
/// are using the reject_zero_sender library.
#[sim_test]
async fn test_all_validators_with_library_rejects_zero_sender() {
    let library_path = match get_reject_zero_sender_library_path() {
        Some(path) => path,
        None => {
            println!("Skipping test: reject_zero_sender library not found");
            println!(
                "Build it with: cargo build --example reject_zero_sender --release -p sui-dynamic-rpc-validator --features parsing"
            );
            return;
        }
    };

    // Set up the callback to provide the library to all validators
    let library_path_clone = library_path.clone();
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(4)
        .with_validator_library_callback(Arc::new(move |_info: &ValidatorInfo| {
            Some(library_path_clone.clone())
        }))
        .build()
        .await;

    // Generate a sender that ends in zero
    let (sender, keypair) = generate_address_ending_in_zero();
    println!("Generated sender ending in zero: {:?}", sender);
    assert!(address_ends_in_zero(&sender), "Sender should end in zero");

    // Fund the sender address
    let rgp = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(10_000_000_000), sender)
        .await;

    // Build and sign the transaction with the custom keypair
    let tx_data = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui(Some(1_000_000), SuiAddress::ZERO)
        .build();
    let tx = Transaction::from_data_and_signer(tx_data, vec![&keypair]);

    // Execute the transaction - should fail because all validators reject zero-ending senders
    let result = test_cluster.wallet.execute_transaction_may_fail(tx).await;

    assert!(
        result.is_err(),
        "Transaction from zero-ending sender should be rejected by all validators"
    );
    let error_msg = result.unwrap_err().to_string();
    println!("Transaction rejected as expected with error: {}", error_msg);
    // The error should indicate the transaction was rejected by the dynamic validator
    assert!(
        error_msg.contains("rejected")
            || error_msg.contains("PermissionDenied")
            || error_msg.contains("failed"),
        "Error message should indicate rejection, got: {}",
        error_msg
    );
}

/// Test that transactions can still succeed when only one validator is using
/// the reject_zero_sender library (the other validators will accept the transaction).
#[sim_test]
async fn test_single_validator_with_library() {
    let library_path = match get_reject_zero_sender_library_path() {
        Some(path) => path,
        None => {
            println!("Skipping test: reject_zero_sender library not found");
            println!(
                "Build it with: cargo build --example reject_zero_sender --release -p sui-dynamic-rpc-validator --features parsing"
            );
            return;
        }
    };

    // Set up the callback to provide the library only to validator 0
    let library_path_clone = library_path.clone();
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(4)
        .with_validator_library_callback(Arc::new(move |info: &ValidatorInfo| {
            if info.index == 0 {
                Some(library_path_clone.clone())
            } else {
                None
            }
        }))
        .build()
        .await;

    // Generate a sender that ends in zero
    let (sender, keypair) = generate_address_ending_in_zero();
    println!("Generated sender ending in zero: {:?}", sender);
    assert!(address_ends_in_zero(&sender), "Sender should end in zero");

    // Fund the sender address
    let rgp = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(10_000_000_000), sender)
        .await;

    // Build and sign the transaction with the custom keypair
    let tx_data = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui(Some(1_000_000), SuiAddress::ZERO)
        .build();
    let tx = Transaction::from_data_and_signer(tx_data, vec![&keypair]);

    // Execute the transaction - should succeed because only 1 of 4 validators rejects,
    // and the other 3 form a quorum to accept the transaction
    let result = test_cluster.wallet.execute_transaction_may_fail(tx).await;

    assert!(
        result.is_ok(),
        "Transaction should succeed with quorum (3/4 validators accepting), got error: {:?}",
        result.err()
    );
    let response = result.unwrap();
    assert!(
        response.effects.status().is_ok(),
        "Transaction effects should indicate success"
    );
    println!(
        "Transaction succeeded as expected - only validator 0 had the library: {:?}",
        library_path
    );
}
