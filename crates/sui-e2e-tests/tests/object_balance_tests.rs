// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use rand::{Rng, seq::SliceRandom};
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::FundSource;
use sui_types::gas_coin::MIST_PER_SUI;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn test_object_balance_withdraw_stress() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.create_root_accumulator_object_for_testing();
        cfg.enable_accumulators_for_testing();
        cfg.set_enable_object_funds_withdraw_for_testing(true);
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
        let effects = test_cluster
            .sign_and_execute_transaction(&tx)
            .await
            .effects
            .unwrap();
        let vault_object = effects
            .created()
            .first()
            .cloned()
            .unwrap()
            .reference
            .to_object_ref();
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
    let effects = test_cluster
        .sign_and_execute_transaction(&tx)
        .await
        .effects
        .unwrap();
    let gas_coins: Vec<_> = effects
        .created()
        .iter()
        .map(|oref| oref.reference.object_id)
        .collect();

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
                    .effects
                    .unwrap();
                vault_object = effects
                    .mutated()
                    .iter()
                    .find(|oref| oref.object_id() == vault_object.0)
                    .unwrap()
                    .reference
                    .to_object_ref();
            }
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }
    test_cluster.trigger_reconfiguration().await;
}
