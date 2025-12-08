// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{identifier::Identifier, u256::U256};
use shared_crypto::intent::Intent;
use std::path::PathBuf;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_keys::keystore::AccountKeystore;
use sui_macros::*;
use sui_protocol_config::{ProtocolConfig, ProtocolVersion};
use sui_sdk::wallet_context::WalletContext;
use sui_test_transaction_builder::{FundSource, TestTransactionBuilder};
use sui_types::{
    SUI_ACCUMULATOR_ROOT_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID,
    accumulator_metadata::AccumulatorOwner,
    accumulator_root::{AccumulatorValue, U128},
    balance::Balance,
    base_types::{ObjectID, ObjectRef, SuiAddress, dbg_addr},
    digests::{ChainIdentifier, CheckpointDigest},
    effects::{InputConsensusObject, TransactionEffectsAPI},
    gas::GasCostSummary,
    gas_coin::GAS,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    storage::ChildObjectResolver,
    supported_protocol_versions::SupportedProtocolVersions,
    transaction::{
        Argument, Command, FundsWithdrawalArg, GasData, ObjectArg, Transaction, TransactionData,
        TransactionDataAPI, TransactionDataV1, TransactionExpiration, TransactionKind,
    },
};
use test_cluster::{TestCluster, TestClusterBuilder};

async fn get_sender_and_all_gas(context: &mut WalletContext) -> (SuiAddress, Vec<ObjectRef>) {
    let sender = context
        .config
        .keystore
        .addresses()
        .first()
        .cloned()
        .unwrap();

    let gas = context
        .gas_objects(sender)
        .await
        .unwrap()
        .into_iter()
        .map(|(_, obj)| obj.object_ref())
        .collect();

    (sender, gas)
}

async fn get_sender_and_one_gas(context: &mut WalletContext) -> (SuiAddress, ObjectRef) {
    let (sender, gas) = get_sender_and_all_gas(context).await;
    (sender, gas.into_iter().next().unwrap())
}

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
            min_timestamp_seconds: None,
            max_timestamp_seconds: None,
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
    let _guard = ProtocolConfig::apply_overrides_for_testing(|version, mut cfg| {
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
    });

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        // We start at MAX - 1 to be sure the root object isn't created in genesis, and then
        // immediately reconfigure.
        .with_protocol_version(ProtocolVersion::MAX - 1)
        .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
            ProtocolVersion::MAX.as_u64() - 1,
            ProtocolVersion::MAX_ALLOWED.as_u64(),
        ))
        .build()
        .await;

    test_cluster.trigger_reconfiguration().await;

    // accumulator root is not created yet.
    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        assert!(
            !state
                .load_epoch_store_one_call_per_task()
                .accumulator_root_exists()
        );
    });

    test_cluster.trigger_reconfiguration().await;

    // accumulator root was created at the end of previous epoch,
    // but we didn't upgrade to the next protocol version yet.
    test_cluster.fullnode_handle.sui_node.with(|node| {
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
    test_cluster.trigger_reconfiguration().await;

    test_cluster.fullnode_handle.sui_node.with(|node| {
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
    let _guard = ProtocolConfig::apply_overrides_for_testing(|version, mut cfg| {
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
    });

    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        // We start at MAX - 1 to be sure the root object isn't created in genesis, and then
        // immediately reconfigure.
        .with_protocol_version(ProtocolVersion::MAX - 1)
        .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
            ProtocolVersion::MAX.as_u64() - 1,
            ProtocolVersion::MAX_ALLOWED.as_u64(),
        ))
        .build()
        .await;
    test_cluster.trigger_reconfiguration().await;
    let rgp = test_cluster.get_reference_gas_price().await;

    let (sender, gas) = get_sender_and_one_gas(&mut test_cluster.wallet).await;

    let recipient = SuiAddress::random_for_testing_only();

    // Withdraw must be rejected at signing.
    let withdraw_tx = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(1000),
            vec![(1000, dbg_addr(2))],
        )
        .build();
    let withdraw_tx = test_cluster.wallet.sign_transaction(&withdraw_tx).await;
    test_cluster
        .wallet
        .execute_transaction_may_fail(withdraw_tx)
        .await
        .unwrap_err();

    // Transfer fails at execution time
    let tx = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(1000, recipient)])
        .build();

    let signed = test_cluster.wallet.sign_transaction(&tx).await;
    let effects = test_cluster
        .wallet
        .execute_transaction_may_fail(signed)
        .await
        .unwrap()
        .effects
        .unwrap();
    let gas = effects.gas_object().reference.to_object_ref();
    let status = effects.status().clone();
    assert!(status.is_err());

    // we reconfigure, and create the accumulator root at the end of this epoch.
    // but because the root did not exist during this epoch, we don't upgrade to
    // the next protocol version yet.
    test_cluster.trigger_reconfiguration().await;

    // Withdraw must still be rejected at signing.
    let withdraw_tx = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(1000),
            vec![(1000, dbg_addr(2))],
        )
        .build();
    let withdraw_tx = test_cluster.wallet.sign_transaction(&withdraw_tx).await;
    test_cluster
        .wallet
        .execute_transaction_may_fail(withdraw_tx)
        .await
        .unwrap_err();

    // transfer fails at execution time
    let tx = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(1000, recipient)])
        .build();

    let signed = test_cluster.wallet.sign_transaction(&tx).await;
    let effects = test_cluster
        .wallet
        .execute_transaction_may_fail(signed)
        .await
        .unwrap()
        .effects
        .unwrap();
    let gas = effects.gas_object().reference.to_object_ref();
    let status = effects.status().clone();
    assert!(status.is_err());

    // after one more reconfig, we can upgrade to the next protocol version.
    test_cluster.trigger_reconfiguration().await;

    let tx = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(1000, sender)])
        .build();

    let gas = test_cluster
        .sign_and_execute_transaction(&tx)
        .await
        .effects
        .unwrap()
        .gas_object()
        .reference
        .to_object_ref();

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        verify_accumulator_exists(child_object_resolver, sender, 1000);
    });

    // Withdraw can succeed now
    let withdraw_tx = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(1000),
            vec![(1000, dbg_addr(2))],
        )
        .build();
    test_cluster
        .sign_and_execute_transaction(&withdraw_tx)
        .await;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();

        let sui_coin_type = Balance::type_tag(GAS::type_tag());
        assert!(
            !AccumulatorValue::exists(child_object_resolver, None, sender, &sui_coin_type).unwrap(),
            "Accumulator value should have been removed"
        );
    });

    // ensure that no conservation failures are detected during reconfig.
    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_deposits() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.create_root_accumulator_object_for_testing();
        cfg.enable_accumulators_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let context = &mut test_cluster.wallet;

    let (sender, gas) = get_sender_and_one_gas(context).await;

    let recipient = SuiAddress::random_for_testing_only();

    let tx = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(1000, recipient)])
        .build();

    let res = test_cluster.sign_and_execute_transaction(&tx).await;
    let gas = res.effects.unwrap().gas_object().reference.to_object_ref();

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let tx = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(1000, recipient)])
        .build();

    test_cluster.sign_and_execute_transaction(&tx).await;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        verify_accumulator_exists(child_object_resolver, recipient, 2000);

        // Ensure that the accumulator root object is considered a read-only InputConsensusObject
        // by the settlement transaction. This is necessary so that causal sorting in CheckpointBuilder
        // orders barriers after settlements.
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

    // ensure that no conservation failures are detected during reconfig.
    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_multiple_settlement_txns() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.enable_accumulators_for_testing();
        cfg.set_max_updates_per_settlement_txn_for_testing(3);
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let context = &mut test_cluster.wallet;

    let (sender, gas) = get_sender_and_one_gas(context).await;

    let recipient = SuiAddress::random_for_testing_only();

    let amounts_and_recipients = (0..20)
        .map(|_| (1u64, SuiAddress::random_for_testing_only()))
        .collect::<Vec<_>>();

    let tx = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui_to_address_balance(FundSource::coin(gas), amounts_and_recipients.clone())
        .build();

    let res = test_cluster.sign_and_execute_transaction(&tx).await;
    let gas = res.effects.unwrap().gas_object().reference.to_object_ref();

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let tx = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(1000, recipient)])
        .build();

    test_cluster.sign_and_execute_transaction(&tx).await;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();

        for (amount, recipient) in amounts_and_recipients {
            verify_accumulator_exists(child_object_resolver, recipient, amount);
        }
    });

    // ensure that no conservation failures are detected during reconfig.
    test_cluster.trigger_reconfiguration().await;
}

