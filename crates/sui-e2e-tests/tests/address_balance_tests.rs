// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{identifier::Identifier, u256::U256};
use rand::{Rng, seq::SliceRandom};
use shared_crypto::intent::Intent;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use sui_core::accumulators::balances::get_all_balances_for_owner;
use sui_keys::keystore::AccountKeystore;
use sui_macros::*;
use sui_protocol_config::{ProtocolConfig, ProtocolVersion};
use sui_test_transaction_builder::FundSource;
use sui_types::{
    SUI_ACCUMULATOR_ROOT_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID, TypeTag,
    accumulator_root::AccumulatorValue,
    balance::Balance,
    base_types::{ObjectID, ObjectRef, SuiAddress, dbg_addr},
    digests::{ChainIdentifier, CheckpointDigest},
    effects::{InputConsensusObject, TransactionEffectsAPI},
    gas::GasCostSummary,
    gas_coin::GAS,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    supported_protocol_versions::SupportedProtocolVersions,
    transaction::{
        Argument, CallArg, Command, FundsWithdrawalArg, GasData, ObjectArg, Transaction,
        TransactionData, TransactionDataAPI, TransactionDataV1, TransactionExpiration,
        TransactionKind, VerifiedTransaction,
    },
};
use test_cluster::{
    TestClusterBuilder,
    addr_balance_test_env::{TestEnv, TestEnvBuilder},
};

fn create_transaction_with_expiration(
    sender: SuiAddress,
    gas_coin: ObjectRef,
    rgp: u64,
    min_epoch: Option<u64>,
    max_epoch: Option<u64>,
    chain_id: ChainIdentifier,
    nonce: u32,
) -> TransactionData {
    let mut builder = ProgrammableTransactionBuilder::new();
    let amount = builder.pure(1000u64).unwrap();
    let coin = builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount]));
    let Argument::Result(coin_idx) = coin else {
        panic!("coin is not a result");
    };
    let coin = Argument::NestedResult(coin_idx, 0);
    builder.transfer_arg(sender, coin);
    let tx = TransactionKind::ProgrammableTransaction(builder.finish());
    TransactionData::V1(TransactionDataV1 {
        kind: tx,
        sender,
        gas_data: GasData {
            payment: vec![gas_coin],
            owner: sender,
            price: rgp,
            budget: 10000000,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch,
            max_epoch,
            min_timestamp: None,
            max_timestamp: None,
            chain: chain_id,
            nonce,
        },
    })
}

// Test protocol gating of accumulator root creation. This test can be deleted after the feature
// is released.
#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_accumulators_root_created() {
    let test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|version, mut cfg| {
            if version == ProtocolVersion::MAX - 1 {
                cfg.disable_accumulators_for_testing();
                cfg.disable_create_root_accumulator_object_for_testing();
            } else if version == ProtocolVersion::MAX {
                // accumulators are enabled for devnet/tests, so we need to disable them to run
                // this test
                cfg.disable_accumulators_for_testing();
                cfg.create_root_accumulator_object_for_testing();
                // for some reason all 4 nodes are not reliably submitting capability messages
                cfg.set_buffer_stake_for_protocol_upgrade_bps_for_testing(0);
            } else if version == ProtocolVersion::MAX_ALLOWED {
                cfg.enable_accumulators_for_testing();
            }
            cfg
        }))
        .with_test_cluster_builder_cb(Box::new(|builder| {
            builder
                .with_num_validators(1)
                // We start at MAX - 1 to be sure the root object isn't created in genesis, and then
                // immediately reconfigure.
                .with_protocol_version(ProtocolVersion::MAX - 1)
                .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
                    ProtocolVersion::MAX.as_u64() - 1,
                    ProtocolVersion::MAX_ALLOWED.as_u64(),
                ))
        }))
        .build()
        .await;

    test_env.trigger_reconfiguration().await;

    // accumulator root is not created yet.
    test_env.cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        assert!(
            !state
                .load_epoch_store_one_call_per_task()
                .accumulator_root_exists()
        );
    });

    test_env.trigger_reconfiguration().await;

    // accumulator root was created at the end of previous epoch,
    // but we didn't upgrade to the next protocol version yet.
    test_env.cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        assert!(
            state
                .load_epoch_store_one_call_per_task()
                .accumulator_root_exists()
        );
        assert_eq!(
            state
                .load_epoch_store_one_call_per_task()
                .protocol_config()
                .version,
            ProtocolVersion::MAX
        );
    });

    // now we can upgrade to the next protocol version.
    test_env.trigger_reconfiguration().await;

    test_env.cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        assert_eq!(
            state
                .load_epoch_store_one_call_per_task()
                .protocol_config()
                .version,
            ProtocolVersion::MAX_ALLOWED
        );
    });
}

// Test protocol gating of address balances. This test can be deleted after the feature
// is released.
#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_accumulators_disabled() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|version, mut cfg| {
            if version == ProtocolVersion::MAX - 1 {
                cfg.disable_accumulators_for_testing();
                cfg.disable_create_root_accumulator_object_for_testing();
            } else if version == ProtocolVersion::MAX {
                // accumulators are enabled for devnet/tests, so we need to disable them to run
                // this test
                cfg.disable_accumulators_for_testing();
                cfg.create_root_accumulator_object_for_testing();
                // for some reason all 4 nodes are not reliably submitting capability messages
                cfg.set_buffer_stake_for_protocol_upgrade_bps_for_testing(0);
            } else if version == ProtocolVersion::MAX_ALLOWED {
                cfg.enable_accumulators_for_testing();
            }
            cfg
        }))
        .with_test_cluster_builder_cb(Box::new(|builder| {
            builder
                // We start at MAX - 1 to be sure the root object isn't created in genesis, and then
                // immediately reconfigure.
                .with_protocol_version(ProtocolVersion::MAX - 1)
                .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
                    ProtocolVersion::MAX.as_u64() - 1,
                    ProtocolVersion::MAX_ALLOWED.as_u64(),
                ))
        }))
        .build()
        .await;

    test_env.trigger_reconfiguration().await;

    let (sender, gas) = test_env.get_sender_and_gas(0);

    let recipient = SuiAddress::random_for_testing_only();

    // Withdraw must be rejected at signing.
    let withdraw_tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(1000),
            vec![(1000, dbg_addr(2))],
        )
        .build();
    test_env.exec_tx_directly(withdraw_tx).await.unwrap_err();

    // Transfer fails at execution time
    let tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(1000, recipient)])
        .build();

    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    let status = effects.status().clone();
    assert!(status.is_err());

    // we reconfigure, and create the accumulator root at the end of this epoch.
    // but because the root did not exist during this epoch, we don't upgrade to
    // the next protocol version yet.
    test_env.trigger_reconfiguration().await;

    // Withdraw must still be rejected at signing.
    let withdraw_tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(1000),
            vec![(1000, dbg_addr(2))],
        )
        .build();
    test_env.exec_tx_directly(withdraw_tx).await.unwrap_err();

    // transfer fails at execution time
    let (_, gas) = test_env.get_sender_and_gas(0);
    let tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(1000, recipient)])
        .build();

    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    let status = effects.status().clone();
    assert!(status.is_err());

    // after one more reconfig, we can upgrade to the next protocol version.
    test_env.trigger_reconfiguration().await;

    let (_, gas) = test_env.get_sender_and_gas(0);
    let tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(1000, sender)])
        .build();
    test_env.exec_tx_directly(tx).await.unwrap();

    test_env.verify_accumulator_exists(sender, 1000);

    // Withdraw can succeed now
    let withdraw_tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(1000),
            vec![(1000, dbg_addr(2))],
        )
        .build();
    test_env.exec_tx_directly(withdraw_tx).await.unwrap();

    test_env.verify_accumulator_removed(sender);

    // ensure that no conservation failures are detected during reconfig.
    test_env.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_deposits() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.create_root_accumulator_object_for_testing();
            cfg.enable_accumulators_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, gas) = test_env.get_sender_and_gas(0);
    let recipient = SuiAddress::random_for_testing_only();

    let tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(1000, recipient)])
        .build();
    test_env.exec_tx_directly(tx).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let (_, gas) = test_env.get_sender_and_gas(0);
    let tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(1000, recipient)])
        .build();
    test_env.exec_tx_directly(tx).await.unwrap();

    test_env.verify_accumulator_exists(recipient, 2000);
    // Verify the accumulator object count after settlement.
    test_env.verify_accumulator_object_count(1);

    test_env.cluster.fullnode_handle.sui_node.with(|node| {

        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();

        // Ensure that the accumulator root object is considered a read-only InputConsensusObject
        // by the settlement transaction.
        let sui_coin_type = Balance::type_tag(GAS::type_tag());
        let accumulator_object =
            AccumulatorValue::load_object(child_object_resolver, None, recipient, &sui_coin_type)
                .expect("read cannot fail")
                .expect("accumulator should exist");
        let settlement_digest = accumulator_object.previous_transaction;
        let settlement_effects = state
            .get_transaction_cache_reader()
            .get_executed_effects(&settlement_digest)
            .expect("settlement digest should exist");
        let input_consensus_objects = settlement_effects.input_consensus_objects();
        input_consensus_objects.iter().find(|input_consensus_object| {
            matches!(input_consensus_object, InputConsensusObject::ReadOnly(obj_ref) if obj_ref.0 == SUI_ACCUMULATOR_ROOT_OBJECT_ID)
        }).expect("settlement should have accumulator root object as read-only input consensus object");
    });

    test_env.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_multiple_settlement_txns() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_accumulators_for_testing();
            cfg.set_max_updates_per_settlement_txn_for_testing(3);
            cfg
        }))
        .build()
        .await;

    let (sender, gas) = test_env.get_sender_and_gas(0);
    let recipient = SuiAddress::random_for_testing_only();

    let amounts_and_recipients = (0..20)
        .map(|_| (1u64, SuiAddress::random_for_testing_only()))
        .collect::<Vec<_>>();

    let tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(FundSource::coin(gas), amounts_and_recipients.clone())
        .build();
    test_env.exec_tx_directly(tx).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let (_, gas) = test_env.get_sender_and_gas(0);
    let tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(1000, recipient)])
        .build();
    test_env.exec_tx_directly(tx).await.unwrap();

    for (amount, recipient) in amounts_and_recipients {
        test_env.verify_accumulator_exists(recipient, amount);
    }
    // Verify the accumulator object count after settlement.
    test_env.verify_accumulator_object_count(21);

    test_env.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_deposit_and_withdraw() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.create_root_accumulator_object_for_testing();
            cfg.enable_accumulators_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);

    test_env.fund_one_address_balance(sender, 1000).await;
    test_env.verify_accumulator_exists(sender, 1000);

    let tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(1000),
            vec![(1000, dbg_addr(2))],
        )
        .build();
    test_env.exec_tx_directly(tx).await.unwrap();
    // Verify the accumulator object count after settlement.
    test_env.verify_accumulator_object_count(1);
    test_env.verify_accumulator_removed(sender);
    test_env.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_deposit_and_withdraw_with_larger_reservation() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.create_root_accumulator_object_for_testing();
            cfg.enable_accumulators_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);

    test_env.fund_one_address_balance(sender, 1000).await;

    // Withdraw 800 with a reservation of 1000 (larger than actual withdrawal)
    let tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(1000),
            vec![(800, dbg_addr(2))],
        )
        .build();
    test_env.exec_tx_directly(tx).await.unwrap();

    // Verify that the accumulator still exists, as the entire balance was not withdrawn
    test_env.verify_accumulator_exists(sender, 200);
    // Verify the accumulator object count after settlement.
    // 2 accounts (sender with 200, dbg_addr(2) with 800)
    test_env.verify_accumulator_object_count(2);
    test_env.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_withdraw_non_existent_balance() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.create_root_accumulator_object_for_testing();
            cfg.enable_accumulators_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);

    // Settlement transaction fails because balance doesn't exist
    let tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(1000),
            vec![(1000, dbg_addr(2))],
        )
        .build();
    let err = test_env.exec_tx_directly(tx).await.unwrap_err();

    assert!(err.to_string().contains("is less than requested"));
}

