// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use move_core_types::identifier::Identifier;

use sui_json_rpc_api::BridgeReadApiClient;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_json_rpc_types::{SuiExecutionStatus, SuiTransactionBlockResponseOptions};
use sui_macros::sim_test;
use sui_types::bridge::BridgeTrait;
use sui_types::bridge::{get_bridge, get_bridge_obj_initial_shared_version, BRIDGE_MODULE_NAME};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{CallArg, ObjectArg, Transaction, TransactionData};
use sui_types::{BRIDGE_PACKAGE_ID, SUI_BRIDGE_OBJECT_ID};
use test_cluster::TestClusterBuilder;

pub const BRIDGE_ENABLE_PROTOCOL_VERSION: u64 = 43;

#[sim_test]
async fn test_create_bridge_state_object() {
    let test_cluster = TestClusterBuilder::new()
        .with_protocol_version((BRIDGE_ENABLE_PROTOCOL_VERSION - 1).into())
        .with_epoch_duration_ms(20000)
        .build()
        .await;

    let handles = test_cluster.all_node_handles();

    // no node has the bridge state object yet
    for h in &handles {
        h.with(|node| {
            assert!(node
                .state()
                .get_cache_reader()
                .get_latest_object_ref_or_tombstone(SUI_BRIDGE_OBJECT_ID)
                .unwrap()
                .is_none());
        });
    }

    // wait until feature is enabled
    test_cluster
        .wait_for_protocol_version(BRIDGE_ENABLE_PROTOCOL_VERSION.into())
        .await;
    // wait until next epoch - authenticator state object is created at the end of the first epoch
    // in which it is supported.
    test_cluster.wait_for_epoch_all_nodes(2).await; // protocol upgrade completes in epoch 1

    for h in &handles {
        h.with(|node| {
            node.state()
                .get_cache_reader()
                .get_latest_object_ref_or_tombstone(SUI_BRIDGE_OBJECT_ID)
                .unwrap()
                .expect("auth state object should exist");
        });
    }
}

#[tokio::test]
async fn test_committee_registration() {
    telemetry_subscribers::init_for_testing();
    let test_cluster: test_cluster::TestCluster = TestClusterBuilder::new()
        .with_protocol_version(BRIDGE_ENABLE_PROTOCOL_VERSION.into())
        .with_epoch_duration_ms(10000)
        .build()
        .await;
    let ref_gas_price = test_cluster.get_reference_gas_price().await;
    let bridge_shared_version = get_bridge_obj_initial_shared_version(
        test_cluster
            .fullnode_handle
            .sui_node
            .state()
            .get_object_store(),
    )
    .unwrap()
    .unwrap();

    // Register bridge authorities
    for (n, node) in test_cluster.swarm.active_validators().enumerate() {
        let validator_address = node.config.sui_address();
        // 1, send some gas to validator
        let sender = test_cluster.get_address_0();
        let tx = test_cluster
            .test_transaction_builder_with_sender(sender)
            .await
            .transfer_sui(Some(1000000000), validator_address)
            .build();
        let response = test_cluster.sign_and_execute_transaction(&tx).await;
        assert_eq!(
            &SuiExecutionStatus::Success,
            response.effects.unwrap().status()
        );

        // 2, create committee registration tx
        let coins = test_cluster
            .sui_client()
            .coin_read_api()
            .get_coins(validator_address, None, None, None)
            .await
            .unwrap();
        let gas = coins.data.first().unwrap();
        let mut builder = ProgrammableTransactionBuilder::new();
        let bridge = builder
            .obj(ObjectArg::SharedObject {
                id: SUI_BRIDGE_OBJECT_ID,
                initial_shared_version: bridge_shared_version,
                mutable: true,
            })
            .unwrap();
        let system_state = builder.obj(ObjectArg::SUI_SYSTEM_MUT).unwrap();
        let pub_key = [n as u8; 33].to_vec();
        let bridge_pubkey = builder
            .input(CallArg::Pure(bcs::to_bytes(&pub_key).unwrap()))
            .unwrap();
        let url = builder
            .input(CallArg::Pure(
                bcs::to_bytes("bridge_test_url".as_bytes()).unwrap(),
            ))
            .unwrap();

        builder.programmable_move_call(
            BRIDGE_PACKAGE_ID,
            BRIDGE_MODULE_NAME.into(),
            Identifier::from_str("committee_registration").unwrap(),
            vec![],
            vec![bridge, system_state, bridge_pubkey, url],
        );

        let data = TransactionData::new_programmable(
            validator_address,
            vec![gas.object_ref()],
            builder.finish(),
            1000000000,
            ref_gas_price,
        );

        let tx =
            Transaction::from_data_and_signer(data, vec![node.config.account_key_pair.keypair()]);

        let response = test_cluster
            .sui_client()
            .quorum_driver_api()
            .execute_transaction_block(
                tx,
                SuiTransactionBlockResponseOptions::new().with_effects(),
                None,
            )
            .await
            .unwrap();
        assert_eq!(
            &SuiExecutionStatus::Success,
            response.effects.unwrap().status()
        );
    }

    let bridge = get_bridge(
        test_cluster
            .fullnode_handle
            .sui_node
            .state()
            .get_object_store(),
    )
    .unwrap();

    // Member should be empty before end of epoch
    assert!(bridge.committee().members.contents.is_empty());
    assert_eq!(
        test_cluster.swarm.active_validators().count(),
        bridge.committee().member_registrations.contents.len()
    );

    // wait for next epoch
    test_cluster.wait_for_epoch(None).await;

    let bridge = get_bridge(
        test_cluster
            .fullnode_handle
            .sui_node
            .state()
            .get_object_store(),
    )
    .unwrap();

    // Committee should be initiated
    assert!(bridge.committee().member_registrations.contents.is_empty());
    assert_eq!(
        test_cluster.swarm.active_validators().count(),
        bridge.committee().members.contents.len()
    );
}

#[tokio::test]
async fn test_bridge_api_compatibility() {
    let test_cluster: test_cluster::TestCluster = TestClusterBuilder::new()
        .with_protocol_version(BRIDGE_ENABLE_PROTOCOL_VERSION.into())
        .with_epoch_duration_ms(10000)
        .build()
        .await;

    let client = test_cluster.rpc_client();
    client.get_latest_bridge().await.unwrap();
    // TODO: assert fields in summary

    client
        .get_bridge_object_initial_shared_version()
        .await
        .unwrap();
}
