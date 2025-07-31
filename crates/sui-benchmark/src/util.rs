// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::workloads::Gas;
use crate::ValidatorProxy;
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::ObjectRef;
use sui_types::crypto::{AccountKeyPair, KeypairTraits};
use sui_types::object::Owner;
use sui_types::transaction::{Transaction, TransactionData, TEST_ONLY_GAS_UNIT_FOR_TRANSFER};
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{base_types::SuiAddress, crypto::SuiKeyPair};

// This is the maximum gas we will transfer from primary coin into any gas coin
// for running the benchmark

pub type UpdatedAndNewlyMintedGasCoins = Vec<Gas>;

pub fn get_ed25519_keypair_from_keystore(
    keystore_path: PathBuf,
    requested_address: &SuiAddress,
) -> Result<AccountKeyPair> {
    let keystore = FileBasedKeystore::new(&keystore_path)?;
    match keystore.export(requested_address) {
        Ok(SuiKeyPair::Ed25519(kp)) => Ok(kp.copy()),
        other => Err(anyhow::anyhow!("Invalid key type: {:?}", other)),
    }
}

pub fn make_pay_tx(
    input_coins: Vec<ObjectRef>,
    sender: SuiAddress,
    addresses: Vec<SuiAddress>,
    split_amounts: Vec<u64>,
    gas: ObjectRef,
    keypair: &AccountKeyPair,
    gas_price: u64,
) -> Result<Transaction> {
    let pay = TransactionData::new_pay(
        sender,
        input_coins,
        addresses,
        split_amounts,
        gas,
        TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
        gas_price,
    )?;
    Ok(to_sender_signed_transaction(pay, keypair))
}

pub async fn publish_basics_package(
    gas: ObjectRef,
    proxy: Arc<dyn ValidatorProxy + Sync + Send>,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    gas_price: u64,
) -> ObjectRef {
    let mut current_gas = gas;
    let max_retries = 5;

    for retry in 1..=max_retries {
        let transaction = TestTransactionBuilder::new(sender, current_gas, gas_price)
            .publish_examples("basics")
            .build_and_sign(keypair);
        tracing::info!(
            "Publishing basics package with tx digest {:?} (attempt {})",
            transaction.digest(),
            retry
        );

        let (client_type, execution_result) = proxy.execute_transaction_block(transaction).await;
        tracing::debug!(
            "Executed publish_basics_package transaction via {:?}",
            client_type
        );

        match execution_result {
            Ok(effects) => {
                if let Some(package_ref) = effects
                    .created()
                    .iter()
                    .find(|(_, owner)| matches!(owner, Owner::Immutable))
                    .map(|(reference, _)| *reference)
                {
                    tracing::info!("Successfully published basics package: {:?}", package_ref);
                    return package_ref;
                } else {
                    tracing::error!("Transaction succeeded but no package was created");
                }
            }
            Err(e) => {
                tracing::error!(
                    "Attempt {}: Failed to execute publish_basics_package transaction: {:?}",
                    retry,
                    e
                );
            }
        }

        // Small delay before retrying.
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Get fresh gas object version in case gas object was used.
        match proxy.get_object(current_gas.0).await {
            Ok(gas_obj) => {
                current_gas = gas_obj.compute_object_reference();
                tracing::info!(
                    "Retry {}: Using updated gas object version {:?}",
                    retry,
                    current_gas
                );
            }
            Err(e) => {
                tracing::error!("Retry {}: Failed to get updated gas object: {:?}", retry, e);
                // Continue with existing gas reference, hoping it works
            }
        }
    }

    panic!(
        "publish_basics_package failed to create package after {} attempts",
        max_retries
    );
}
