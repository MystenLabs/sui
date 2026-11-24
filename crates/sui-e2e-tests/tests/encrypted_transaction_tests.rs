// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end test of encrypted transactions on a 4-validator cluster. A user encrypts a
//! programmable transaction to the beacon key and submits it like any transaction. Every
//! validator derives the key from its beacon shares after commit, and the fullnode decrypts it
//! from the key in the commit prologue.

use std::time::Duration;

use sui_macros::sim_test;
use sui_node::SuiNodeHandle;
use sui_types::SUI_CLOCK_OBJECT_ID;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::base_types::SuiAddress;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::encrypted_transaction::{BeaconPublicKey, EncryptedTransaction};
use sui_types::message_envelope::Message;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{CallArg, ProgrammableTransaction, TransactionData, TransactionKind};
use test_cluster::addr_balance_test_env::{TestEnv, TestEnvBuilder};

const TIMEOUT: Duration = Duration::from_secs(120);
const FEE: u64 = 1_000;

async fn build_env() -> TestEnv {
    TestEnvBuilder::new()
        .with_num_validators(4)
        .with_proto_override_cb(Box::new(|_, mut config| {
            config.set_encrypted_transactions_for_testing(true);
            config.set_encrypted_transaction_max_per_commit_for_testing(4);
            config.set_encrypted_transaction_fee_for_testing(FEE);
            config
        }))
        .build()
        .await
}

/// Waits until the validator's beacon DKG completes and returns the beacon key.
async fn beacon_key(validator: &SuiNodeHandle) -> (u64, BeaconPublicKey) {
    tokio::time::timeout(TIMEOUT, async {
        loop {
            let key = validator.with(|node| {
                let epoch_store = node.state().load_epoch_store_one_call_per_task();
                let keys = epoch_store.encrypted_transaction_keys()?;
                Some((
                    epoch_store.epoch(),
                    keys.beacon_public_key(epoch_store.epoch())?,
                ))
            });
            if let Some(key) = key {
                return key;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await
    .expect("beacon DKG should complete")
}

/// Reads the shared clock.
fn read_clock() -> ProgrammableTransaction {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            "clock".parse().unwrap(),
            "timestamp_ms".parse().unwrap(),
            vec![],
            vec![CallArg::CLOCK_IMM],
        )
        .unwrap();
    builder.finish()
}

fn envelope(env: &TestEnv, sender: SuiAddress, encrypted: EncryptedTransaction) -> TransactionData {
    let gas = env.get_gas_for_sender(sender);
    TransactionData::new_with_gas_coins(
        TransactionKind::EncryptedTransaction(encrypted),
        sender,
        gas,
        100_000_000,
        env.rgp
            .max(FEE / sui_types::encrypted_transaction::MIN_COMPUTATION_UNITS),
    )
}

fn reads_clock(effects: &TransactionEffects) -> bool {
    effects
        .accessed_consensus_objects()
        .iter()
        .any(|input| input.id_and_version().0 == SUI_CLOCK_OBJECT_ID)
}

#[sim_test]
async fn test_encrypted_transaction_end_to_end() {
    telemetry_subscribers::init_for_testing();
    let mut env = build_env().await;
    let validators = env.cluster.all_validator_handles();
    let (epoch, beacon_pk) = beacon_key(&validators[0]).await;

    let sender = env.get_sender(0);
    let encrypted = EncryptedTransaction::seal(
        &mut rand::thread_rng(),
        &beacon_pk,
        epoch,
        sender,
        &read_clock(),
    );

    // Another account copies the ciphertext into its own transaction. Its key belongs to a
    // different identity, so the transaction runs no command and only pays gas.
    let copier = env.get_sender(1);
    let (copied_digest, copied_effects) = env
        .exec_tx_directly(envelope(&env, copier, encrypted.clone()))
        .await
        .expect("copied transaction should be sequenced");
    assert!(copied_effects.status().is_ok());
    assert!(
        !reads_clock(&copied_effects),
        "the copied ciphertext must not decrypt"
    );
    assert!(copied_effects.gas_cost_summary().computation_cost > 0);

    let (digest, effects) = env
        .exec_tx_directly(envelope(&env, sender, encrypted))
        .await
        .expect("encrypted transaction should execute");
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    assert!(
        reads_clock(&effects),
        "the decrypted transaction reads the clock"
    );

    // Every validator and the fullnode produce the same effects. The fullnode decrypts the
    // transaction with the key from the commit prologue.
    tokio::time::timeout(
        TIMEOUT,
        env.cluster
            .wait_for_tx_settlement_all_nodes(&[digest, copied_digest]),
    )
    .await
    .expect("all nodes should execute the transactions");
    for node in validators
        .iter()
        .chain(std::iter::once(&env.cluster.fullnode_handle.sui_node))
    {
        let node_effects = node.with(|node| {
            node.state()
                .get_transaction_cache_reader()
                .get_executed_effects(&digest)
        });
        assert_eq!(
            node_effects
                .expect("node executed the transaction")
                .digest(),
            effects.digest()
        );
    }

    // Reconfiguration succeeds after encrypted transactions were processed.
    env.trigger_reconfiguration().await;
}
