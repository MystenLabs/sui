// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end test of encrypted transactions on a 4-validator cluster. A user encrypts a
//! programmable transaction to the beacon key and submits it like any transaction. Every
//! validator derives the key from its beacon shares after commit, and the fullnode decrypts it
//! from the key in the commit prologue.

use std::path::PathBuf;
use std::time::Duration;

use move_core_types::language_storage::TypeTag;
use sui_macros::sim_test;
use sui_node::SuiNodeHandle;
use sui_types::base_types::ObjectID;
use sui_types::base_types::{SequenceNumber, SuiAddress};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::encrypted_transaction::{BeaconPublicKey, EncryptedTransaction};
use sui_types::gas_coin::GAS;
use sui_types::message_envelope::Message;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{
    CallArg, FundsWithdrawalArg, ObjectArg, ProgrammableTransaction, SharedObjectMutability,
    TransactionData, TransactionKind,
};
use sui_types::{SUI_CLOCK_OBJECT_ID, SUI_DENY_LIST_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID};
use test_cluster::addr_balance_test_env::{TestEnv, TestEnvBuilder};

const TIMEOUT: Duration = Duration::from_secs(120);
const MIN_GAS_PRICE: u64 = 1_000;

async fn build_env() -> TestEnv {
    TestEnvBuilder::new()
        .with_num_validators(4)
        .with_proto_override_cb(Box::new(|_, mut config| {
            config.set_encrypted_transactions_for_testing(true);
            config.set_encrypted_transaction_max_per_commit_for_testing(4);
            config.set_encrypted_transaction_min_gas_price_for_testing(MIN_GAS_PRICE);
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
                Some((epoch_store.epoch(), node.beacon_public_key()?))
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

/// Sends `amount` from the sender's address balance to the recipient's address balance.
fn send_from_address_balance(
    coin_type: TypeTag,
    amount: u64,
    recipient: SuiAddress,
) -> ProgrammableTransaction {
    let mut builder = ProgrammableTransactionBuilder::new();
    let withdrawal = builder
        .funds_withdrawal(FundsWithdrawalArg::balance_from_sender(
            amount,
            coin_type.clone(),
        ))
        .unwrap();
    let balance = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        "balance".parse().unwrap(),
        "redeem_funds".parse().unwrap(),
        vec![coin_type.clone()],
        vec![withdrawal],
    );
    let recipient = builder.pure(recipient).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        "balance".parse().unwrap(),
        "send_funds".parse().unwrap(),
        vec![coin_type],
        vec![balance, recipient],
    );
    builder.finish()
}

/// Encrypts and executes `transaction`, and waits until it settles on all nodes.
async fn execute_encrypted(
    env: &mut TestEnv,
    sender: SuiAddress,
    transaction: &ProgrammableTransaction,
) -> TransactionEffects {
    let (epoch, beacon_pk) = beacon_key(&env.cluster.all_validator_handles()[0]).await;
    let encrypted = EncryptedTransaction::seal(
        &mut rand::thread_rng(),
        &beacon_pk,
        epoch,
        sender,
        transaction,
    );
    let data = envelope(env, sender, encrypted);
    let (digest, effects) = env
        .exec_tx_directly(data)
        .await
        .expect("encrypted transaction should execute");
    tokio::time::timeout(
        TIMEOUT,
        env.cluster.wait_for_tx_settlement_all_nodes(&[digest]),
    )
    .await
    .expect("all nodes should execute the transaction");
    effects
}

fn envelope(env: &TestEnv, sender: SuiAddress, encrypted: EncryptedTransaction) -> TransactionData {
    let gas = env.get_gas_for_sender(sender);
    TransactionData::new_with_gas_coins(
        TransactionKind::EncryptedTransaction(encrypted),
        sender,
        gas,
        100_000_000,
        env.rgp.max(MIN_GAS_PRICE),
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

#[sim_test]
async fn test_encrypted_transaction_withdraws_from_address_balance() {
    telemetry_subscribers::init_for_testing();
    let mut env = build_env().await;
    let sender = env.get_sender(0);
    let recipient = SuiAddress::random_for_testing_only();
    env.fund_one_address_balance(sender, 1_000).await;

    let transfer = send_from_address_balance(GAS::type_tag(), 400, recipient);
    let effects = execute_encrypted(&mut env, sender, &transfer).await;
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    assert_eq!(env.get_sui_balance_ab(sender), 600);
    assert_eq!(env.get_sui_balance_ab(recipient), 400);

    // A transaction that withdraws more than the balance runs no command and pays gas.
    let transfer = send_from_address_balance(GAS::type_tag(), 601, recipient);
    let effects = execute_encrypted(&mut env, sender, &transfer).await;
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    assert!(effects.gas_cost_summary().computation_cost > 0);
    assert_eq!(env.get_sui_balance_ab(sender), 600);
    assert_eq!(env.get_sui_balance_ab(recipient), 400);

    // The whole balance can be withdrawn.
    let transfer = send_from_address_balance(GAS::type_tag(), 600, recipient);
    let effects = execute_encrypted(&mut env, sender, &transfer).await;
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    assert_eq!(env.get_sui_balance_ab(recipient), 1_000);

    env.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_encrypted_withdrawal_respects_the_deny_list() {
    telemetry_subscribers::init_for_testing();
    let mut env = build_env().await;
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code");

    let publisher = env.get_sender(0);
    let publish = env.tx_builder(publisher).publish_async(path).await.build();
    let (_, effects) = env.exec_tx_directly(publish).await.unwrap();
    let mut coin: Option<(ObjectID, TypeTag)> = None;
    let mut deny_cap = None;
    for (reference, _) in effects.created() {
        let object = env
            .cluster
            .get_object_from_fullnode_store(&reference.0)
            .await
            .unwrap();
        if object.is_coin() {
            coin = Some((reference.0, object.coin_type_maybe().unwrap()));
        } else if object
            .type_()
            .is_some_and(|type_| type_.is_coin_deny_cap_v2())
        {
            deny_cap = Some(reference);
        }
    }
    let (coin_id, coin_type) = coin.unwrap();
    let coin_ref = env
        .cluster
        .get_object_from_fullnode_store(&coin_id)
        .await
        .unwrap()
        .compute_object_reference();

    // Move the coin to the publisher's address balance.
    let deposit = env
        .tx_builder(publisher)
        .move_call_with_type_args(
            SUI_FRAMEWORK_PACKAGE_ID,
            "coin",
            "send_funds",
            vec![coin_type.clone()],
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_ref)),
                CallArg::Pure(bcs::to_bytes(&publisher).unwrap()),
            ],
        )
        .build();
    let (digest, effects) = env.exec_tx_directly(deposit).await.unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    env.cluster.wait_for_tx_settlement(&[digest]).await;

    let recipient = SuiAddress::random_for_testing_only();
    let transfer = send_from_address_balance(coin_type.clone(), 1, recipient);
    let effects = execute_encrypted(&mut env, publisher, &transfer).await;
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    assert_eq!(env.get_balance_ab(recipient, coin_type.clone()), 1);

    let deny_list_version = env
        .cluster
        .get_object_from_fullnode_store(&SUI_DENY_LIST_OBJECT_ID)
        .await
        .unwrap()
        .owner()
        .start_version()
        .unwrap();
    let deny = env
        .tx_builder(publisher)
        .move_call_with_type_args(
            SUI_FRAMEWORK_PACKAGE_ID,
            "coin",
            "deny_list_v2_add",
            vec![coin_type.clone()],
            vec![
                CallArg::Object(ObjectArg::SharedObject {
                    id: SUI_DENY_LIST_OBJECT_ID,
                    initial_shared_version: deny_list_version,
                    mutability: SharedObjectMutability::Mutable,
                }),
                CallArg::Object(ObjectArg::ImmOrOwnedObject(deny_cap.unwrap())),
                CallArg::Pure(bcs::to_bytes(&publisher).unwrap()),
            ],
        )
        .build();
    let (_, effects) = env.exec_tx_directly(deny).await.unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());

    // The denial applies in the same epoch, as it does to plaintext transactions at the vote.
    let effects = execute_encrypted(&mut env, publisher, &transfer).await;
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    assert!(effects.gas_cost_summary().computation_cost > 0);
    assert_eq!(env.get_balance_ab(recipient, coin_type), 1);
}

#[sim_test]
async fn test_hidden_inputs_that_the_vote_would_reject() {
    telemetry_subscribers::init_for_testing();
    let mut env = build_env().await;
    let sender = env.get_sender(0);
    let owned = env.get_gas_for_sender(env.get_sender(1))[0];

    let use_shared = |id, version, mutability| {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder
            .obj(ObjectArg::SharedObject {
                id,
                initial_shared_version: version,
                mutability,
            })
            .unwrap();
        builder.transfer_sui(SuiAddress::ZERO, Some(1));
        builder.finish()
    };
    let call_object = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.programmable_move_call(
            SUI_CLOCK_OBJECT_ID,
            "clock".parse().unwrap(),
            "timestamp_ms".parse().unwrap(),
            vec![],
            vec![],
        );
        builder.finish()
    };
    let invalid = [
        use_shared(
            SUI_CLOCK_OBJECT_ID,
            SequenceNumber::from_u64(1),
            SharedObjectMutability::Mutable,
        ),
        use_shared(
            SUI_CLOCK_OBJECT_ID,
            SequenceNumber::from_u64(7),
            SharedObjectMutability::Immutable,
        ),
        use_shared(owned.0, owned.1, SharedObjectMutability::Immutable),
        call_object,
    ];
    for transaction in &invalid {
        let effects = execute_encrypted(&mut env, sender, transaction).await;
        assert!(effects.status().is_ok(), "{:?}", effects.status());
        assert!(effects.created().is_empty());
        assert!(effects.gas_cost_summary().computation_cost > 0);
    }

    // The network still executes encrypted transactions.
    let effects = execute_encrypted(&mut env, sender, &read_clock()).await;
    assert!(reads_clock(&effects));
    env.trigger_reconfiguration().await;
}

