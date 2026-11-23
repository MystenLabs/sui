// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end test of encrypted bundles on a 4-validator cluster. The enclave library builds a
//! bundle, validator 0 submits it, every validator derives the key from its beacon shares, and
//! the decrypted transactions execute.

use std::time::Duration;

use fastcrypto::serde_helpers::ToFromByteArray;
use mysten_common::ZipDebugEqIteratorExt;
use sui_encrypted_bundle_enclave::{
    AddTransactionPayload, EnclaveState, MockAttestor, encrypt_request,
};
use sui_macros::sim_test;
use sui_node::SuiNodeHandle;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::encrypted_bundle::{BeaconKeyInfo, EncryptedBundleSubmission};
use sui_types::transaction::{CallArg, Transaction};
use test_cluster::addr_balance_test_env::{TestEnv, TestEnvBuilder};

const TIMEOUT: Duration = Duration::from_secs(120);

async fn build_env() -> TestEnv {
    TestEnvBuilder::new()
        .with_num_validators(4)
        .with_proto_override_cb(Box::new(|_, mut config| {
            config.set_encrypted_bundles_for_testing(true);
            config.set_encrypted_bundle_max_per_commit_for_testing(4);
            config.set_encrypted_bundle_max_transactions_for_testing(16);
            config.set_encrypted_bundle_fee_for_testing(1_000);
            config
        }))
        .build()
        .await
}

/// Waits until the validator's beacon DKG completes and returns the key the enclave needs.
async fn beacon_key_info(validator: &SuiNodeHandle) -> BeaconKeyInfo {
    tokio::time::timeout(TIMEOUT, async {
        loop {
            let info = validator.with(|node| {
                let state = node.state();
                let epoch_store = state.load_epoch_store_one_call_per_task();
                let keys = epoch_store.encrypted_bundle_keys()?;
                let public_key = keys.beacon_public_key(epoch_store.epoch())?;
                Some(BeaconKeyInfo {
                    epoch: epoch_store.epoch(),
                    authority: state.name,
                    beacon_public_key: public_key.to_byte_array().to_vec(),
                })
            });
            if let Some(info) = info {
                return info;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await
    .expect("beacon DKG should complete")
}

/// A transaction that reads the shared clock and pays gas from the sender's address balance.
async fn bundle_transaction(env: &TestEnv, epoch: u64, nonce: u32) -> Transaction {
    let sender = env.get_sender(0);
    let data = TestTransactionBuilder::new_with_address_balance_gas(
        sender,
        env.rgp,
        env.chain_id,
        epoch,
        nonce,
    )
    .move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        "clock",
        "timestamp_ms",
        vec![CallArg::CLOCK_IMM],
    )
    .build();
    env.cluster.wallet.sign_transaction(&data).await
}

#[sim_test]
async fn test_encrypted_bundle_end_to_end() {
    telemetry_subscribers::init_for_testing();
    let mut env = build_env().await;
    let sender = env.get_sender(0);
    env.fund_one_address_balance(sender, 10_000_000_000).await;

    let validators = env.cluster.all_validator_handles();
    let beacon = beacon_key_info(&validators[0]).await;

    let enclave = EnclaveState::new(&mut rand::thread_rng(), Box::new(MockAttestor));
    enclave.update_key(beacon.clone()).unwrap();
    let transactions = vec![
        bundle_transaction(&env, beacon.epoch, 1).await,
        bundle_transaction(&env, beacon.epoch, 2).await,
    ];
    for transaction in &transactions {
        let request = encrypt_request(
            &mut rand::thread_rng(),
            &enclave.request_public_key(),
            &AddTransactionPayload {
                transaction: transaction.clone(),
                expected_epoch: beacon.epoch,
                expected_beacon_public_key: beacon.beacon_public_key.clone(),
            },
        );
        enclave.add_transaction(&request).unwrap();
    }
    let bundle = enclave
        .next_block(&mut rand::thread_rng())
        .unwrap()
        .expect("bundle has transactions");

    // The validator pays the bundle fee with a plain transaction from another account.
    let fee_payer = env.get_sender(1);
    let fee_data = env
        .tx_builder(fee_payer)
        .transfer_sui(Some(1), fee_payer)
        .build();
    let fee_tx = env.cluster.wallet.sign_transaction(&fee_data).await;

    let submission = EncryptedBundleSubmission { bundle, fee_tx };
    let (_, fee_digest) = validators[0]
        .with_async(|node| async move { node.submit_encrypted_bundle(submission).await })
        .await
        .expect("submission should succeed");

    let mut digests: Vec<_> = transactions.iter().map(|tx| *tx.digest()).collect();
    digests.push(fee_digest);
    tokio::time::timeout(TIMEOUT, env.cluster.wait_for_tx_settlement(&digests))
        .await
        .expect("bundle transactions should execute");

    for validator in &validators {
        let effects = validator.with(|node| {
            node.state()
                .get_transaction_cache_reader()
                .multi_get_executed_effects(&digests)
        });
        for (digest, effects) in digests.iter().zip_debug_eq(effects) {
            let effects = effects.unwrap_or_else(|| panic!("{digest} did not execute"));
            assert!(
                effects.status().is_ok(),
                "{digest} failed: {:?}",
                effects.status()
            );
        }
    }

    // Reconfiguration succeeds after bundles were processed.
    env.trigger_reconfiguration().await;
}