fn get_balance(child_object_resolver: &dyn ChildObjectResolver, owner: SuiAddress) -> u64 {
    let sui_coin_type = Balance::type_tag(GAS::type_tag());
    let accumulator_value =
        AccumulatorValue::load(child_object_resolver, None, owner, &sui_coin_type)
            .expect("read cannot fail");
    match accumulator_value {
        Some(AccumulatorValue::U128(u128_val)) => u128_val.value as u64,
        None => 0,
    }
}

fn verify_accumulator_exists(
    child_object_resolver: &dyn ChildObjectResolver,
    owner: SuiAddress,
    expected_balance: u64,
) {
    let sui_coin_type = Balance::type_tag(GAS::type_tag());

    assert!(
        AccumulatorValue::exists(child_object_resolver, None, owner, &sui_coin_type).unwrap(),
        "Accumulator value should have been created"
    );

    let accumulator_object =
        AccumulatorValue::load_object(child_object_resolver, None, owner, &sui_coin_type)
            .expect("read cannot fail")
            .expect("accumulator should exist");

    assert!(
        accumulator_object
            .data
            .try_as_move()
            .unwrap()
            .type_()
            .is_efficient_representation()
    );

    let accumulator_value =
        AccumulatorValue::load(child_object_resolver, None, owner, &sui_coin_type)
            .expect("read cannot fail")
            .expect("accumulator should exist");

    assert_eq!(
        accumulator_value,
        AccumulatorValue::U128(U128 {
            value: expected_balance as u128
        }),
        "Accumulator value should be {expected_balance}"
    );

    assert!(
        AccumulatorOwner::exists(child_object_resolver, None, owner).unwrap(),
        "Owner object should have been created"
    );

    let owner = AccumulatorOwner::load(child_object_resolver, None, owner)
        .expect("read cannot fail")
        .expect("owner must exist");

    assert!(
        owner
            .metadata_exists(child_object_resolver, None, &sui_coin_type)
            .unwrap(),
        "Metadata object should have been created"
    );

    let _metadata = owner
        .load_metadata(child_object_resolver, None, &sui_coin_type)
        .unwrap();
}

