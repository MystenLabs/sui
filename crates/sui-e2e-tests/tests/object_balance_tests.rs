// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use rand::{Rng, seq::SliceRandom};
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::FundSource;
use sui_types::{
    SUI_ACCUMULATOR_ROOT_OBJECT_ID,
    base_types::{SequenceNumber, SuiAddress},
    effects::{TransactionEffects, TransactionEffectsAPI, UnchangedConsensusKind},
    gas_coin::MIST_PER_SUI,
    object::Owner,
};
use test_cluster::{
    TestClusterBuilder,
    addr_balance_test_env::{TestEnv, TestEnvBuilder},
};

#[sim_test]
async fn test_object_balance_withdraw_stress() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.set_create_root_accumulator_object_for_testing(true);
        cfg.set_enable_accumulators_for_testing(true);
        cfg.set_enable_object_funds_withdraw_for_testing(true);
        cfg.set_check_object_funds_withdraw_in_execution_for_testing(true);
        cfg
    });

    let test_cluster = Arc::new(TestClusterBuilder::new().build().await);
    let sender = test_cluster.get_address_0();

    // Publish the object_balance package from examples.
    let publish_tx = test_cluster
        .test_transaction_builder_with_sender(sender)
        .await
        .publish_examples("object_balance")
        .await
        .build();
    let response = test_cluster.sign_and_execute_transaction(&publish_tx).await;
    let package_id = response.get_new_package_obj().unwrap().0;

    // Create 3 vault objects, one owned, one party owned, one shared.
    let mut vault_objects = vec![];
    for idx in 0..3 {
        let mut builder = test_cluster
            .test_transaction_builder_with_sender(sender)
            .await;
        if idx == 0 {
            builder = builder.move_call(package_id, "object_balance", "new_owned", vec![]);
        } else if idx == 1 {
            builder = builder.move_call(package_id, "object_balance", "new_party", vec![]);
        } else {
            builder = builder.move_call(package_id, "object_balance", "new_shared", vec![]);
        }
        let tx = builder.build();
        let effects = test_cluster.sign_and_execute_transaction(&tx).await.effects;
        let vault_object = effects.created().first().cloned().unwrap().0;
        vault_objects.push(vault_object);
    }

    // Fund the vault objects, each with an initial balance of 1000 MIST.
    let gas_object = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await
        .unwrap()
        .unwrap();
    let tx = test_cluster
        .test_transaction_builder()
        .await
        .transfer_sui_to_address_balance(
            FundSource::Coin(gas_object),
            vault_objects
                .iter()
                .map(|vault_object| (1000, vault_object.0.into()))
                .collect(),
        )
        .build();
    test_cluster.sign_and_execute_transaction(&tx).await;

    // Create 7 gas coins.
    let gas_coins = test_cluster
        .wallet
        .get_gas_objects_owned_by_address(sender, Some(2))
        .await
        .unwrap();
    let tx = test_cluster
        .test_transaction_builder_with_gas_object(sender, gas_coins[0])
        .await
        .split_coin(gas_coins[1], vec![10000 * MIST_PER_SUI; 7])
        .build();
    let effects = test_cluster.sign_and_execute_transaction(&tx).await.effects;
    let gas_coins: Vec<_> = effects.created().into_iter().map(|oref| oref.0.0).collect();

    // Start 7 threads, each thread controls one gas object.
    // One thread withdraws funds from the owned vault object;
    // 3 threads withdraw funds from the party owned vault object;
    // 3 threads withdraw funds from the shared vault object;
    // These threads will keep withdrawing funds and send to another random vault object
    // with some random amount.
    // Each thread will execute 50 transactions and then exit.
    // Some of them may fail due to insufficient balance and that's expected.
    let mut handles = vec![];
    for (idx, gas_coin) in gas_coins.into_iter().enumerate() {
        let vault_objects = vault_objects.clone();
        let test_cluster = test_cluster.clone();
        handles.push(tokio::spawn(async move {
            let mut vault_object = if idx == 0 {
                vault_objects[0]
            } else if idx < 4 {
                vault_objects[1]
            } else {
                vault_objects[2]
            };
            let init_shared_version = vault_object.1;
            for _ in 0..50 {
                let amount = rand::thread_rng().gen_range(0..500) as u64;
                let recipient = vault_objects
                    .choose(&mut rand::thread_rng())
                    .unwrap()
                    .0
                    .into();
                let gas_object = test_cluster
                    .get_object_from_fullnode_store(&gas_coin)
                    .await
                    .unwrap()
                    .compute_object_reference();
                let fund_source = if idx == 0 {
                    FundSource::object_fund_owned(package_id, vault_object)
                } else {
                    FundSource::object_fund_shared(package_id, vault_object.0, init_shared_version)
                };
                let tx = test_cluster
                    .test_transaction_builder_with_gas_object(sender, gas_object)
                    .await
                    .transfer_sui_to_address_balance(fund_source, vec![(amount, recipient)])
                    .build();
                let tx = test_cluster.sign_transaction(&tx).await;
                let effects = test_cluster
                    .wallet
                    .execute_transaction_may_fail(tx)
                    .await
                    .unwrap()
                    .effects;
                vault_object = effects
                    .mutated()
                    .into_iter()
                    .find(|oref| oref.0.0 == vault_object.0)
                    .unwrap()
                    .0;
            }
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }
    test_cluster.trigger_reconfiguration().await;
}

fn object_funds_in_execution_test_env() -> TestEnvBuilder {
    TestEnvBuilder::new().with_proto_override_cb(Box::new(|_, mut cfg| {
        cfg.set_enable_object_funds_withdraw_for_testing(true);
        cfg.set_check_object_funds_withdraw_in_execution_for_testing(true);
        cfg
    }))
}

fn assert_object_funds_insufficient(
    status: &sui_types::execution_status::ExecutionStatus,
    context: &str,
) {
    assert!(
        matches!(
            status,
            sui_types::execution_status::ExecutionStatus::Failure(failure)
                if sui_types::funds_accumulator::is_object_funds_insufficient_abort(&failure.error)
        ),
        "{context}: expected object-funds insufficiency abort, got: {status:?}"
    );
}

#[sim_test]
async fn test_simulate_object_funds_sufficient_in_execution() {
    let mut test_env = object_funds_in_execution_test_env().build().await;

    let sender = test_env.get_sender(0);
    let (package_id, vault_id) = test_env.setup_funded_object_balance_vault(1000).await;

    let vault_oref = test_env.cluster.get_latest_object_ref(&vault_id).await;
    let tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(
            FundSource::object_fund_owned(package_id, vault_oref),
            vec![(500, sender)],
        )
        .build();

    let result = test_env
        .cluster
        .grpc_client()
        .simulate_transaction(&tx, false, false)
        .await
        .unwrap();
    assert!(result.transaction.effects.status().is_ok());
}

#[sim_test]
async fn test_simulate_object_funds_insufficient_in_execution() {
    let mut test_env = object_funds_in_execution_test_env().build().await;

    let sender = test_env.get_sender(0);
    let (package_id, vault_id) = test_env.setup_funded_object_balance_vault(100).await;

    let vault_oref = test_env.cluster.get_latest_object_ref(&vault_id).await;
    let tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(
            FundSource::object_fund_owned(package_id, vault_oref),
            vec![(500, sender)],
        )
        .build();

    let result = test_env
        .cluster
        .grpc_client()
        .simulate_transaction(&tx, false, false)
        .await
        .unwrap();
    assert_object_funds_insufficient(result.transaction.effects.status(), "simulated transaction");
}

fn accumulator_recorded_as_read_only_root(effects: &TransactionEffects) -> bool {
    effects
        .unchanged_consensus_objects()
        .iter()
        .any(|(id, kind)| {
            *id == SUI_ACCUMULATOR_ROOT_OBJECT_ID
                && matches!(kind, UnchangedConsensusKind::ReadOnlyRoot(_))
        })
}

#[sim_test]
async fn test_object_funds_sufficient_in_execution() {
    let mut test_env = object_funds_in_execution_test_env().build().await;

    let sender = test_env.get_sender(0);
    let (package_id, vault_id) = test_env.setup_funded_object_balance_vault(1000).await;

    let vault_oref = test_env.cluster.get_latest_object_ref(&vault_id).await;
    let tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(
            FundSource::object_fund_owned(package_id, vault_oref),
            vec![(500, sender)],
        )
        .build();

    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(
        effects.status().is_ok(),
        "sufficient object-funds withdrawal should succeed: {:?}",
        effects.status()
    );
    assert!(
        accumulator_recorded_as_read_only_root(&effects),
        "accumulator root should be recorded as ReadOnlyRoot with the in-execution check on: {:?}",
        effects.unchanged_consensus_objects()
    );
}

#[sim_test]
async fn test_object_funds_effects_unchanged_when_flag_off() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.set_enable_object_funds_withdraw_for_testing(true);
            cfg.set_check_object_funds_withdraw_in_execution_for_testing(false);
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);
    let (package_id, vault_id) = test_env.setup_funded_object_balance_vault(1000).await;

    let vault_oref = test_env.cluster.get_latest_object_ref(&vault_id).await;
    let tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(
            FundSource::object_fund_owned(package_id, vault_oref),
            vec![(500, sender)],
        )
        .build();

    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok());
    assert!(
        !accumulator_recorded_as_read_only_root(&effects),
        "no ReadOnlyRoot should be recorded with the in-execution check off: {:?}",
        effects.unchanged_consensus_objects()
    );
}