#[sim_test]
async fn test_withdraw_insufficient_balance() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.create_root_accumulator_object_for_testing();
            cfg.enable_accumulators_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, gas) = test_env.get_sender_and_all_gas(0);
    let gas1 = gas[0];

    // send 1000 from our gas coin to our balance
    let tx = test_env
        .tx_builder_with_gas(sender, gas1)
        .transfer_sui_to_address_balance(FundSource::coin(gas1), vec![(1000, sender)])
        .build();
    test_env.exec_tx_directly(tx).await.unwrap();

    let gas1 = test_env.get_sender_and_gas(0).1;

    // Try to withdraw 1001 from balance - should fail
    let tx = test_env
        .tx_builder_with_gas(sender, gas1)
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(1001),
            vec![(1001, dbg_addr(2))],
        )
        .build();
    let err = test_env.exec_tx_directly(tx).await.unwrap_err();
    assert!(err.to_string().contains("is less than requested"));

    // Refresh gas1 after the failed transaction
    let (sender, gas) = test_env.get_sender_and_all_gas(0);

    // Now exceed the balance with two transactions, each of which can be certified
    // The second one will fail at execution time
    let tx1 = test_env
        .tx_builder_with_gas(sender, gas[0])
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(500),
            vec![(500, dbg_addr(2))],
        )
        .build();
    let tx2 = test_env
        .tx_builder_with_gas(sender, gas[1])
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(501),
            vec![(501, dbg_addr(2))],
        )
        .build();

    let mut effects = test_env
        .cluster
        .sign_and_execute_txns_in_soft_bundle(&[tx1, tx2])
        .await
        .unwrap();

    let effects2 = effects.pop().unwrap().1;
    let effects1 = effects.pop().unwrap().1;

    assert!(effects1.status().is_ok(), "Expected transaction to succeed");
    assert!(
        effects2.status().is_err(),
        "Expected transaction to fail due to insufficient balance"
    );

    test_env.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_address_balance_gas() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg
        }))
        .build()
        .await;

    let funding_amount = 100_000_000;

    let (sender, gas_package_id) =
        setup_address_balance_account(&mut test_env, funding_amount).await;

    test_env.verify_accumulator_exists(sender, funding_amount);
    // Verify the accumulator object count after settlement.
    test_env.verify_accumulator_object_count(1);

    // check that a transaction with a zero gas budget is rejected
    {
        let mut tx = create_storage_test_transaction_address_balance(
            sender,
            gas_package_id,
            test_env.rgp,
            test_env.chain_id,
            None,
            0,
        );
        tx.gas_data_mut().budget = 0;
        let signed_tx = test_env.cluster.sign_transaction(&tx).await;
        let err = test_env
            .cluster
            .wallet
            .execute_transaction_may_fail(signed_tx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Gas budget: 0 is lower than min:"));
    }

    let tx = create_storage_test_transaction_address_balance(
        sender,
        gas_package_id,
        test_env.rgp,
        test_env.chain_id,
        None,
        0,
    );

    let signed_tx = test_env.cluster.sign_transaction(&tx).await;
    let (effects, _) = test_env
        .cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await
        .expect("Transaction should succeed with address balance gas payment");

    assert!(
        effects.status().is_ok(),
        "Expected transaction to succeed with address balance gas payment, but got error: {:?}",
        effects.status()
    );

    let gas_summary = effects.gas_cost_summary();
    let gas_used = calculate_total_gas_cost(gas_summary);

    assert!(
        gas_used > 0,
        "Gas used should be greater than 0 with Move function calls, got: {}",
        gas_used
    );

    let expected_balance = funding_amount - gas_used;

    test_env.verify_accumulator_exists(sender, expected_balance);

    // Test tx with no input objects.
    let tx = test_env
        .tx_builder_with_gas_objects(sender, vec![])
        .with_address_balance_gas(test_env.chain_id, 0, 0)
        .with_gas_budget(10_000_000)
        .build();

    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok());

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_sponsored_address_balance_storage_rebates() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg
        }))
        .build()
        .await;

    let gas_test_package_id = test_env.setup_test_package(move_test_code_path()).await;

    let (sender, sender_gas) = test_env.get_sender_and_gas(0);
    let (sponsor, sponsor_gas) = test_env.get_sender_and_gas(1);

    let deposit_tx_sender = test_env
        .tx_builder_with_gas(sender, sender_gas)
        .transfer_sui_to_address_balance(FundSource::coin(sender_gas), vec![(100_000_000, sender)])
        .build();
    test_env.exec_tx_directly(deposit_tx_sender).await.unwrap();

    let deposit_tx_sponsor = test_env
        .tx_builder_with_gas(sponsor, sponsor_gas)
        .transfer_sui_to_address_balance(
            FundSource::coin(sponsor_gas),
            vec![(100_000_000, sponsor)],
        )
        .build();
    test_env.exec_tx_directly(deposit_tx_sponsor).await.unwrap();

    let create_txn = create_storage_test_transaction_address_balance(
        sender,
        gas_test_package_id,
        test_env.rgp,
        test_env.chain_id,
        Some(sponsor),
        0,
    );

    let sender_sig = test_env
        .cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sender, &create_txn, Intent::sui_transaction())
        .await
        .unwrap();
    let sponsor_sig = test_env
        .cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sponsor, &create_txn, Intent::sui_transaction())
        .await
        .unwrap();

    let signed_create_txn = Transaction::from_data(create_txn, vec![sender_sig, sponsor_sig]);
    let create_resp = test_env
        .cluster
        .execute_transaction(signed_create_txn)
        .await;
    let create_effects = create_resp.effects;

    assert!(
        create_effects.status().is_ok(),
        "Sponsored storage transaction should succeed, but got error: {:?}",
        create_effects.status()
    );

    let gas_summary = create_effects.gas_cost_summary();
    let gas_used = calculate_total_gas_cost(gas_summary);

    assert!(
        gas_used > 0,
        "Gas should be charged for sponsored transaction, got: {}",
        gas_used
    );

    let sponsor_actual = test_env.get_sui_balance(sponsor);
    let sender_actual = test_env.get_sui_balance(sender);

    assert!(
        sponsor_actual < 100_000_000,
        "Sponsor balance should have decreased from 100_000_000, got: {}",
        sponsor_actual
    );
    assert_eq!(
        sender_actual, 100_000_000,
        "Sender balance should remain at 100_000_000, got: {}",
        sender_actual
    );

    let created_objects: Vec<_> = create_effects.created();
    assert_eq!(
        created_objects.len(),
        1,
        "Should have created exactly one object"
    );
    let created_obj = created_objects[0].0;
    let delete_txn = create_delete_transaction_address_balance(
        sender,
        gas_test_package_id,
        created_obj,
        test_env.rgp,
        test_env.chain_id,
        Some(sponsor),
    );

    let sender_delete_sig = test_env
        .cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sender, &delete_txn, Intent::sui_transaction())
        .await
        .unwrap();
    let sponsor_delete_sig = test_env
        .cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sponsor, &delete_txn, Intent::sui_transaction())
        .await
        .unwrap();

    let signed_delete_txn =
        Transaction::from_data(delete_txn, vec![sender_delete_sig, sponsor_delete_sig]);
    let delete_resp = test_env
        .cluster
        .execute_transaction(signed_delete_txn)
        .await;
    let delete_effects = delete_resp.effects;

    assert!(
        delete_effects.status().is_ok(),
        "Sponsored delete transaction should succeed, but got error: {:?}",
        delete_effects.status()
    );

    let delete_gas_summary = delete_effects.gas_cost_summary();
    assert!(
        delete_gas_summary.storage_rebate > 0,
        "Should receive storage rebate when deleting object, got: {}",
        delete_gas_summary.storage_rebate
    );

    let sponsor_final = test_env.get_sui_balance(sponsor);
    let sender_final = test_env.get_sui_balance(sender);

    assert_eq!(
        sender_final, 100_000_000,
        "Sender balance should remain unchanged at 100_000_000"
    );
    assert_ne!(
        sponsor_final, 100_000_000,
        "Sponsor balance should have changed from 100_000_000"
    );

    test_env.cluster.trigger_reconfiguration().await;
}

fn move_test_code_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code");
    path
}

async fn setup_address_balance_account(
    test_env: &mut TestEnv,
    deposit_amount: u64,
) -> (SuiAddress, ObjectID) {
    let gas_package_id = test_env.setup_test_package(move_test_code_path()).await;

    let (sender, gas) = test_env.get_sender_and_gas(0);

    let deposit_tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(deposit_amount, sender)])
        .build();
    test_env.exec_tx_directly(deposit_tx).await.unwrap();

    (sender, gas_package_id)
}

fn calculate_total_gas_cost(gas_summary: &GasCostSummary) -> u64 {
    gas_summary.computation_cost + gas_summary.storage_cost + gas_summary.non_refundable_storage_fee
}

fn create_storage_test_transaction_kind(gas_test_package_id: ObjectID) -> TransactionKind {
    let mut builder = ProgrammableTransactionBuilder::new();
    let value = builder.pure(42u64).unwrap();
    builder.programmable_move_call(
        gas_test_package_id,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("create_object_with_storage").unwrap(),
        vec![],
        vec![value],
    );
    TransactionKind::ProgrammableTransaction(builder.finish())
}

fn create_abort_test_transaction_kind(
    gas_test_package_id: ObjectID,
    should_abort: bool,
) -> TransactionKind {
    let mut builder = ProgrammableTransactionBuilder::new();
    let should_abort_arg = builder.pure(should_abort).unwrap();
    builder.programmable_move_call(
        gas_test_package_id,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("abort_with_computation").unwrap(),
        vec![],
        vec![should_abort_arg],
    );
    TransactionKind::ProgrammableTransaction(builder.finish())
}