#[sim_test]
async fn test_deposit_and_withdraw() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.create_root_accumulator_object_for_testing();
        cfg.enable_accumulators_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let context = &mut test_cluster.wallet;

    let (sender, gas) = get_sender_and_one_gas(context).await;

    let tx = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(1000, sender)])
        .build();
    let res = test_cluster.sign_and_execute_transaction(&tx).await;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        verify_accumulator_exists(child_object_resolver, sender, 1000);
    });

    let gas = res.effects.unwrap().gas_object().reference.to_object_ref();

    let tx = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(1000),
            vec![(1000, dbg_addr(2))],
        )
        .build();
    test_cluster.sign_and_execute_transaction(&tx).await;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        let sui_coin_type = Balance::type_tag(GAS::type_tag());

        assert!(
            !AccumulatorValue::exists(child_object_resolver, None, sender, &sui_coin_type).unwrap(),
            "Accumulator value should have been removed"
        );
        assert!(
            !AccumulatorOwner::exists(child_object_resolver, None, sender).unwrap(),
            "Owner object should have been removed"
        );
    });

    // ensure that no conservation failures are detected during reconfig.
    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_deposit_and_withdraw_with_larger_reservation() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.create_root_accumulator_object_for_testing();
        cfg.enable_accumulators_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let context = &mut test_cluster.wallet;

    let (sender, gas) = get_sender_and_one_gas(context).await;

    let tx = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(1000, sender)])
        .build();
    let res = test_cluster.sign_and_execute_transaction(&tx).await;
    let gas = res.effects.unwrap().gas_object().reference.to_object_ref();

    // Withdraw 800 with a reservation of 1000 (larger than actual withdrawal)
    let tx = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(1000),
            vec![(800, dbg_addr(2))],
        )
        .build();
    test_cluster.sign_and_execute_transaction(&tx).await;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        // Verify that the accumulator still exists, as the entire balance was not withdrawn
        verify_accumulator_exists(child_object_resolver, sender, 200);
    });

    // ensure that no conservation failures are detected during reconfig.
    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_withdraw_non_existent_balance() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.create_root_accumulator_object_for_testing();
        cfg.enable_accumulators_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let context = &mut test_cluster.wallet;

    let (sender, gas) = get_sender_and_one_gas(context).await;

    // Settlement transaction fails with EInvalidSplitAmount because
    let tx = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(1000),
            vec![(1000, dbg_addr(2))],
        )
        .build();
    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let err = test_cluster
        .wallet
        .execute_transaction_may_fail(signed_tx)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("is less than requested"));
}

#[sim_test]
async fn test_withdraw_insufficient_balance() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.create_root_accumulator_object_for_testing();
        cfg.enable_accumulators_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let context = &mut test_cluster.wallet;

    let (sender, mut gas) = get_sender_and_all_gas(context).await;

    let gas1 = gas.pop().unwrap();
    let gas2 = gas.pop().unwrap();

    // send 1000 from our gas coin to our balance
    let tx = TestTransactionBuilder::new(sender, gas1, rgp)
        .transfer_sui_to_address_balance(FundSource::coin(gas1), vec![(1000, sender)])
        .build();
    let res = test_cluster.sign_and_execute_transaction(&tx).await;
    let gas1 = res.effects.unwrap().gas_object().reference.to_object_ref();

    // Try to withdraw 1001 from balance
    // Transaction fails at signing time
    let tx = TestTransactionBuilder::new(sender, gas1, rgp)
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(1001),
            vec![(1001, dbg_addr(2))],
        )
        .build();
    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let err = test_cluster
        .wallet
        .execute_transaction_may_fail(signed_tx)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("is less than requested"));

    // Now exceed the balance with two transactions, each of which can be certified
    // The second one will fail at execution time
    let tx1 = TestTransactionBuilder::new(sender, gas1, rgp)
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(500),
            vec![(500, dbg_addr(2))],
        )
        .build();
    let tx2 = TestTransactionBuilder::new(sender, gas2, rgp)
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(501),
            vec![(501, dbg_addr(2))],
        )
        .build();

    let mut effects = test_cluster
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

    // ensure that no conservation failures are detected during reconfig.
    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_address_balance_gas() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.enable_address_balance_gas_payments_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let (sender, gas_package_id) =
        setup_address_balance_account(&mut test_cluster, 10_000_000).await;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        verify_accumulator_exists(child_object_resolver, sender, 10_000_000);
    });

    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();

    // check that a transaction with a zero gas budget is rejected
    {
        let mut tx = create_storage_test_transaction_address_balance(
            sender,
            gas_package_id,
            rgp,
            chain_id,
            None,
            0,
        );
        tx.gas_data_mut().budget = 0;
        let signed_tx = test_cluster.sign_transaction(&tx).await;
        let err = test_cluster
            .wallet
            .execute_transaction_may_fail(signed_tx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Gas budget: 0 is lower than min:"));
    }

    let tx = create_storage_test_transaction_address_balance(
        sender,
        gas_package_id,
        rgp,
        chain_id,
        None,
        0,
    );

    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let (effects, _) = test_cluster
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

    let expected_balance = 10_000_000 - gas_used;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        verify_accumulator_exists(child_object_resolver, sender, expected_balance);
    });

    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_sponsored_address_balance_storage_rebates() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.enable_address_balance_gas_payments_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;

    let addresses = test_cluster.wallet.get_addresses();
    let sender = addresses[0];
    let sponsor = addresses[1];

    let chain_id = test_cluster.get_chain_identifier();
    let gas_test_package_id = setup_test_package(&mut test_cluster.wallet).await;
    let rgp = test_cluster.wallet.get_reference_gas_price().await.unwrap();

    let sender_gas = test_cluster
        .wallet
        .gas_objects(sender)
        .await
        .unwrap()
        .pop()
        .unwrap()
        .1
        .object_ref();
    let deposit_tx_sender = TestTransactionBuilder::new(sender, sender_gas, rgp)
        .transfer_sui_to_address_balance(FundSource::coin(sender_gas), vec![(100_000_000, sender)])
        .build();
    test_cluster
        .sign_and_execute_transaction(&deposit_tx_sender)
        .await;

    let sponsor_gas = test_cluster
        .wallet
        .gas_objects(sponsor)
        .await
        .unwrap()
        .pop()
        .unwrap()
        .1
        .object_ref();
    let deposit_tx_sponsor = TestTransactionBuilder::new(sponsor, sponsor_gas, rgp)
        .transfer_sui_to_address_balance(
            FundSource::coin(sponsor_gas),
            vec![(100_000_000, sponsor)],
        )
        .build();
    test_cluster
        .sign_and_execute_transaction(&deposit_tx_sponsor)
        .await;

    let create_txn = create_storage_test_transaction_address_balance(
        sender,
        gas_test_package_id,
        rgp,
        chain_id,
        Some(sponsor),
        0,
    );

    let sender_sig = test_cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sender, &create_txn, Intent::sui_transaction())
        .await
        .unwrap();
    let sponsor_sig = test_cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sponsor, &create_txn, Intent::sui_transaction())
        .await
        .unwrap();

    let signed_create_txn = Transaction::from_data(create_txn, vec![sender_sig, sponsor_sig]);
    let create_resp = test_cluster.execute_transaction(signed_create_txn).await;
    let create_effects = create_resp.effects.as_ref().unwrap();

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

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();

        let sponsor_actual = get_balance(child_object_resolver, sponsor);
        let sender_actual = get_balance(child_object_resolver, sender);

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
    });

    let created_objects: Vec<_> = create_effects.created().iter().collect();
    assert_eq!(
        created_objects.len(),
        1,
        "Should have created exactly one object"
    );
    let created_obj = created_objects[0].reference.to_object_ref();
    let delete_txn = create_delete_transaction_address_balance(
        sender,
        gas_test_package_id,
        created_obj,
        rgp,
        chain_id,
        Some(sponsor),
    );

    let sender_delete_sig = test_cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sender, &delete_txn, Intent::sui_transaction())
        .await
        .unwrap();
    let sponsor_delete_sig = test_cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sponsor, &delete_txn, Intent::sui_transaction())
        .await
        .unwrap();

    let signed_delete_txn =
        Transaction::from_data(delete_txn, vec![sender_delete_sig, sponsor_delete_sig]);
    let delete_resp = test_cluster.execute_transaction(signed_delete_txn).await;
    let delete_effects = delete_resp.effects.as_ref().unwrap();

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

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();

        let sponsor_final = get_balance(child_object_resolver, sponsor);
        let sender_final = get_balance(child_object_resolver, sender);

        assert_eq!(
            sender_final, 100_000_000,
            "Sender balance should remain unchanged at 100_000_000"
        );
        assert_ne!(
            sponsor_final, 100_000_000,
            "Sponsor balance should have changed from 100_000_000"
        );
    });

    test_cluster.trigger_reconfiguration().await;
}