#[sim_test]
async fn test_object_funds_insufficient_in_execution() {
    let mut test_env = object_funds_in_execution_test_env().build().await;

    let sender = test_env.get_sender(0);
    let (package_id, vault_id) = test_env.setup_funded_object_balance_vault(100).await;

    let vault_oref = test_env.cluster.get_latest_object_ref(&vault_id).await;
    let tx = test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(
            FundSource::object_fund_owned(package_id, vault_oref),
            vec![(500, sender)],
        )
        .build();

    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert_object_funds_insufficient(effects.status(), "executed transaction");
}

fn accumulator_read_only_root_version(effects: &TransactionEffects) -> Option<SequenceNumber> {
    effects
        .unchanged_consensus_objects()
        .iter()
        .find_map(|(id, kind)| match kind {
            UnchangedConsensusKind::ReadOnlyRoot((version, _))
                if *id == SUI_ACCUMULATOR_ROOT_OBJECT_ID =>
            {
                Some(*version)
            }
            _ => None,
        })
}

#[sim_test]
async fn test_object_funds_unsettled_withdraws_same_commit() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.set_enable_object_funds_withdraw_for_testing(true);
            cfg.set_check_object_funds_withdraw_in_execution_for_testing(true);
            cfg.set_record_net_unsettled_object_withdraws_for_testing(true);
            cfg
        }))
        .build()
        .await;

    let sender0 = test_env.get_sender(0);
    let sender1 = test_env.get_sender(1);

    let tx = test_env
        .tx_builder(sender0)
        .publish_examples("object_balance")
        .await
        .build();
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    let package_id = effects
        .created()
        .into_iter()
        .find(|(_, owner)| owner.is_immutable())
        .unwrap()
        .0
        .0;

    let tx = test_env
        .tx_builder(sender0)
        .move_call(package_id, "object_balance", "new_shared", vec![])
        .build();
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    let (vault_ref, vault_owner) = effects.created().into_iter().next().unwrap();
    let vault_id = vault_ref.0;
    let Owner::Shared {
        initial_shared_version,
    } = vault_owner
    else {
        panic!("vault must be shared, got {vault_owner:?}");
    };

    let fund_vault = |test_env: &TestEnv, amount: u64| {
        let gas = test_env.get_sender_and_gas(0).1;
        test_env
            .tx_builder(sender0)
            .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(amount, vault_id.into())])
            .build()
    };
    let withdraw_tx = |test_env: &TestEnv, sender: SuiAddress, amount: u64| {
        test_env
            .tx_builder(sender)
            .transfer_sui_to_address_balance(
                FundSource::object_fund_shared(package_id, vault_id, initial_shared_version),
                vec![(amount, sender)],
            )
            .build()
    };

    let tx = fund_vault(&test_env, 1000);
    test_env.exec_tx_directly(tx).await.unwrap();
    test_env.trigger_reconfiguration().await;

    let mut executed_digests = vec![];
    let mut settled = 1000u64;

    let mut same_accumulator_version = false;
    for _ in 0..10 {
        let tx0 = withdraw_tx(&test_env, sender0, 600);
        let tx1 = withdraw_tx(&test_env, sender1, 600);
        let results = test_env
            .cluster
            .sign_and_execute_txns_in_soft_bundle(&[tx0, tx1])
            .await
            .unwrap();
        test_env.update_all_gas().await;

        let successes = results.iter().filter(|(_, fx)| fx.status().is_ok()).count();
        assert_eq!(successes, 1, "the settled balance covers exactly one 600");
        let failed_status = results
            .iter()
            .find(|(_, fx)| fx.status().is_err())
            .map(|(_, fx)| fx.status())
            .unwrap();
        assert_object_funds_insufficient(failed_status, "same-commit overdraw");
        settled -= 600;
        executed_digests.extend(results.iter().map(|(digest, _)| *digest));

        let v0 = accumulator_read_only_root_version(&results[0].1)
            .expect("withdrawal must record the accumulator root read");
        let v1 = accumulator_read_only_root_version(&results[1].1)
            .expect("withdrawal must record the accumulator root read");
        if v0 == v1 {
            same_accumulator_version = true;
            break;
        }
        let tx = fund_vault(&test_env, 600);
        test_env.exec_tx_directly(tx).await.unwrap();
        settled += 600;
    }
    assert!(
        same_accumulator_version,
        "withdrawals never landed in the same consensus commit"
    );

    let tx = fund_vault(&test_env, 1000 - settled);
    test_env.exec_tx_directly(tx).await.unwrap();
    settled = 1000;
    let tx0 = withdraw_tx(&test_env, sender0, 400);
    let tx1 = withdraw_tx(&test_env, sender1, 400);
    let results = test_env
        .cluster
        .sign_and_execute_txns_in_soft_bundle(&[tx0, tx1])
        .await
        .unwrap();
    test_env.update_all_gas().await;
    for (digest, fx) in &results {
        assert!(
            fx.status().is_ok(),
            "both fitting withdrawals must succeed, got: {:?}",
            fx.status()
        );
        executed_digests.push(*digest);
    }
    settled -= 800;

    let tx = withdraw_tx(&test_env, sender0, settled);
    let (digest, fx) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(
        fx.status().is_ok(),
        "spending the exact remaining balance must succeed, got: {:?}",
        fx.status()
    );
    executed_digests.push(digest);

    let fullnode = test_env.cluster.spawn_new_fullnode().await.sui_node;
    fullnode
        .state()
        .get_transaction_cache_reader()
        .notify_read_executed_effects("", &executed_digests)
        .await;
}