fn create_storage_test_transaction_gas(
    sender: SuiAddress,
    gas_test_package_id: ObjectID,
    gas_coin: ObjectRef,
    rgp: u64,
) -> TransactionData {
    let tx = create_storage_test_transaction_kind(gas_test_package_id);

    TransactionData::V1(TransactionDataV1 {
        kind: tx,
        sender,
        gas_data: GasData {
            payment: vec![gas_coin],
            owner: sender,
            price: rgp,
            budget: 10000000,
        },
        expiration: TransactionExpiration::None,
    })
}

fn create_address_balance_transaction(
    tx_kind: TransactionKind,
    sender: SuiAddress,
    budget: u64,
    rgp: u64,
    chain_id: ChainIdentifier,
) -> TransactionData {
    create_address_balance_transaction_with_sponsor(tx_kind, sender, None, budget, rgp, chain_id)
}

fn create_address_balance_transaction_with_sponsor(
    tx_kind: TransactionKind,
    sender: SuiAddress,
    sponsor: Option<SuiAddress>,
    budget: u64,
    rgp: u64,
    chain_id: ChainIdentifier,
) -> TransactionData {
    let gas_owner = sponsor.unwrap_or(sender);
    TransactionData::V1(TransactionDataV1 {
        kind: tx_kind,
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: gas_owner,
            price: rgp,
            budget,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: chain_id,
            nonce: 0,
        },
    })
}

fn create_storage_test_transaction_address_balance(
    sender: SuiAddress,
    gas_test_package_id: ObjectID,
    rgp: u64,
    chain_id: ChainIdentifier,
    sponsor: Option<SuiAddress>,
    nonce: u32,
) -> TransactionData {
    let tx = create_storage_test_transaction_kind(gas_test_package_id);
    let gas_owner = sponsor.unwrap_or(sender);

    TransactionData::V1(TransactionDataV1 {
        kind: tx,
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: gas_owner,
            price: rgp,
            budget: 10000000,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: chain_id,
            nonce,
        },
    })
}

fn create_delete_test_transaction_kind(
    gas_test_package_id: ObjectID,
    object_to_delete: ObjectRef,
) -> TransactionKind {
    let mut builder = ProgrammableTransactionBuilder::new();
    let object_arg = builder
        .obj(ObjectArg::ImmOrOwnedObject(object_to_delete))
        .unwrap();
    builder.programmable_move_call(
        gas_test_package_id,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("delete_object").unwrap(),
        vec![],
        vec![object_arg],
    );
    TransactionKind::ProgrammableTransaction(builder.finish())
}

fn create_delete_transaction_gas(
    sender: SuiAddress,
    gas_test_package_id: ObjectID,
    object_to_delete: ObjectRef,
    gas_coin: ObjectRef,
    rgp: u64,
) -> TransactionData {
    let tx = create_delete_test_transaction_kind(gas_test_package_id, object_to_delete);

    TransactionData::V1(TransactionDataV1 {
        kind: tx,
        sender,
        gas_data: GasData {
            payment: vec![gas_coin],
            owner: sender,
            price: rgp,
            budget: 10000000,
        },
        expiration: TransactionExpiration::None,
    })
}

fn create_delete_transaction_address_balance(
    sender: SuiAddress,
    gas_test_package_id: ObjectID,
    object_to_delete: ObjectRef,
    rgp: u64,
    chain_id: ChainIdentifier,
    sponsor: Option<SuiAddress>,
) -> TransactionData {
    let tx = create_delete_test_transaction_kind(gas_test_package_id, object_to_delete);
    let gas_owner = sponsor.unwrap_or(sender);

    TransactionData::V1(TransactionDataV1 {
        kind: tx,
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: gas_owner,
            price: rgp,
            budget: 10000000,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: chain_id,
            nonce: 0u32,
        },
    })
}

fn create_abort_test_transaction_address_balance(
    sender: SuiAddress,
    gas_test_package_id: ObjectID,
    rgp: u64,
    chain_id: ChainIdentifier,
    should_abort: bool,
    sponsor: Option<SuiAddress>,
) -> TransactionData {
    let tx = create_abort_test_transaction_kind(gas_test_package_id, should_abort);
    let gas_owner = sponsor.unwrap_or(sender);

    TransactionData::V1(TransactionDataV1 {
        kind: tx,
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: gas_owner,
            price: rgp,
            budget: 10000000,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: chain_id,
            nonce: 0u32,
        },
    })
}

fn create_withdraw_balance_transaction(
    sender: SuiAddress,
    rgp: u64,
    chain_id: ChainIdentifier,
    withdraw_amount: u64,
    nonce: u32,
) -> TransactionData {
    let mut builder = ProgrammableTransactionBuilder::new();

    let withdraw_arg = FundsWithdrawalArg::balance_from_sender(
        withdraw_amount,
        sui_types::gas_coin::GAS::type_tag(),
    );
    let withdraw_arg = builder.funds_withdrawal(withdraw_arg).unwrap();

    let amount_arg = builder.pure(U256::from(withdraw_amount)).unwrap();

    let split_withdraw_arg = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("funds_accumulator").unwrap(),
        Identifier::new("withdrawal_split").unwrap(),
        vec!["0x2::balance::Balance<0x2::sui::SUI>".parse().unwrap()],
        vec![withdraw_arg, amount_arg],
    );

    let coin = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("redeem_funds").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![split_withdraw_arg],
    );

    builder.transfer_arg(sender, coin);

    let tx = TransactionKind::ProgrammableTransaction(builder.finish());
    TransactionData::V1(TransactionDataV1 {
        kind: tx,
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: sender,
            price: rgp,
            budget: 10000000,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: chain_id,
            nonce,
        },
    })
}

fn create_address_balance_transaction_with_expiration(
    sender: SuiAddress,
    rgp: u64,
    min_epoch: Option<u64>,
    max_epoch: Option<u64>,
    chain_id: ChainIdentifier,
    nonce: u32,
    gas_test_package_id: ObjectID,
) -> TransactionData {
    use sui_types::transaction::{GasData, TransactionDataV1, TransactionExpiration};

    let mut builder = ProgrammableTransactionBuilder::new();
    let value = builder.pure(42u64).unwrap();
    builder.programmable_move_call(
        gas_test_package_id,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("create_object_with_storage").unwrap(),
        vec![],
        vec![value],
    );
    let tx = TransactionKind::ProgrammableTransaction(builder.finish());

    TransactionData::V1(TransactionDataV1 {
        kind: tx,
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: sender,
            price: rgp,
            budget: 10000000,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch,
            max_epoch,
            min_timestamp: None,
            max_timestamp: None,
            chain: chain_id,
            nonce,
        },
    })
}

fn create_regular_gas_transaction_with_current_epoch(
    sender: SuiAddress,
    gas_coin: ObjectRef,
    rgp: u64,
    current_epoch: u64,
    chain_id: ChainIdentifier,
) -> TransactionData {
    let mut builder = ProgrammableTransactionBuilder::new();

    let amount = builder.pure(1000u64).unwrap();
    let coin = builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount]));
    let Argument::Result(coin_idx) = coin else {
        panic!("coin is not a result");
    };

    let coin = Argument::NestedResult(coin_idx, 0);
    builder.transfer_arg(sender, coin);

    let tx = TransactionKind::ProgrammableTransaction(builder.finish());

    TransactionData::V1(TransactionDataV1 {
        kind: tx,
        sender,
        gas_data: GasData {
            payment: vec![gas_coin],
            owner: sender,
            price: rgp,
            budget: 10000000,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(current_epoch),
            max_epoch: Some(current_epoch),
            min_timestamp: None,
            max_timestamp: None,
            chain: chain_id,
            nonce: 12345,
        },
    })
}

#[sim_test]
async fn test_regular_gas_payment_with_valid_during_current_epoch() {
    let mut test_env = TestEnvBuilder::new().with_num_validators(1).build().await;

    let (sender, gas_coin) = test_env.get_sender_and_gas(0);
    let current_epoch = 0;

    let tx = create_regular_gas_transaction_with_current_epoch(
        sender,
        gas_coin,
        test_env.rgp,
        current_epoch,
        test_env.chain_id,
    );

    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    assert!(
        effects.status().is_ok(),
        "Transaction should execute successfully. Error: {:?}",
        effects.status()
    );
}

#[sim_test]
async fn test_transaction_expired_too_early() {
    let mut test_env = TestEnvBuilder::new().with_num_validators(1).build().await;

    let (sender, gas_coin) = test_env.get_sender_and_gas(0);
    let future_epoch = 10;

    let tx = create_regular_gas_transaction_with_current_epoch(
        sender,
        gas_coin,
        test_env.rgp,
        future_epoch,
        test_env.chain_id,
    );

    let result = test_env.exec_tx_directly(tx).await;

    match result {
        Err(err) => {
            let err_str = err.to_string();
            assert!(
                err_str.contains("Transaction Expired"),
                "Expected Transaction Expired error, got: {}",
                err_str
            );
        }
        Ok(_) => panic!("Transaction should be rejected when epoch is too early"),
    }
}

#[sim_test]
async fn test_transaction_expired_too_late() {
    let mut test_env = TestEnvBuilder::new().with_num_validators(1).build().await;

    let (sender, gas_coin) = test_env.get_sender_and_gas(0);

    let past_epoch = 0;

    // trigger epoch 2
    test_env.trigger_reconfiguration().await;
    test_env.trigger_reconfiguration().await;

    let tx = create_regular_gas_transaction_with_current_epoch(
        sender,
        gas_coin,
        test_env.rgp,
        past_epoch,
        test_env.chain_id,
    );

    let result = test_env.exec_tx_directly(tx).await;

    match result {
        Err(err) => {
            let err_str = err.to_string();
            assert!(
                err_str.contains("Transaction Expired"),
                "Expected Transaction Expired error, got: {}",
                err_str
            );
        }
        Ok(_) => panic!("Transaction should be rejected when epoch is too late"),
    }
}

#[sim_test]
async fn test_transaction_invalid_chain_id() {
    let mut test_env = TestEnvBuilder::new().with_num_validators(1).build().await;

    let (sender, gas_coin) = test_env.get_sender_and_gas(0);
    let current_epoch = 0;

    let wrong_chain_id = ChainIdentifier::from(CheckpointDigest::default());

    let tx = create_regular_gas_transaction_with_current_epoch(
        sender,
        gas_coin,
        test_env.rgp,
        current_epoch,
        wrong_chain_id,
    );

    let result = test_env.exec_tx_directly(tx).await;

    match result {
        Err(err) => {
            let err_str = err.to_string();
            assert!(
                err_str.contains("does not match network chain ID"),
                "Expected chain ID mismatch error, got: {}",
                err_str
            );
        }
        Ok(_) => panic!("Transaction should be rejected with invalid chain ID"),
    }
}