async fn setup_test_package(context: &mut WalletContext) -> ObjectID {
    let mut move_test_code_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    move_test_code_path.push("tests/move_test_code");

    let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
    let gas_price = context.get_reference_gas_price().await.unwrap();
    let txn = context
        .sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, gas_price)
                .publish_async(move_test_code_path)
                .await
                .build(),
        )
        .await;
    let resp = context.execute_transaction_must_succeed(txn).await;
    let package_ref = resp.get_new_package_obj().unwrap();
    package_ref.0
}

async fn setup_address_balance_account(
    test_cluster: &mut TestCluster,
    deposit_amount: u64,
) -> (SuiAddress, ObjectID) {
    let rgp = test_cluster.get_reference_gas_price().await;
    let context = &mut test_cluster.wallet;

    let (sender, gas_objects) = get_sender_and_all_gas(context).await;
    let gas_package_id = setup_test_package(context).await;

    let deposit_tx = TestTransactionBuilder::new(sender, gas_objects[1], rgp)
        .transfer_sui_to_address_balance(
            FundSource::coin(gas_objects[1]),
            vec![(deposit_amount, sender)],
        )
        .build();
    test_cluster.sign_and_execute_transaction(&deposit_tx).await;

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
            min_timestamp_seconds: None,
            max_timestamp_seconds: None,
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
            min_timestamp_seconds: None,
            max_timestamp_seconds: None,
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
        .obj(sui_types::transaction::ObjectArg::ImmOrOwnedObject(
            object_to_delete,
        ))
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
            min_timestamp_seconds: None,
            max_timestamp_seconds: None,
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
            min_timestamp_seconds: None,
            max_timestamp_seconds: None,
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
        sui_types::type_input::TypeInput::from(sui_types::gas_coin::GAS::type_tag()),
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
            min_timestamp_seconds: None,
            max_timestamp_seconds: None,
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
            min_timestamp_seconds: None,
            max_timestamp_seconds: None,
            chain: chain_id,
            nonce: 12345,
        },
    })
}

#[sim_test]
async fn test_regular_gas_payment_with_valid_during_current_epoch() {
    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();
    let context = &mut test_cluster.wallet;

    let (sender, gas_coin) = get_sender_and_one_gas(context).await;
    let current_epoch = 0;

    let tx = create_regular_gas_transaction_with_current_epoch(
        sender,
        gas_coin,
        rgp,
        current_epoch,
        chain_id,
    );

    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let (effects, _) = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await
        .unwrap();

    assert!(
        effects.status().is_ok(),
        "Transaction should execute successfully. Error: {:?}",
        effects.status()
    );
}