/// Executes an encrypted transaction that reads the clock. The transaction driver retries when it
/// picks a validator that is stopped or behind.
async fn execute_through_driver(
    env: &mut TestEnv,
    epoch: u64,
    beacon_pk: &BeaconPublicKey,
    sender: SuiAddress,
) {
    let encrypted = EncryptedTransaction::seal(
        &mut rand::thread_rng(),
        beacon_pk,
        epoch,
        sender,
        &read_clock(),
    );
    let transaction = env
        .cluster
        .sign_transaction(&envelope(env, sender, encrypted))
        .await;
    let (effects, _) = env
        .cluster
        .execute_transaction_return_raw_effects(transaction)
        .await
        .expect("encrypted transaction should execute");
    assert!(reads_clock(&effects));
    env.update_all_gas().await;
}

#[sim_test]
async fn test_restarted_validators_resolve_encrypted_transactions() {
    telemetry_subscribers::init_for_testing();
    let mut env = build_env().await;
    let sender = env.get_sender(0);
    execute_encrypted(&mut env, sender, &read_clock()).await;

    // One validator misses encrypted transactions. Then all validators restart, so the others
    // lose the keys that they derived.
    let (epoch, beacon_pk) = beacon_key(&env.cluster.all_validator_handles()[0]).await;
    let lagging = env.cluster.get_validator_pubkeys()[3];
    env.cluster.stop_node(&lagging);
    for _ in 0..3 {
        execute_through_driver(&mut env, epoch, &beacon_pk, sender).await;
    }
    tokio::time::sleep(Duration::from_secs(10)).await;
    env.cluster.stop_all_validators().await;
    env.cluster.start_all_validators().await;
    execute_through_driver(&mut env, epoch, &beacon_pk, sender).await;

    // Every consensus handler catches up and builds new checkpoints.
    let target = env.cluster.fullnode_handle.sui_node.with(|node| {
        node.state()
            .get_checkpoint_store()
            .get_highest_executed_checkpoint_seq_number()
            .unwrap()
            .unwrap()
    });
    tokio::time::timeout(TIMEOUT, async {
        for validator in env.cluster.all_validator_handles() {
            while validator.with(|node| {
                node.state()
                    .get_checkpoint_store()
                    .get_latest_locally_computed_checkpoint()
                    .unwrap()
                    .is_none_or(|checkpoint| checkpoint.sequence_number < target)
            }) {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    })
    .await
    .expect("every validator should build checkpoints");
    env.trigger_reconfiguration().await;
}