#[sim_test]
async fn test_transaction_expiration_min_none_max_some() {
    let mut test_env = TestEnvBuilder::new().with_num_validators(1).build().await;

    let (sender, gas_coin) = test_env.get_sender_and_gas(0);
    let current_epoch = 0;

    let tx = create_transaction_with_expiration(
        sender,
        gas_coin,
        test_env.rgp,
        None,
        Some(current_epoch + 5),
        test_env.chain_id,
        12345,
    );

    let result = test_env.exec_tx_directly(tx).await;

    let err = result.expect_err("Transaction should be rejected when only max_epoch is specified");
    let err_str = format!("{:?}", err);
    assert!(
        err_str.contains("Both min_epoch and max_epoch must be specified"),
        "Expected validation error 'Both min_epoch and max_epoch must be specified', got: {:?}",
        err
    );
}

#[sim_test]
async fn test_transaction_expiration_edge_cases() {
    let mut test_env = TestEnvBuilder::new().with_num_validators(1).build().await;

    let (sender, gas_objects) = test_env.get_sender_and_all_gas(0);
    let gas_coin1 = gas_objects[0];
    let gas_coin2 = gas_objects[1];
    let gas_coin3 = gas_objects[2];
    let current_epoch = 0;

    // Test case 1: min_epoch = max_epoch (single epoch window)
    let tx1 = create_transaction_with_expiration(
        sender,
        gas_coin1,
        test_env.rgp,
        Some(current_epoch), // min_epoch: Some(current)
        Some(current_epoch), // max_epoch: Some(current) - single epoch window
        test_env.chain_id,
        100,
    );

    let result1 = test_env.exec_tx_directly(tx1).await;
    assert!(
        result1.is_ok(),
        "Single epoch window transaction should succeed"
    );

    // Test case 2: Transaction with min_epoch in the future should be rejected as not yet valid
    let tx2 = create_transaction_with_expiration(
        sender,
        gas_coin2,
        test_env.rgp,
        Some(current_epoch + 1),
        Some(current_epoch + 1),
        test_env.chain_id,
        200,
    );

    let result2 = test_env.exec_tx_directly(tx2).await;
    let err2 = result2.expect_err("Transaction should be rejected when min_epoch is in the future");
    let err_str2 = err2.to_string();
    assert!(
        err_str2.contains("Transaction Expired"),
        "Expected Transaction Expired for future min_epoch, got: {}",
        err_str2
    );

    // Test case 3: min_epoch: Some(past), max_epoch: Some(past) - expired
    // First trigger an epoch change to make current_epoch "past"
    test_env.trigger_reconfiguration().await;

    let tx3 = create_transaction_with_expiration(
        sender,
        gas_coin3,
        test_env.rgp,
        Some(0), // min_epoch: Some(past)
        Some(0), // max_epoch: Some(past) - now expired
        test_env.chain_id,
        300,
    );

    let result3 = test_env.exec_tx_directly(tx3).await;
    match result3 {
        Err(err) => {
            let err_str = err.to_string();
            assert!(
                err_str.contains("Transaction Expired"),
                "Expected Transaction Expired for past max_epoch, got: {}",
                err_str
            );
        }
        Ok(_) => panic!("Transaction should be rejected when max_epoch is in the past"),
    }
}

#[sim_test]
async fn test_address_balance_gas_cost_parity() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg
        }))
        .build()
        .await;
    let (sender, gas_test_package_id) =
        setup_address_balance_account(&mut test_env, 100_000_000).await;

    let (_, gas_coin) = test_env.get_sender_and_gas(0);

    let coin_tx =
        create_storage_test_transaction_gas(sender, gas_test_package_id, gas_coin, test_env.rgp);
    let (_, coin_effects) = test_env.exec_tx_directly(coin_tx).await.unwrap();

    assert!(
        coin_effects.status().is_ok(),
        "Coin payment storage transaction should succeed, but got error: {:?}",
        coin_effects.status()
    );

    let address_balance_tx = create_storage_test_transaction_address_balance(
        sender,
        gas_test_package_id,
        test_env.rgp,
        test_env.chain_id,
        None,
        0,
    );
    let (_, address_balance_effects) = test_env.exec_tx_directly(address_balance_tx).await.unwrap();

    assert!(
        address_balance_effects.status().is_ok(),
        "Address balance payment storage transaction should succeed, but got error: {:?}",
        address_balance_effects.status()
    );

    let coin_gas_summary = coin_effects.gas_cost_summary();
    let address_balance_gas_summary = address_balance_effects.gas_cost_summary();

    // This txn has stores an object and incurs storage costs
    // Coin gas transaction has higher storage costs because it mutates the gas coin object
    assert!(
        coin_gas_summary.storage_cost > 0,
        "Coin payment should have storage costs for object creation"
    );
    assert!(
        address_balance_gas_summary.storage_cost > 0,
        "Address balance payment should have storage costs for object creation"
    );
    assert!(
        coin_gas_summary.storage_cost > address_balance_gas_summary.storage_cost,
        "Gas coin storage cost should be higher due to gas coin mutation overhead"
    );
    assert_eq!(
        coin_gas_summary.computation_cost, address_balance_gas_summary.computation_cost,
        "Computation costs should be identical for the same transaction"
    );

    let coin_created_objects: Vec<_> = coin_effects.created().to_vec();
    let address_balance_created_objects: Vec<_> = address_balance_effects.created().to_vec();

    assert_eq!(
        coin_created_objects.len(),
        1,
        "Should have created exactly one object with coin payment"
    );
    assert_eq!(
        address_balance_created_objects.len(),
        1,
        "Should have created exactly one object with address balance payment"
    );

    let coin_created_obj = coin_created_objects[0].0;
    let address_balance_created_obj = address_balance_created_objects[0].0;

    let (_, gas_coin_for_delete) = test_env.get_sender_and_gas(0);

    let coin_delete_tx = create_delete_transaction_gas(
        sender,
        gas_test_package_id,
        coin_created_obj,
        gas_coin_for_delete,
        test_env.rgp,
    );
    let (_, coin_delete_effects) = test_env.exec_tx_directly(coin_delete_tx).await.unwrap();

    let address_balance_delete_tx = create_delete_transaction_address_balance(
        sender,
        gas_test_package_id,
        address_balance_created_obj,
        test_env.rgp,
        test_env.chain_id,
        None,
    );
    let address_balance_delete_resp = test_env
        .exec_tx_directly(address_balance_delete_tx)
        .await
        .unwrap();
    let (_, address_balance_delete_effects) = address_balance_delete_resp;

    let coin_delete_gas_summary = coin_delete_effects.gas_cost_summary();
    let address_balance_delete_gas_summary = address_balance_delete_effects.gas_cost_summary();

    assert!(
        coin_delete_effects.status().is_ok(),
        "Coin deletion should succeed"
    );
    assert!(
        address_balance_delete_effects.status().is_ok(),
        "Address balance deletion should succeed"
    );

    assert!(
        coin_delete_gas_summary.storage_rebate > 0,
        "Coin deletion should provide non-zero storage rebate"
    );
    assert!(
        address_balance_delete_gas_summary.storage_rebate > 0,
        "Address balance deletion should provide non-zero storage rebate"
    );

    assert_eq!(
        coin_delete_gas_summary.computation_cost,
        address_balance_delete_gas_summary.computation_cost,
        "Deletion computation costs should be identical for both payment methods"
    );

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_address_balance_gas_charged_on_move_abort() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, gas_test_package_id) =
        setup_address_balance_account(&mut test_env, 10_000_000).await;

    test_env.verify_accumulator_exists(sender, 10_000_000);
    // Verify the accumulator object count after settlement.
    test_env.verify_accumulator_object_count(1);

    let abort_tx = create_abort_test_transaction_address_balance(
        sender,
        gas_test_package_id,
        test_env.rgp,
        test_env.chain_id,
        true,
        None,
    );

    let (_, effects) = test_env.exec_tx_directly(abort_tx).await.unwrap();

    assert!(
        effects.status().is_err(),
        "Expected transaction to fail due to Move abort"
    );

    let gas_summary = effects.gas_cost_summary();
    let gas_used = calculate_total_gas_cost(gas_summary);

    assert!(
        gas_used > 0,
        "Gas should still be charged even on Move abort, got: {}",
        gas_used
    );
    assert!(
        gas_summary.computation_cost > 0,
        "Computation cost should be charged for work done before abort"
    );

    let expected_balance = 10_000_000 - gas_used;

    test_env.verify_accumulator_exists(sender, expected_balance);

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_explicit_sponsor_withdrawal_banned() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg
        }))
        .with_test_cluster_builder_cb(Box::new(|builder| {
            builder
                .with_protocol_version(ProtocolConfig::get_for_max_version_UNSAFE().version)
                .with_epoch_duration_ms(600000)
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);
    let sponsor = test_env.get_sender(1);

    let mut builder = ProgrammableTransactionBuilder::new();
    let withdrawal = FundsWithdrawalArg::balance_from_sponsor(1000, GAS::type_tag());
    builder.funds_withdrawal(withdrawal).unwrap();

    let tx = TransactionData::V1(TransactionDataV1 {
        kind: TransactionKind::ProgrammableTransaction(builder.finish()),
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: sponsor,
            price: test_env.rgp,
            budget: 10000000,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: test_env.chain_id,
            nonce: 0u32,
        },
    });

    let result = test_env.exec_tx_directly(tx).await;

    let err = result.expect_err("Transaction with explicit sponsor withdrawal should be rejected");
    let err_str = format!("{:?}", err);
    assert!(
        err_str.contains("Explicit sponsor withdrawals are not yet supported"),
        "Error should mention that sponsor withdrawals are not supported, got: {}",
        err_str
    );
}