#[sim_test]
async fn test_transaction_expired_too_early() {
    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();
    let context = &mut test_cluster.wallet;

    let (sender, gas_coin) = get_sender_and_one_gas(context).await;
    let future_epoch = 10;

    let tx = create_regular_gas_transaction_with_current_epoch(
        sender,
        gas_coin,
        rgp,
        future_epoch,
        chain_id,
    );

    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let result = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await;

    match result {
        Err(err) => {
            let err_str = format!("{:?}", err);
            assert!(
                err_str.contains("TransactionExpired"),
                "Expected TransactionExpired error, got: {:?}",
                err
            );
        }
        Ok(_) => panic!("Transaction should be rejected when epoch is too early"),
    }
}

#[sim_test]
async fn test_transaction_expired_too_late() {
    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();
    let context = &mut test_cluster.wallet;

    let (sender, gas_coin) = get_sender_and_one_gas(context).await;

    let past_epoch = 0;

    // trigger epoch 1
    test_cluster.trigger_reconfiguration().await;

    let tx = create_regular_gas_transaction_with_current_epoch(
        sender, gas_coin, rgp, past_epoch, chain_id,
    );

    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let result = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await;

    match result {
        Err(err) => {
            let err_str = format!("{:?}", err);
            assert!(
                err_str.contains("TransactionExpired"),
                "Expected TransactionExpired error, got: {:?}",
                err
            );
        }
        Ok(_) => panic!("Transaction should be rejected when epoch is too late"),
    }
}

#[sim_test]
async fn test_transaction_invalid_chain_id() {
    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let context = &mut test_cluster.wallet;

    let (sender, gas_coin) = get_sender_and_one_gas(context).await;
    let current_epoch = 0;

    let wrong_chain_id = ChainIdentifier::from(CheckpointDigest::default());

    let tx = create_regular_gas_transaction_with_current_epoch(
        sender,
        gas_coin,
        rgp,
        current_epoch,
        wrong_chain_id,
    );

    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let result = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await;

    match result {
        Err(err) => {
            let err_str = format!("{:?}", err);
            assert!(
                err_str.contains("InvalidChainId"),
                "Expected InvalidChainId error, got: {:?}",
                err
            );
        }
        Ok(_) => panic!("Transaction should be rejected with invalid chain ID"),
    }
}

#[sim_test]
async fn test_transaction_expiration_min_none_max_some() {
    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();
    let context = &mut test_cluster.wallet;

    let (sender, gas_coin) = get_sender_and_one_gas(context).await;
    let current_epoch = 0;

    let tx = create_transaction_with_expiration(
        sender,
        gas_coin,
        rgp,
        None,
        Some(current_epoch + 5),
        chain_id,
        12345,
    );

    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let result = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await;

    let err = result.expect_err("Transaction should be rejected when only max_epoch is specified");
    let err_str = format!("{:?}", err);
    assert!(
        err_str.contains("Both min_epoch and max_epoch must be specified and equal"),
        "Expected validation error 'Both min_epoch and max_epoch must be specified and equal', got: {:?}",
        err
    );
}

#[sim_test]
async fn test_transaction_expiration_edge_cases() {
    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();

    let (sender, gas_coin1) = get_sender_and_one_gas(&mut test_cluster.wallet).await;
    let current_epoch = 0;

    // Test case 1: min_epoch = max_epoch (single epoch window)
    let tx1 = create_transaction_with_expiration(
        sender,
        gas_coin1,
        rgp,
        Some(current_epoch), // min_epoch: Some(current)
        Some(current_epoch), // max_epoch: Some(current) - single epoch window
        chain_id,
        100,
    );

    let result1 = test_cluster
        .execute_transaction_return_raw_effects(test_cluster.sign_transaction(&tx1).await)
        .await;
    assert!(
        result1.is_ok(),
        "Single epoch window transaction should succeed"
    );

    // Test case 2: Transaction with min_epoch in the future should be rejected as not yet valid
    let (_, gas_coin2) = get_sender_and_one_gas(&mut test_cluster.wallet).await;
    let tx2 = create_transaction_with_expiration(
        sender,
        gas_coin2,
        rgp,
        Some(current_epoch + 1),
        Some(current_epoch + 1),
        chain_id,
        200,
    );

    let result2 = test_cluster
        .execute_transaction_return_raw_effects(test_cluster.sign_transaction(&tx2).await)
        .await;
    let err2 = result2.expect_err("Transaction should be rejected when min_epoch is in the future");
    let err_str2 = format!("{:?}", err2);
    assert!(
        err_str2.contains("TransactionExpired"),
        "Expected TransactionExpired for future min_epoch, got: {:?}",
        err2
    );

    // Test case 3: min_epoch: Some(past), max_epoch: Some(past) - expired
    let (_, gas_coin3) = get_sender_and_one_gas(&mut test_cluster.wallet).await;

    // First trigger an epoch change to make current_epoch "past"
    test_cluster.trigger_reconfiguration().await;

    let tx3 = create_transaction_with_expiration(
        sender,
        gas_coin3,
        rgp,
        Some(0), // min_epoch: Some(past)
        Some(0), // max_epoch: Some(past) - now expired
        chain_id,
        300,
    );

    let result3 = test_cluster
        .execute_transaction_return_raw_effects(test_cluster.sign_transaction(&tx3).await)
        .await;
    match result3 {
        Err(err) => {
            let err_str = format!("{:?}", err);
            assert!(
                err_str.contains("TransactionExpired"),
                "Expected TransactionExpired for past max_epoch, got: {:?}",
                err
            );
        }
        Ok(_) => panic!("Transaction should be rejected when max_epoch is in the past"),
    }
}