#[sim_test]
async fn test_sponsor_insufficient_balance_charges_zero_gas() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg
        }))
        .build()
        .await;

    let gas_test_package_id = test_env.setup_test_package(move_test_code_path()).await;

    test_env.update_all_gas().await;
    let (sender, sender_gas) = test_env.get_sender_and_gas(0);
    let (sponsor, sponsor_gas) = test_env.get_sender_and_gas(1);

    let deposit_tx_sender = test_env
        .tx_builder_with_gas(sender, sender_gas)
        .transfer_sui_to_address_balance(FundSource::coin(sender_gas), vec![(100_000_000, sender)])
        .build();
    test_env.exec_tx_directly(deposit_tx_sender).await.unwrap();

    let sponsor_initial_balance = 15_000_000u64;
    let deposit_tx_sponsor = test_env
        .tx_builder_with_gas(sponsor, sponsor_gas)
        .transfer_sui_to_address_balance(
            FundSource::coin(sponsor_gas),
            vec![(sponsor_initial_balance, sponsor)],
        )
        .build();
    test_env.exec_tx_directly(deposit_tx_sponsor).await.unwrap();
    test_env.verify_accumulator_exists(sponsor, sponsor_initial_balance);

    let tx1 = create_storage_test_transaction_address_balance(
        sender,
        gas_test_package_id,
        test_env.rgp,
        test_env.chain_id,
        Some(sponsor),
        0,
    );
    let tx2 = create_storage_test_transaction_address_balance(
        sender,
        gas_test_package_id,
        test_env.rgp,
        test_env.chain_id,
        Some(sponsor),
        1,
    );

    let sender_sig1 = test_env
        .cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sender, &tx1, Intent::sui_transaction())
        .await
        .unwrap();
    let sponsor_sig1 = test_env
        .cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sponsor, &tx1, Intent::sui_transaction())
        .await
        .unwrap();
    let signed_tx1 = Transaction::from_data(tx1, vec![sender_sig1, sponsor_sig1]);

    let sender_sig2 = test_env
        .cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sender, &tx2, Intent::sui_transaction())
        .await
        .unwrap();
    let sponsor_sig2 = test_env
        .cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sponsor, &tx2, Intent::sui_transaction())
        .await
        .unwrap();
    let signed_tx2 = Transaction::from_data(tx2, vec![sender_sig2, sponsor_sig2]);

    let tx1_digest = *signed_tx1.digest();
    let tx2_digest = *signed_tx2.digest();

    assert_ne!(
        tx1_digest, tx2_digest,
        "Transaction digests should be different"
    );

    let mut effects = test_env
        .cluster
        .execute_signed_txns_in_soft_bundle(&[signed_tx1, signed_tx2])
        .await
        .unwrap();

    test_env
        .cluster
        .wait_for_tx_settlement(&[tx1_digest, tx2_digest])
        .await;

    let tx2_effects = effects.pop().unwrap().1;
    let tx1_effects = effects.pop().unwrap().1;

    let tx1_gas = tx1_effects.gas_cost_summary();
    let tx1_total_gas = calculate_total_gas_cost(tx1_gas);
    let tx2_gas = tx2_effects.gas_cost_summary();
    let tx2_total_gas = calculate_total_gas_cost(tx2_gas);

    let (succeeded_effects, succeeded_gas, failed_effects, failed_gas) =
        if tx1_effects.status().is_ok() {
            (&tx1_effects, tx1_total_gas, &tx2_effects, tx2_total_gas)
        } else {
            (&tx2_effects, tx2_total_gas, &tx1_effects, tx1_total_gas)
        };

    assert!(
        succeeded_effects.status().is_ok(),
        "One transaction should succeed"
    );
    assert!(
        !failed_effects.status().is_ok(),
        "One transaction should fail"
    );
    assert!(
        succeeded_gas > 0,
        "Successful transaction should have gas charged"
    );
    assert_eq!(
        failed_gas, 0,
        "Failed transaction should have zero gas charged"
    );
    let status_str = format!("{:?}", failed_effects.status());
    assert!(
        status_str.contains("InsufficientFundsForWithdraw"),
        "Failed transaction should have InsufficientFundsForWithdraw error: {}",
        status_str
    );

    let successful_tx_gas = succeeded_gas;

    let final_sponsor_balance = test_env.get_sui_balance(sponsor);

    let expected_final_sponsor_balance = sponsor_initial_balance - successful_tx_gas;
    assert_eq!(
        final_sponsor_balance, expected_final_sponsor_balance,
        "Sponsor balance should reflect only the successful transaction"
    );

    let final_sender_balance = test_env.get_sui_balance(sender);

    assert_eq!(
        final_sender_balance, 100_000_000,
        "Sender balance should remain unchanged"
    );

    test_env.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_insufficient_balance_charges_zero_gas() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, gas_for_deposit) = test_env.get_sender_and_gas(0);

    let initial_balance = 30_000_000u64;
    let withdraw_amount = 15_000_000u64;

    let deposit_tx = test_env
        .tx_builder_with_gas(sender, gas_for_deposit)
        .transfer_sui_to_address_balance(
            FundSource::coin(gas_for_deposit),
            vec![(initial_balance, sender)],
        )
        .build();
    test_env.exec_tx_directly(deposit_tx).await.unwrap();
    test_env.verify_accumulator_exists(sender, initial_balance);

    let tx1 = create_withdraw_balance_transaction(
        sender,
        test_env.rgp,
        test_env.chain_id,
        withdraw_amount,
        0,
    );

    let tx2 = create_withdraw_balance_transaction(
        sender,
        test_env.rgp,
        test_env.chain_id,
        withdraw_amount,
        1,
    );

    let tx1_digest = tx1.digest();
    let tx2_digest = tx2.digest();

    assert_ne!(
        tx1_digest, tx2_digest,
        "Transaction digests should be different"
    );

    let mut effects = test_env
        .cluster
        .sign_and_execute_txns_in_soft_bundle(&[tx1, tx2])
        .await
        .unwrap();

    let tx2_effects = effects.pop().unwrap().1;
    let tx1_effects = effects.pop().unwrap().1;

    let tx1_gas = tx1_effects.gas_cost_summary();
    let tx1_total_gas = calculate_total_gas_cost(tx1_gas);
    let tx2_gas = tx2_effects.gas_cost_summary();
    let tx2_total_gas = calculate_total_gas_cost(tx2_gas);

    let (succeeded_effects, succeeded_gas, failed_effects, failed_gas) =
        if tx1_effects.status().is_ok() {
            (&tx1_effects, tx1_total_gas, &tx2_effects, tx2_total_gas)
        } else {
            (&tx2_effects, tx2_total_gas, &tx1_effects, tx1_total_gas)
        };

    assert!(
        succeeded_effects.status().is_ok(),
        "One transaction should succeed"
    );
    assert!(
        !failed_effects.status().is_ok(),
        "One transaction should fail"
    );
    assert!(
        succeeded_gas > 0,
        "Successful transaction should have gas charged"
    );
    assert_eq!(
        failed_gas, 0,
        "Failed transaction should have zero gas charged"
    );
    let status_str = format!("{:?}", failed_effects.status());
    assert!(
        status_str.contains("InsufficientFundsForWithdraw"),
        "Failed transaction should have InsufficientFundsForWithdraw error: {}",
        status_str
    );

    let successful_tx_gas = succeeded_gas;

    test_env
        .cluster
        .wait_for_tx_settlement(&[tx1_digest, tx2_digest])
        .await;

    let final_sender_balance = test_env.get_sui_balance(sender);

    let expected_final_balance = initial_balance - withdraw_amount - successful_tx_gas;
    assert_eq!(
        final_sender_balance, expected_final_balance,
        "Final balance should reflect only the successful transaction"
    );

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_soft_bundle_different_gas_payers() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg
        }))
        .build()
        .await;

    let gas_test_package_id = test_env.setup_test_package(move_test_code_path()).await;

    let (sender1, sender1_gas) = test_env.get_sender_and_gas(0);
    let (sender2, sender2_gas) = test_env.get_sender_and_gas(1);

    let deposit_tx1 = test_env
        .tx_builder_with_gas(sender1, sender1_gas)
        .transfer_sui_to_address_balance(FundSource::coin(sender1_gas), vec![(10_000_000, sender1)])
        .build();
    test_env.exec_tx_directly(deposit_tx1).await.unwrap();

    let deposit_tx2 = test_env
        .tx_builder_with_gas(sender2, sender2_gas)
        .transfer_sui_to_address_balance(FundSource::coin(sender2_gas), vec![(10_000_000, sender2)])
        .build();
    test_env.exec_tx_directly(deposit_tx2).await.unwrap();

    let tx1 = create_storage_test_transaction_address_balance(
        sender1,
        gas_test_package_id,
        test_env.rgp,
        test_env.chain_id,
        None,
        0,
    );

    let tx2 = create_storage_test_transaction_address_balance(
        sender2,
        gas_test_package_id,
        test_env.rgp,
        test_env.chain_id,
        None,
        1,
    );

    let tx1_digest = tx1.digest();
    let tx2_digest = tx2.digest();

    assert_ne!(
        tx1_digest, tx2_digest,
        "Transaction digests should be different"
    );

    let mut effects = test_env
        .cluster
        .sign_and_execute_txns_in_soft_bundle(&[tx1, tx2])
        .await
        .unwrap();

    let tx2_effects = effects.pop().unwrap().1;
    let tx1_effects = effects.pop().unwrap().1;

    assert!(tx1_effects.status().is_ok(), "Transaction 1 should succeed");
    assert!(tx2_effects.status().is_ok(), "Transaction 2 should succeed");

    let acc_events1 = tx1_effects.accumulator_events();
    let acc_events2 = tx2_effects.accumulator_events();

    tracing::info!("Sender1: {:?}", sender1);
    tracing::info!("Sender2: {:?}", sender2);
    tracing::info!("Transaction 1 accumulator events: {:?}", acc_events1);
    tracing::info!("Transaction 2 accumulator events: {:?}", acc_events2);

    let gas_summary1 = tx1_effects.gas_cost_summary();
    let gas_used1 = calculate_total_gas_cost(gas_summary1);
    let gas_summary2 = tx2_effects.gas_cost_summary();
    let gas_used2 = calculate_total_gas_cost(gas_summary2);

    let expected_balance1 = 10_000_000 - gas_used1;
    let expected_balance2 = 10_000_000 - gas_used2;

    test_env
        .cluster
        .wait_for_tx_settlement(&[tx1_digest, tx2_digest])
        .await;

    let actual_balance1 = test_env.get_sui_balance(sender1);
    let actual_balance2 = test_env.get_sui_balance(sender2);

    assert_eq!(
        actual_balance1, expected_balance1,
        "Sender1 balance should be {} after gas deduction, got {}",
        expected_balance1, actual_balance1
    );
    assert_eq!(
        actual_balance2, expected_balance2,
        "Sender2 balance should be {} after gas deduction, got {}",
        expected_balance2, actual_balance2
    );

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_multiple_deposits_merged_in_effects() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.create_root_accumulator_object_for_testing();
            cfg.enable_accumulators_for_testing();
            cfg
        }))
        .with_num_validators(1)
        .build()
        .await;

    let (sender, gas) = test_env.get_sender_and_gas(0);
    let recipient = sender;

    let initial_deposit = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(10000, recipient)])
        .build();
    test_env.exec_tx_directly(initial_deposit).await.unwrap();

    test_env.verify_accumulator_exists(recipient, 10000);

    let mut builder = ProgrammableTransactionBuilder::new();

    let deposit_amounts = vec![1000u64, 2000u64, 3000u64];
    for amount in &deposit_amounts {
        let amount_arg = builder.pure(*amount).unwrap();
        let recipient_arg = builder.pure(recipient).unwrap();
        let coin = builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount_arg]));

        let Argument::Result(coin_idx) = coin else {
            panic!("coin is not a result");
        };

        let coin = Argument::NestedResult(coin_idx, 0);

        builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("coin").unwrap(),
            Identifier::new("send_funds").unwrap(),
            vec!["0x2::sui::SUI".parse().unwrap()],
            vec![coin, recipient_arg],
        );
    }

    let withdraw_amounts = vec![500u64, 1500u64];
    for amount in &withdraw_amounts {
        let withdraw_arg = sui_types::transaction::FundsWithdrawalArg::balance_from_sender(
            *amount,
            sui_types::gas_coin::GAS::type_tag(),
        );
        let withdraw_arg = builder.funds_withdrawal(withdraw_arg).unwrap();

        let amount_arg = builder
            .pure(move_core_types::u256::U256::from(*amount))
            .unwrap();

        let split_withdraw_arg = builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("funds_accumulator").unwrap(),
            Identifier::new("withdrawal_split").unwrap(),
            vec!["0x2::balance::Balance<0x2::sui::SUI>".parse().unwrap()],
            vec![withdraw_arg, amount_arg],
        );

        let coin = builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::new("coin").unwrap(),
            Identifier::new("redeem_funds").unwrap(),
            vec!["0x2::sui::SUI".parse().unwrap()],
            vec![split_withdraw_arg],
        );

        builder.transfer_arg(sender, coin);
    }

    let (_, gas) = test_env.get_sender_and_gas(0);
    let tx = TransactionKind::ProgrammableTransaction(builder.finish());
    let tx_data = TransactionData::new(tx, sender, gas, 10000000, test_env.rgp);

    let (_, effects) = test_env.exec_tx_directly(tx_data).await.unwrap();

    assert!(
        effects.status().is_ok(),
        "Transaction should succeed, got: {:?}",
        effects.status()
    );

    let acc_events = effects.accumulator_events();
    assert_eq!(
        acc_events.len(),
        1,
        "Should have exactly 1 accumulator event (merged), got: {}",
        acc_events.len()
    );

    let event = &acc_events[0];
    assert_eq!(
        event.write.address.address, recipient,
        "Accumulator event should be for the recipient"
    );

    let deposit_total: u64 = deposit_amounts.iter().sum();
    let withdraw_total: u64 = withdraw_amounts.iter().sum();
    let expected_net = deposit_total - withdraw_total;

    match &event.write.value {
        sui_types::effects::AccumulatorValue::Integer(value) => {
            assert_eq!(
                *value, expected_net,
                "Merged accumulator value should be {} (deposits {} - withdrawals {}), got {}",
                expected_net, deposit_total, withdraw_total, value
            );
        }
        _ => panic!("Expected Integer accumulator value"),
    }

    match &event.write.operation {
        sui_types::effects::AccumulatorOperation::Merge => {}
        _ => panic!("Expected Merge operation since deposits > withdrawals"),
    }

    let expected_final_balance = 10000 + expected_net;
    test_env.verify_accumulator_exists(recipient, expected_final_balance);

    test_env.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_address_balance_gas_budget_enforcement_with_storage() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, gas_package_id) = setup_address_balance_account(&mut test_env, 100_000_000).await;

    test_env.verify_accumulator_exists(sender, 100_000_000);

    // First, create a simple object to use as input
    let mut builder1 = ProgrammableTransactionBuilder::new();
    let value1 = builder1.pure(1u64).unwrap();
    builder1.programmable_move_call(
        gas_package_id,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("create_object_with_storage").unwrap(),
        vec![],
        vec![value1],
    );
    let tx_kind1 = TransactionKind::ProgrammableTransaction(builder1.finish());
    let tx1 = create_address_balance_transaction(
        tx_kind1,
        sender,
        5_000_000,
        test_env.rgp,
        test_env.chain_id,
    );
    let (_, effects1) = test_env.exec_tx_directly(tx1).await.unwrap();
    let created_obj = effects1.created()[0].0;

    // Now create a transaction with the created object as input to trigger storage OOG
    let mut builder = ProgrammableTransactionBuilder::new();
    let obj_arg = builder
        .obj(ObjectArg::ImmOrOwnedObject(created_obj))
        .unwrap();

    // Delete the input object (explicitly using it in the transaction)
    builder.programmable_move_call(
        gas_package_id,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("delete_object").unwrap(),
        vec![],
        vec![obj_arg],
    );

    // Create a large object to trigger storage OOG
    let value = builder.pure(42u64).unwrap();
    let large_data = vec![0u8; 200];
    let data_arg = builder.pure(large_data).unwrap();
    builder.programmable_move_call(
        gas_package_id,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("create_object_with_large_storage").unwrap(),
        vec![],
        vec![value, data_arg],
    );
    let tx_kind = TransactionKind::ProgrammableTransaction(builder.finish());

    let low_budget = 1_500_000;
    let tx = create_address_balance_transaction(
        tx_kind,
        sender,
        low_budget,
        test_env.rgp,
        test_env.chain_id,
    );

    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    let gas_summary = effects.gas_cost_summary();
    let computation_cost = gas_summary.computation_cost;
    let total_gas_cost = computation_cost + gas_summary.storage_cost - gas_summary.storage_rebate;

    let (error_kind, _) = effects.status().clone().unwrap_err();
    assert!(
        matches!(
            error_kind,
            sui_types::execution_status::ExecutionFailureStatus::InsufficientGas
        ),
        "Expected InsufficientGas error when storage exceeds budget, got: {:?}",
        error_kind
    );

    assert!(
        computation_cost < low_budget,
        "Computation cost ({}) should be under budget ({}). This ensures we hit storage OOG, not computation OOG.",
        computation_cost,
        low_budget
    );

    assert!(
        gas_summary.storage_cost > 0,
        "When storage OOG with input objects, storage_cost should be non-zero (charging for input objects), got: {}",
        gas_summary.storage_cost
    );

    assert!(
        total_gas_cost <= low_budget,
        "Total gas charged ({}) must not exceed budget ({}). This verifies the fix prevents over-charging.",
        total_gas_cost,
        low_budget
    );

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_address_balance_computation_oog() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, gas_package_id) = setup_address_balance_account(&mut test_env, 100_000_000).await;

    // Create many small objects to trigger computation OOG

    let mut builder = ProgrammableTransactionBuilder::new();
    for _ in 0..100 {
        let value = builder.pure(42u64).unwrap();
        builder.programmable_move_call(
            gas_package_id,
            Identifier::new("gas_test").unwrap(),
            Identifier::new("create_object_with_storage").unwrap(),
            vec![],
            vec![value],
        );
    }
    let tx_kind = TransactionKind::ProgrammableTransaction(builder.finish());

    let low_budget = 1_000_000;
    let tx = create_address_balance_transaction(
        tx_kind,
        sender,
        low_budget,
        test_env.rgp,
        test_env.chain_id,
    );

    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    let gas_summary = effects.gas_cost_summary();
    let computation_cost = gas_summary.computation_cost;

    let (error_kind, _) = effects.status().clone().unwrap_err();
    assert!(
        matches!(
            error_kind,
            sui_types::execution_status::ExecutionFailureStatus::InsufficientGas
        ),
        "Expected InsufficientGas error when computation exceeds budget, got: {:?}",
        error_kind
    );

    assert_eq!(
        computation_cost, low_budget,
        "When computation exceeds budget, should charge full budget as computation: {} vs {}",
        computation_cost, low_budget
    );

    assert_eq!(
        gas_summary.storage_cost, 0,
        "When computation OOG, storage_cost should be 0, got: {}",
        gas_summary.storage_cost
    );

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_address_balance_large_rebate() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, gas_package_id) = setup_address_balance_account(&mut test_env, 100_000_000).await;

    let mut builder = ProgrammableTransactionBuilder::new();
    let value = builder.pure(42u64).unwrap();
    let large_data = vec![0u8; 500];
    let data_arg = builder.pure(large_data).unwrap();
    builder.programmable_move_call(
        gas_package_id,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("create_object_with_large_storage").unwrap(),
        vec![],
        vec![value, data_arg],
    );
    let tx_kind = TransactionKind::ProgrammableTransaction(builder.finish());
    let tx = create_address_balance_transaction(
        tx_kind,
        sender,
        50_000_000,
        test_env.rgp,
        test_env.chain_id,
    );

    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    assert!(
        effects.status().is_ok(),
        "Creating object should succeed, got: {:?}",
        effects.status()
    );

    let created_object_ref = effects
        .created()
        .iter()
        .find(|(obj_ref, _)| obj_ref.0 != effects.gas_object().0.0)
        .map(|(obj_ref, _)| *obj_ref)
        .expect("Should have created an object");

    let initial_balance = test_env.get_sui_balance(sender);

    let mut builder = ProgrammableTransactionBuilder::new();
    let object_arg = builder
        .obj(sui_types::transaction::ObjectArg::ImmOrOwnedObject(
            created_object_ref,
        ))
        .unwrap();
    builder.programmable_move_call(
        gas_package_id,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("delete_object").unwrap(),
        vec![],
        vec![object_arg],
    );
    let tx_kind = TransactionKind::ProgrammableTransaction(builder.finish());
    let tx = create_address_balance_transaction(
        tx_kind,
        sender,
        50_000_000,
        test_env.rgp,
        test_env.chain_id,
    );

    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    assert!(
        effects.status().is_ok(),
        "Deleting object should succeed, got: {:?}",
        effects.status()
    );

    let gas_summary = effects.gas_cost_summary();
    let net_gas = (gas_summary.computation_cost + gas_summary.storage_cost) as i128
        - gas_summary.storage_rebate as i128;

    assert!(
        gas_summary.storage_rebate > gas_summary.storage_cost,
        "Rebate ({}) should exceed new storage cost ({})",
        gas_summary.storage_rebate,
        gas_summary.storage_cost
    );

    assert!(
        net_gas < 0,
        "Net gas ({}) should be negative when rebate exceeds total gas cost",
        net_gas
    );

    let final_balance = test_env.get_sui_balance(sender);

    let expected_balance = (initial_balance as i128 - net_gas) as u64;
    assert_eq!(
        final_balance, expected_balance,
        "Balance change should match net gas ({}): initial {} -> final {} (expected {})",
        net_gas, initial_balance, final_balance, expected_balance
    );

    assert!(
        final_balance > initial_balance,
        "Balance should increase when rebate exceeds costs: {} -> {}",
        initial_balance,
        final_balance
    );

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_sponsored_address_balance_storage_oog() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg
        }))
        .build()
        .await;

    let gas_package_id = test_env.setup_test_package(move_test_code_path()).await;

    let (sender, sender_gas) = test_env.get_sender_and_gas(0);
    let (sponsor, sponsor_gas) = test_env.get_sender_and_gas(1);

    let deposit_tx_sender = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(FundSource::coin(sender_gas), vec![(100_000_000, sender)])
        .build();
    test_env.exec_tx_directly(deposit_tx_sender).await.unwrap();

    let deposit_tx_sponsor = test_env
        .tx_builder(sponsor)
        .transfer_sui_to_address_balance(
            FundSource::coin(sponsor_gas),
            vec![(100_000_000, sponsor)],
        )
        .build();
    test_env.exec_tx_directly(deposit_tx_sponsor).await.unwrap();

    // Create a large object to trigger storage OOG

    let mut builder = ProgrammableTransactionBuilder::new();
    let value = builder.pure(42u64).unwrap();
    let large_data = vec![0u8; 200];
    let data_arg = builder.pure(large_data).unwrap();
    builder.programmable_move_call(
        gas_package_id,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("create_object_with_large_storage").unwrap(),
        vec![],
        vec![value, data_arg],
    );
    let tx_kind = TransactionKind::ProgrammableTransaction(builder.finish());

    let sponsor_budget = 1_500_000;
    let tx = create_address_balance_transaction_with_sponsor(
        tx_kind,
        sender,
        Some(sponsor),
        sponsor_budget,
        test_env.rgp,
        test_env.chain_id,
    );

    let sender_sig = test_env
        .cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sender, &tx, Intent::sui_transaction())
        .await
        .unwrap();
    let sponsor_sig = test_env
        .cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sponsor, &tx, Intent::sui_transaction())
        .await
        .unwrap();

    let signed_tx = Transaction::from_data(tx, vec![sender_sig, sponsor_sig]);
    let (effects, _) = test_env
        .cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await
        .expect("Transaction execution should not panic");

    let gas_summary = effects.gas_cost_summary();
    let total_gas_cost =
        gas_summary.computation_cost + gas_summary.storage_cost - gas_summary.storage_rebate;

    let (error_kind, _) = effects.status().clone().unwrap_err();
    assert!(
        matches!(
            error_kind,
            sui_types::execution_status::ExecutionFailureStatus::InsufficientGas
        ),
        "Expected InsufficientGas error when sponsor's budget exceeded, got: {:?}",
        error_kind
    );

    assert!(
        total_gas_cost <= sponsor_budget,
        "Sponsor charged ({}) must not exceed sponsor's budget ({})",
        total_gas_cost,
        sponsor_budget
    );

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_get_all_balances() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_accumulators_for_testing();
            cfg
        }))
        .with_num_validators(1)
        .build()
        .await;

    let sender = test_env.get_sender(0);

    publish_and_mint_trusted_coin(&mut test_env, sender).await;

    let (_, gas) = test_env.get_sender_and_gas(0);
    // send 1000 gas from the gas coins to ourselves
    let tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(1000, sender)])
        .build();

    test_env.exec_tx_directly(tx).await.unwrap();

    let recipient = SuiAddress::random_for_testing_only();
    // send 1000 gas from the gas coins to the other recipient
    let (_, gas) = test_env.get_sender_and_gas(0);
    let tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(1001, recipient)])
        .build();

    test_env.exec_tx_directly(tx).await.unwrap();

    test_env.cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let indexes = state.indexes.clone().unwrap();
        let child_object_resolver = state.get_child_object_resolver().as_ref();

        let balances = get_all_balances_for_owner(sender, child_object_resolver, &indexes).unwrap();

        assert_eq!(balances.len(), 2);
        assert!(
            balances
                .iter()
                .any(|(t, _)| t.to_canonical_string(true).contains("::sui::SUI"))
        );
        assert!(balances.iter().any(|(t, _)| {
            t.to_canonical_string(true)
                .contains("::trusted_coin::TRUSTED_COIN")
        }));
    });
}

// publishes trusted_coin, mints a coin with balance 1000000, transfers some to the sender's
// address balance, and returns the updated gas object ref
async fn publish_and_mint_trusted_coin(test_env: &mut TestEnv, sender: SuiAddress) {
    let test_tx_builder = test_env.tx_builder(sender);

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "rpc", "data", "trusted_coin"]);
    let coin_publish = test_tx_builder.publish_async(path).await.build();

    let (_, effects) = test_env.exec_tx_directly(coin_publish).await.unwrap();

    // Find the treasury cap object
    let treasury_cap = {
        let mut treasury_cap = None;
        for (obj_ref, owner) in effects.created() {
            if owner.is_address_owned() {
                let object = test_env
                    .cluster
                    .fullnode_handle
                    .sui_node
                    .with_async(
                        |node| async move { node.state().get_object(&obj_ref.0).await.unwrap() },
                    )
                    .await;
                if object.type_().unwrap().name().as_str() == "TreasuryCap" {
                    treasury_cap = Some(obj_ref);
                    break;
                }
            }
        }
        treasury_cap.expect("Treasury cap not found")
    };

    // extract the newly published package id.
    let package_id = effects.published_packages().into_iter().next().unwrap();

    // call my_coin::mint to mint a coin with balance 1000000
    let test_tx_builder = test_env.tx_builder(sender);
    let mint_tx = test_tx_builder
        .move_call(
            package_id,
            "trusted_coin",
            "mint",
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(treasury_cap)),
                CallArg::Pure(bcs::to_bytes(&1000000u64).unwrap()),
            ],
        )
        .build();
    let (_, mint_effects) = test_env.exec_tx_directly(mint_tx).await.unwrap();

    // the trusted coin is the only address-owned object created.
    let trusted_coin_ref = mint_effects
        .created()
        .iter()
        .find(|(_, owner)| owner.is_address_owned())
        .unwrap()
        .0;

    let send_tx = test_env
        .tx_builder(sender)
        .transfer_funds_to_address_balance(
            FundSource::Coin(trusted_coin_ref),
            vec![(1000, sender)],
            format!("{}::trusted_coin::TRUSTED_COIN", package_id)
                .parse()
                .unwrap(),
        )
        .build();
    let (_, send_effects) = test_env.exec_tx_directly(send_tx).await.unwrap();
    assert!(
        send_effects.status().is_ok(),
        "Transaction should succeed, got: {:?}",
        send_effects.status()
    );
}