#[sim_test]
async fn test_address_balance_gas_cost_parity() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.enable_address_balance_gas_payments_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let (sender, gas_test_package_id) =
        setup_address_balance_account(&mut test_cluster, 100_000_000).await;

    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();

    let gas_coin = test_cluster
        .wallet
        .get_one_gas_object()
        .await
        .unwrap()
        .unwrap()
        .1;
    let coin_tx = create_storage_test_transaction_gas(sender, gas_test_package_id, gas_coin, rgp);
    let coin_resp = test_cluster.sign_and_execute_transaction(&coin_tx).await;
    let coin_effects = coin_resp.effects.as_ref().unwrap();

    assert!(
        coin_effects.status().is_ok(),
        "Coin payment storage transaction should succeed, but got error: {:?}",
        coin_effects.status()
    );

    let address_balance_tx = create_storage_test_transaction_address_balance(
        sender,
        gas_test_package_id,
        rgp,
        chain_id,
        None,
        0,
    );
    let address_balance_resp = test_cluster
        .sign_and_execute_transaction(&address_balance_tx)
        .await;
    let address_balance_effects = address_balance_resp.effects.as_ref().unwrap();

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

    let coin_created_objects: Vec<_> = coin_effects.created().iter().collect();
    let address_balance_created_objects: Vec<_> =
        address_balance_effects.created().iter().collect();

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

    let coin_created_obj = coin_created_objects[0].reference.to_object_ref();
    let address_balance_created_obj = address_balance_created_objects[0].reference.to_object_ref();

    let gas_coin_for_delete = test_cluster
        .wallet
        .get_one_gas_object()
        .await
        .unwrap()
        .unwrap()
        .1;

    let coin_delete_tx = create_delete_transaction_gas(
        sender,
        gas_test_package_id,
        coin_created_obj,
        gas_coin_for_delete,
        rgp,
    );
    let coin_delete_resp = test_cluster
        .sign_and_execute_transaction(&coin_delete_tx)
        .await;
    let coin_delete_effects = coin_delete_resp.effects.as_ref().unwrap();

    let address_balance_delete_tx = create_delete_transaction_address_balance(
        sender,
        gas_test_package_id,
        address_balance_created_obj,
        rgp,
        chain_id,
        None,
    );
    let address_balance_delete_resp = test_cluster
        .sign_and_execute_transaction(&address_balance_delete_tx)
        .await;
    let address_balance_delete_effects = address_balance_delete_resp.effects.as_ref().unwrap();

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

    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_address_balance_gas_charged_on_move_abort() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.enable_address_balance_gas_payments_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let (sender, gas_test_package_id) =
        setup_address_balance_account(&mut test_cluster, 10_000_000).await;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        verify_accumulator_exists(child_object_resolver, sender, 10_000_000);
    });

    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();

    let abort_tx = create_abort_test_transaction_address_balance(
        sender,
        gas_test_package_id,
        rgp,
        chain_id,
        true,
        None,
    );
    let signed_tx = test_cluster.sign_transaction(&abort_tx).await;
    let (effects, _) = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await
        .expect("Transaction execution should succeed even with Move abort");

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

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        verify_accumulator_exists(child_object_resolver, sender, expected_balance);
    });

    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_explicit_sponsor_withdrawal_banned() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.enable_address_balance_gas_payments_for_testing();
        cfg
    });

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_protocol_version(ProtocolConfig::get_for_max_version_UNSAFE().version)
        .with_epoch_duration_ms(600000)
        .build()
        .await;

    let addresses = test_cluster.get_addresses();
    let sender = addresses[0];
    let sponsor = addresses[1];

    let chain_id = test_cluster.get_chain_identifier();
    let rgp = test_cluster.wallet.get_reference_gas_price().await.unwrap();

    let mut builder = ProgrammableTransactionBuilder::new();
    let withdrawal = FundsWithdrawalArg::balance_from_sponsor(
        1000,
        sui_types::type_input::TypeInput::from(GAS::type_tag()),
    );
    builder.funds_withdrawal(withdrawal).unwrap();

    let tx = TransactionData::V1(TransactionDataV1 {
        kind: TransactionKind::ProgrammableTransaction(builder.finish()),
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: sponsor,
            price: rgp,
            budget: 10000000,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp_seconds: None,
            max_timestamp_seconds: None,
            chain: chain_id,
            nonce: 0u32,
        },
    });

    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let result = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await;

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
    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg
        });

    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;

    let addresses = test_cluster.wallet.get_addresses();
    let sender = addresses[0];
    let sponsor = addresses[1];

    let chain_id = test_cluster.get_chain_identifier();
    let gas_test_package_id = setup_test_package(&mut test_cluster.wallet).await;
    let rgp = test_cluster.wallet.get_reference_gas_price().await.unwrap();

    let sender_gas = test_cluster
        .wallet
        .gas_objects(sender)
        .await
        .unwrap()
        .pop()
        .unwrap()
        .1
        .object_ref();
    let deposit_tx_sender = TestTransactionBuilder::new(sender, sender_gas, rgp)
        .transfer_sui_to_address_balance(FundSource::coin(sender_gas), vec![(100_000_000, sender)])
        .build();
    test_cluster
        .sign_and_execute_transaction(&deposit_tx_sender)
        .await;

    let sponsor_initial_balance = 15_000_000u64;
    let sponsor_gas = test_cluster
        .wallet
        .gas_objects(sponsor)
        .await
        .unwrap()
        .pop()
        .unwrap()
        .1
        .object_ref();
    let deposit_tx_sponsor = TestTransactionBuilder::new(sponsor, sponsor_gas, rgp)
        .transfer_sui_to_address_balance(
            FundSource::coin(sponsor_gas),
            vec![(sponsor_initial_balance, sponsor)],
        )
        .build();
    test_cluster
        .sign_and_execute_transaction(&deposit_tx_sponsor)
        .await;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        verify_accumulator_exists(child_object_resolver, sponsor, sponsor_initial_balance);
    });

    let tx1 = create_storage_test_transaction_address_balance(
        sender,
        gas_test_package_id,
        rgp,
        chain_id,
        Some(sponsor),
        0,
    );
    let tx2 = create_storage_test_transaction_address_balance(
        sender,
        gas_test_package_id,
        rgp,
        chain_id,
        Some(sponsor),
        1,
    );

    let sender_sig1 = test_cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sender, &tx1, Intent::sui_transaction())
        .await
        .unwrap();
    let sponsor_sig1 = test_cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sponsor, &tx1, Intent::sui_transaction())
        .await
        .unwrap();
    let signed_tx1 = Transaction::from_data(tx1, vec![sender_sig1, sponsor_sig1]);

    let sender_sig2 = test_cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sender, &tx2, Intent::sui_transaction())
        .await
        .unwrap();
    let sponsor_sig2 = test_cluster
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

    let mut effects = test_cluster
        .execute_signed_txns_in_soft_bundle(&[signed_tx1, signed_tx2])
        .await
        .unwrap();

    test_cluster
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
        status_str.contains("InsufficientBalanceForWithdraw"),
        "Failed transaction should have InsufficientBalanceForWithdraw error"
    );

    let successful_tx_gas = succeeded_gas;

    let final_sponsor_balance = test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        get_balance(child_object_resolver, sponsor)
    });

    let expected_final_sponsor_balance = sponsor_initial_balance - successful_tx_gas;
    assert_eq!(
        final_sponsor_balance, expected_final_sponsor_balance,
        "Sponsor balance should reflect only the successful transaction"
    );

    let final_sender_balance = test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        get_balance(child_object_resolver, sender)
    });

    assert_eq!(
        final_sender_balance, 100_000_000,
        "Sender balance should remain unchanged"
    );

    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_insufficient_balance_charges_zero_gas() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.enable_address_balance_gas_payments_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();

    let (sender, gas_for_deposit) = get_sender_and_one_gas(&mut test_cluster.wallet).await;

    let initial_balance = 30_000_000u64;
    let withdraw_amount = 15_000_000u64;

    let deposit_tx = TestTransactionBuilder::new(sender, gas_for_deposit, rgp)
        .transfer_sui_to_address_balance(
            FundSource::coin(gas_for_deposit),
            vec![(initial_balance, sender)],
        )
        .build();
    test_cluster.sign_and_execute_transaction(&deposit_tx).await;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        verify_accumulator_exists(child_object_resolver, sender, initial_balance);
    });
    let tx1 = create_withdraw_balance_transaction(sender, rgp, chain_id, withdraw_amount, 0);

    let tx2 = create_withdraw_balance_transaction(sender, rgp, chain_id, withdraw_amount, 1);

    let tx1_digest = tx1.digest();
    let tx2_digest = tx2.digest();

    assert_ne!(
        tx1_digest, tx2_digest,
        "Transaction digests should be different"
    );

    let mut effects = test_cluster
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
        status_str.contains("InsufficientBalanceForWithdraw"),
        "Failed transaction should have InsufficientBalanceForWithdraw error"
    );

    let successful_tx_gas = succeeded_gas;

    test_cluster
        .wait_for_tx_settlement(&[tx1_digest, tx2_digest])
        .await;

    test_cluster
        .fullnode_handle
        .sui_node
        .with_async(|node| async move {
            let state = node.state();
            let child_object_resolver = state.get_child_object_resolver().as_ref();

            let final_sender_balance = get_balance(child_object_resolver, sender);

            let expected_final_balance = initial_balance - withdraw_amount - successful_tx_gas;
            assert_eq!(
                final_sender_balance, expected_final_balance,
                "Final balance should reflect only the successful transaction"
            );
        })
        .await;

    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_soft_bundle_different_gas_payers() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.enable_address_balance_gas_payments_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;

    let addresses = test_cluster.wallet.get_addresses();
    let sender1 = addresses[0];
    let sender2 = addresses[1];

    let rgp = test_cluster.wallet.get_reference_gas_price().await.unwrap();
    let chain_id = test_cluster.get_chain_identifier();

    let gas_test_package_id = setup_test_package(&mut test_cluster.wallet).await;

    let sender1_gas = test_cluster
        .wallet
        .gas_objects(sender1)
        .await
        .unwrap()
        .pop()
        .unwrap()
        .1
        .object_ref();
    let deposit_tx1 = TestTransactionBuilder::new(sender1, sender1_gas, rgp)
        .transfer_sui_to_address_balance(FundSource::coin(sender1_gas), vec![(10_000_000, sender1)])
        .build();
    test_cluster
        .sign_and_execute_transaction(&deposit_tx1)
        .await;

    let sender2_gas = test_cluster
        .wallet
        .gas_objects(sender2)
        .await
        .unwrap()
        .pop()
        .unwrap()
        .1
        .object_ref();
    let deposit_tx2 = TestTransactionBuilder::new(sender2, sender2_gas, rgp)
        .transfer_sui_to_address_balance(FundSource::coin(sender2_gas), vec![(10_000_000, sender2)])
        .build();
    test_cluster
        .sign_and_execute_transaction(&deposit_tx2)
        .await;

    let tx1 = create_storage_test_transaction_address_balance(
        sender1,
        gas_test_package_id,
        rgp,
        chain_id,
        None,
        0,
    );

    let tx2 = create_storage_test_transaction_address_balance(
        sender2,
        gas_test_package_id,
        rgp,
        chain_id,
        None,
        1,
    );

    let tx1_digest = tx1.digest();
    let tx2_digest = tx2.digest();

    assert_ne!(
        tx1_digest, tx2_digest,
        "Transaction digests should be different"
    );

    let mut effects = test_cluster
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

    test_cluster
        .wait_for_tx_settlement(&[tx1_digest, tx2_digest])
        .await;

    test_cluster
        .fullnode_handle
        .sui_node
        .with_async(|node| async move {
            let state = node.state();

            let child_object_resolver = state.get_child_object_resolver().as_ref();

            let actual_balance1 = get_balance(child_object_resolver, sender1);
            let actual_balance2 = get_balance(child_object_resolver, sender2);

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
        })
        .await;

    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_multiple_deposits_merged_in_effects() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.create_root_accumulator_object_for_testing();
        cfg.enable_accumulators_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let context = &mut test_cluster.wallet;

    let (sender, gas) = get_sender_and_one_gas(context).await;
    let recipient = sender;

    let initial_deposit = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(10000, recipient)])
        .build();
    let res = test_cluster
        .sign_and_execute_transaction(&initial_deposit)
        .await;
    let gas = res.effects.unwrap().gas_object().reference.to_object_ref();

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        verify_accumulator_exists(child_object_resolver, recipient, 10000);
    });

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
            sui_types::type_input::TypeInput::from(sui_types::gas_coin::GAS::type_tag()),
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

    let tx = TransactionKind::ProgrammableTransaction(builder.finish());
    let tx_data = TransactionData::new(tx, sender, gas, 10000000, rgp);

    let signed_tx = test_cluster.wallet.sign_transaction(&tx_data).await;
    let (effects, _) = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await
        .unwrap();

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
    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        verify_accumulator_exists(child_object_resolver, recipient, expected_final_balance);
    });

    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_address_balance_gas_budget_enforcement_with_storage() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.enable_address_balance_gas_payments_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let (sender, gas_package_id) =
        setup_address_balance_account(&mut test_cluster, 100_000_000).await;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        verify_accumulator_exists(child_object_resolver, sender, 100_000_000);
    });

    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();

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
    let tx1 = create_address_balance_transaction(tx_kind1, sender, 5_000_000, rgp, chain_id);
    let res1 = test_cluster.sign_and_execute_transaction(&tx1).await;
    let created_obj = res1.effects.unwrap().created()[0].reference.to_object_ref();

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
    let tx = create_address_balance_transaction(tx_kind, sender, low_budget, rgp, chain_id);

    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let (effects, _) = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await
        .expect("Transaction execution should not panic");

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

    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_address_balance_computation_oog() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.enable_address_balance_gas_payments_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let (sender, gas_package_id) =
        setup_address_balance_account(&mut test_cluster, 100_000_000).await;

    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();

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
    let tx = create_address_balance_transaction(tx_kind, sender, low_budget, rgp, chain_id);

    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let (effects, _) = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await
        .expect("Transaction execution should not panic");

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

    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_address_balance_large_rebate() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.enable_address_balance_gas_payments_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let (sender, gas_package_id) =
        setup_address_balance_account(&mut test_cluster, 100_000_000).await;

    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();

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
    let tx = create_address_balance_transaction(tx_kind, sender, 50_000_000, rgp, chain_id);

    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let (effects, _) = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await
        .expect("Transaction execution should not panic");

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

    let initial_balance = test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        get_balance(child_object_resolver, sender)
    });

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
    let tx = create_address_balance_transaction(tx_kind, sender, 50_000_000, rgp, chain_id);

    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let (effects, _) = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await
        .expect("Transaction execution should not panic");

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

    let final_balance = test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        get_balance(child_object_resolver, sender)
    });

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

    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_sponsored_address_balance_storage_oog() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.enable_address_balance_gas_payments_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let rgp = test_cluster.get_reference_gas_price().await;

    let addresses = test_cluster.wallet.get_addresses();
    let sender = addresses[0];
    let sponsor = addresses[1];

    let gas_package_id = setup_test_package(&mut test_cluster.wallet).await;

    let sender_gas = test_cluster
        .wallet
        .gas_objects(sender)
        .await
        .unwrap()
        .pop()
        .unwrap()
        .1
        .object_ref();
    let deposit_tx_sender = TestTransactionBuilder::new(sender, sender_gas, rgp)
        .transfer_sui_to_address_balance(FundSource::coin(sender_gas), vec![(100_000_000, sender)])
        .build();
    test_cluster
        .sign_and_execute_transaction(&deposit_tx_sender)
        .await;

    let sponsor_gas = test_cluster
        .wallet
        .gas_objects(sponsor)
        .await
        .unwrap()
        .pop()
        .unwrap()
        .1
        .object_ref();
    let deposit_tx_sponsor = TestTransactionBuilder::new(sponsor, sponsor_gas, rgp)
        .transfer_sui_to_address_balance(
            FundSource::coin(sponsor_gas),
            vec![(100_000_000, sponsor)],
        )
        .build();
    test_cluster
        .sign_and_execute_transaction(&deposit_tx_sponsor)
        .await;

    let chain_id = test_cluster.get_chain_identifier();

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
        rgp,
        chain_id,
    );

    let sender_sig = test_cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sender, &tx, Intent::sui_transaction())
        .await
        .unwrap();
    let sponsor_sig = test_cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sponsor, &tx, Intent::sui_transaction())
        .await
        .unwrap();

    let signed_tx = Transaction::from_data(tx, vec![sender_sig, sponsor_sig]);
    let (effects, _) = test_cluster
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

    test_cluster.trigger_reconfiguration().await;
}