#[sim_test]
async fn test_reject_transaction_executed_in_previous_epoch() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg.enable_accumulators_for_testing();
            cfg.enable_multi_epoch_transaction_expiration_for_testing();
            cfg
        }))
        .with_num_validators(1)
        .build()
        .await;

    let (sender, gas_objects) = test_env.get_sender_and_all_gas(0);

    let current_epoch = test_env
        .cluster
        .fullnode_handle
        .sui_node
        .with(|node| node.state().epoch_store_for_testing().epoch());

    let gas_test_package_id = test_env.setup_test_package(move_test_code_path()).await;

    let fund_tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(
            FundSource::coin(gas_objects[1]),
            vec![(100_000_000, sender)],
        )
        .build();
    test_env.exec_tx_directly(fund_tx).await.unwrap();

    let tx = create_address_balance_transaction_with_expiration(
        sender,
        test_env.rgp,
        Some(current_epoch),
        Some(current_epoch + 1),
        test_env.chain_id,
        100,
        gas_test_package_id,
    );

    let (tx_digest, _) = test_env.exec_tx_directly(tx.clone()).await.unwrap();

    test_env.trigger_reconfiguration().await;

    for handle in test_env.cluster.all_node_handles() {
        handle.with(|node| {
            node.state()
                .database_for_testing()
                .remove_executed_effects_for_testing(&tx_digest)
                .unwrap();
            node.state()
                .cache_for_testing()
                .evict_executed_effects_from_cache_for_testing(&tx_digest);
        });
    }

    let result = test_env.exec_tx_directly(tx).await;

    match result {
        Err(e) => {
            let err_str = e.to_string();
            assert!(
                err_str.contains("was already executed"),
                "Expected 'was already executed' error, got: {}",
                err_str
            );
        }
        Ok(_) => {
            panic!(
                "Transaction should be rejected as already executed in previous epoch, but submission succeeded"
            );
        }
    }
}

#[sim_test]
async fn test_transaction_executes_in_next_epoch_with_one_epoch_range() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg.enable_accumulators_for_testing();
            cfg.enable_multi_epoch_transaction_expiration_for_testing();
            cfg
        }))
        .with_num_validators(1)
        .build()
        .await;

    let (sender, gas_objects) = test_env.get_sender_and_all_gas(0);

    let current_epoch = test_env
        .cluster
        .fullnode_handle
        .sui_node
        .with(|node| node.state().epoch_store_for_testing().epoch());

    let gas_test_package_id = test_env.setup_test_package(move_test_code_path()).await;

    let fund_tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(
            FundSource::coin(gas_objects[1]),
            vec![(100_000_000, sender)],
        )
        .build();
    test_env.exec_tx_directly(fund_tx).await.unwrap();

    let tx = create_address_balance_transaction_with_expiration(
        sender,
        test_env.rgp,
        Some(current_epoch),
        Some(current_epoch + 1),
        test_env.chain_id,
        100,
        gas_test_package_id,
    );

    test_env.trigger_reconfiguration().await;

    let response = test_env.exec_tx_directly(tx).await;

    assert!(
        response.is_ok(),
        "Transaction with 1-epoch range should execute successfully in next epoch"
    );
}

#[sim_test]
async fn test_reject_signing_transaction_executed_in_previous_epoch() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg.enable_accumulators_for_testing();
            cfg.enable_multi_epoch_transaction_expiration_for_testing();
            cfg
        }))
        .with_num_validators(1)
        .build()
        .await;

    let (sender, gas_objects) = test_env.get_sender_and_all_gas(0);

    let current_epoch = test_env
        .cluster
        .fullnode_handle
        .sui_node
        .with(|node| node.state().epoch_store_for_testing().epoch());

    let gas_test_package_id = test_env.setup_test_package(move_test_code_path()).await;

    let fund_tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(
            FundSource::coin(gas_objects[1]),
            vec![(100_000_000, sender)],
        )
        .build();
    test_env.exec_tx_directly(fund_tx).await.unwrap();

    let tx = create_address_balance_transaction_with_expiration(
        sender,
        test_env.rgp,
        Some(current_epoch),
        Some(current_epoch + 1),
        test_env.chain_id,
        100,
        gas_test_package_id,
    );
    let signed_tx = test_env.cluster.sign_transaction(&tx).await;

    test_env.exec_tx_directly(tx.clone()).await.unwrap();

    test_env.trigger_reconfiguration().await;

    let result = test_env.cluster.swarm.validator_node_handles()[0]
        .with_async(|node| async move {
            let epoch_store = node.state().epoch_store_for_testing();
            let verified_tx = VerifiedTransaction::new_unchecked(signed_tx);
            node.state()
                .handle_vote_transaction(&epoch_store, verified_tx)
        })
        .await;

    match result {
        Err(e) => {
            let err_str = e.to_string();
            assert!(
                err_str.contains("already been executed"),
                "Expected 'already been executed' error when voting on transaction that was executed in previous epoch, got: {}",
                err_str
            );
        }
        Ok(()) => {
            panic!(
                "Expected handle_vote_transaction to fail for transaction executed in previous epoch"
            );
        }
    }
}

#[sim_test]
async fn address_balance_stress_test() {
    telemetry_subscribers::init_for_testing();
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.enable_accumulators_for_testing();
        cfg
    });

    let test_cluster = Arc::new(
        TestClusterBuilder::new()
            .with_epoch_duration_ms(10000)
            .with_num_validators(7)
            .build()
            .await,
    );

    let addresses: Vec<_> = test_cluster
        .wallet
        .get_addresses()
        .into_iter()
        .take(4)
        .collect();
    assert!(
        addresses.len() == 4,
        "Need 4 addresses, got {}",
        addresses.len()
    );

    // Publish the coins package - creates 5 coin types with 10000 each in sender's address balance
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "data", "coins"]);
    let publish_tx = test_cluster
        .test_transaction_builder()
        .await
        .publish_async(path)
        .await
        .build();
    let publisher = publish_tx.sender();
    let response = test_cluster.sign_and_execute_transaction(&publish_tx).await;
    let package_id = response.get_new_package_obj().unwrap().0;
    let coin_a_type: TypeTag = format!("{}::coin_a::COIN_A", package_id).parse().unwrap();
    let coin_b_type: TypeTag = format!("{}::coin_b::COIN_B", package_id).parse().unwrap();
    let coin_c_type: TypeTag = format!("{}::coin_c::COIN_C", package_id).parse().unwrap();
    let coin_d_type: TypeTag = format!("{}::coin_d::COIN_D", package_id).parse().unwrap();
    let coin_e_type: TypeTag = format!("{}::coin_e::COIN_E", package_id).parse().unwrap();

    let coin_types = vec![
        coin_a_type,
        coin_b_type,
        coin_c_type,
        coin_d_type,
        coin_e_type,
    ];

    // Distribute coins from publisher to all 4 addresses (2500 each per coin type)
    let mut builder = test_cluster
        .test_transaction_builder_with_sender(publisher)
        .await;
    for coin_type in &coin_types {
        let recipients: Vec<(u64, SuiAddress)> =
            addresses.iter().map(|addr| (2500u64, *addr)).collect();
        builder = builder.transfer_funds_to_address_balance(
            FundSource::address_fund_with_reservation(10000),
            recipients,
            coin_type.clone(),
        );
    }
    test_cluster
        .sign_and_execute_transaction(&builder.build())
        .await;

    // Collect gas objects for each address (need 2 per address for 2 concurrent tasks)
    let mut address_gas: Vec<Vec<ObjectRef>> = Vec::new();
    for addr in &addresses {
        let gas_objs = test_cluster
            .wallet
            .gas_objects(*addr)
            .await
            .unwrap()
            .into_iter()
            .map(|(_, obj)| obj.compute_object_reference())
            .collect::<Vec<_>>();
        assert!(
            gas_objs.len() >= 2,
            "Need at least 2 gas objects for address {:?}, got {}",
            addr,
            gas_objs.len()
        );
        address_gas.push(gas_objs);
    }

    // Counters for success/failure tracking
    let success_count = Arc::new(AtomicU64::new(0));
    let exec_failure_count = Arc::new(AtomicU64::new(0));
    let signing_failure_count = Arc::new(AtomicU64::new(0));

    // Spawn 8 tasks (2 per address)
    let mut handles = Vec::new();
    for addr in addresses.iter().cloned() {
        let gas_objects = test_cluster
            .wallet
            .get_gas_objects_owned_by_address(addr, Some(2))
            .await
            .unwrap();
        assert_eq!(gas_objects.len(), 2);
        for gas_obj in gas_objects {
            let test_cluster = Arc::clone(&test_cluster);
            let all_addresses = addresses.clone();
            let coin_types = coin_types.clone();
            let success_count = Arc::clone(&success_count);
            let exec_failure_count = Arc::clone(&exec_failure_count);
            let signing_failure_count = Arc::clone(&signing_failure_count);

            let handle = tokio::spawn(async move {
                let start_time = std::time::Instant::now();
                let duration = std::time::Duration::from_secs(60);
                let mut current_gas = gas_obj;

                while start_time.elapsed() < duration {
                    let (selected_types, recipients) = {
                        let mut rng = rand::thread_rng();

                        // Pick 1-5 random coin types
                        let num_coin_types = rng.gen_range(1..=5);
                        let mut selected_types = coin_types.clone();
                        selected_types.shuffle(&mut rng);
                        selected_types.truncate(num_coin_types);

                        // Pick random recipient(s) - different from self
                        let other_addresses: Vec<SuiAddress> = all_addresses
                            .iter()
                            .filter(|a| **a != addr)
                            .copied()
                            .collect();
                        let num_recipients = rng.gen_range(1..=other_addresses.len());
                        let mut recipients = other_addresses.clone();
                        recipients.shuffle(&mut rng);
                        recipients.truncate(num_recipients);

                        (selected_types, recipients)
                    };

                    // Build transaction with random amounts
                    // Use amounts that sometimes exceed balance to trigger failures
                    // Each address has 2500 per coin type, so use amounts from 100 to 1500
                    let mut builder = test_cluster
                        .test_transaction_builder_with_gas_object(addr, current_gas)
                        .await;
                    let mut total_reservation_per_type = std::collections::HashMap::new();

                    for coin_type in &selected_types {
                        let mut transfers = Vec::new();
                        for recipient in &recipients {
                            // Amount between 100 and 1500 - sometimes will exceed balance
                            let amount = rand::thread_rng().gen_range(100..=1500);
                            transfers.push((amount, *recipient));
                            *total_reservation_per_type
                                .entry(coin_type.clone())
                                .or_insert(0u64) += amount;
                        }

                        let reservation = *total_reservation_per_type.get(coin_type).unwrap();
                        builder = builder.transfer_funds_to_address_balance(
                            FundSource::address_fund_with_reservation(reservation),
                            transfers,
                            coin_type.clone(),
                        );
                    }

                    let tx = builder.build();
                    let signed_tx = test_cluster.sign_transaction(&tx).await;
                    match test_cluster
                        .wallet
                        .execute_transaction_may_fail(signed_tx)
                        .await
                    {
                        Ok(response) => {
                            let effects = response.effects;
                            if effects.status().is_ok() {
                                success_count.fetch_add(1, Ordering::Relaxed);
                            } else {
                                exec_failure_count.fetch_add(1, Ordering::Relaxed);
                            }
                            current_gas = effects.gas_object().0;
                        }
                        Err(err) => {
                            let err_str = err.to_string();
                            if err_str.contains("Available amount in account for object id") {
                                signing_failure_count.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                }
            });
            handles.push(handle);
        }
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.unwrap();
    }

    let final_success = success_count.load(Ordering::Relaxed);
    let final_exec_failure = exec_failure_count.load(Ordering::Relaxed);
    let final_signing_failure = signing_failure_count.load(Ordering::Relaxed);

    tracing::info!(
        "Stress test completed: {} successes, {} execution failures, {} signing failures",
        final_success,
        final_exec_failure,
        final_signing_failure
    );

    // Ensure we had both successes and failures (the random amounts should cause some failures)
    assert!(
        final_success > 0,
        "Expected at least some successful transactions, got {}",
        final_success
    );
    assert!(
        final_exec_failure > 0,
        "Expected at least some execution failed transactions (due to insufficient balance), got {}",
        final_exec_failure
    );
    assert!(
        final_signing_failure > 0,
        "Expected at least some signing failed transactions (due to insufficient balance), got {}",
        final_signing_failure
    );

    // Verify total transactions is reasonable (should have executed many in 30 seconds)
    let total = final_success + final_exec_failure + final_signing_failure;
    assert!(
        total >= 10,
        "Expected at least 10 total transactions, got {}",
        total
    );
}

#[sim_test]
async fn test_address_balance_gas_merge_accumulator_events() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg
        }))
        .with_num_validators(1)
        .build()
        .await;

    let (sender, gas_objects) = test_env.get_sender_and_all_gas(0);

    let initial_balance = 100_000_000u64;
    let deposit_tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(
            FundSource::coin(gas_objects[0]),
            vec![(initial_balance, sender)],
        )
        .build();
    test_env.exec_tx_directly(deposit_tx).await.unwrap();

    test_env.verify_accumulator_exists(sender, initial_balance);

    let (_, gas) = test_env.get_sender_and_gas(0);

    let send_to_self_amount = 5_000_000u64;

    let mut builder = ProgrammableTransactionBuilder::new();
    let coin_input = builder.obj(ObjectArg::ImmOrOwnedObject(gas)).unwrap();
    let amount_arg = builder.pure(send_to_self_amount).unwrap();
    let recipient_arg = builder.pure(sender).unwrap();
    let coin = builder.command(Command::SplitCoins(coin_input, vec![amount_arg]));
    let Argument::Result(coin_idx) = coin else {
        panic!("coin is not a result");
    };
    let coin = Argument::NestedResult(coin_idx, 0);
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![coin, recipient_arg],
    );

    let tx_kind = TransactionKind::ProgrammableTransaction(builder.finish());
    let budget = 10_000_000u64;
    let tx_data = create_address_balance_transaction(
        tx_kind,
        sender,
        budget,
        test_env.rgp,
        test_env.chain_id,
    );

    let (_, effects) = test_env.exec_tx_directly(tx_data).await.unwrap();

    assert!(effects.status().is_ok());

    let gas_cost = effects.gas_cost_summary().net_gas_usage();
    let acc_events = effects.accumulator_events();

    assert_eq!(acc_events.len(), 1);

    let expected_net = send_to_self_amount as i64 - gas_cost;
    match &acc_events[0].write.value {
        sui_types::effects::AccumulatorValue::Integer(value) => {
            assert_eq!(*value, expected_net.unsigned_abs());
        }
        _ => panic!("Expected Integer accumulator value"),
    }

    let expected_final_balance = initial_balance as i64 + expected_net;
    test_env.verify_accumulator_exists(sender, expected_final_balance as u64);

    test_env.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_funds_withdraw_scheduler_type_alternation() {
    use sui_config::node::FundsWithdrawSchedulerType;

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(4)
        .build()
        .await;

    let validator_configs = test_cluster.swarm.config().validator_configs();
    assert_eq!(validator_configs.len(), 4);

    // Verify alternating scheduler types: even indices use Eager, odd indices use Naive
    for (idx, config) in validator_configs.iter().enumerate() {
        let expected_type = if idx % 2 == 0 {
            FundsWithdrawSchedulerType::Eager
        } else {
            FundsWithdrawSchedulerType::Naive
        };
        assert_eq!(
            config.funds_withdraw_scheduler_type, expected_type,
            "Validator {} has unexpected scheduler type: {:?}, expected {:?}",
            idx, config.funds_withdraw_scheduler_type, expected_type
        );
    }
}
