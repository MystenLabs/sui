// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc::Client;
use sui_rpc_api::grpc::local_execution::LocalExecutionServiceClient;
use sui_types::base_types::TransactionDigest;
use sui_types::local_execution::WaitForLocalEffectsRequest;
use sui_types::local_execution::WaitForLocalEffectsResponse;
use sui_types::message_envelope::Message;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::TransactionData;
use test_cluster::TestClusterBuilder;

const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

/// Once the full node has locally executed a transaction, `WaitForLocalEffects` returns its
/// effects immediately (and the returned effects digest matches the details when requested).
#[sim_test]
async fn wait_for_local_effects_returns_executed_effects() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let sender = cluster.get_address_0();

    let gas = cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await
        .unwrap()
        .expect("sender should have a gas object");
    let gas_price = cluster.wallet.get_reference_gas_price().await.unwrap();
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(sender, None);
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        gas_price,
    );
    let signed = cluster.wallet.sign_transaction(&data).await;
    let digest = *signed.digest();

    // Drive the transaction through the full node so it executes it locally.
    let mut rpc_client = Client::new(cluster.rpc_url().to_owned()).unwrap();
    super::execute_transaction(&mut rpc_client, &signed).await;

    let mut client = LocalExecutionServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();
    let response = client
        .wait_for_local_effects(WaitForLocalEffectsRequest {
            transaction_digest: digest,
            timeout_ms: Some(10_000),
            include_details: true,
        })
        .await
        .unwrap()
        .into_inner();

    match response {
        WaitForLocalEffectsResponse::Executed {
            effects_digest,
            effects,
        } => {
            let effects = effects.expect("details were requested");
            assert_eq!(effects_digest, effects.digest());
        }
        WaitForLocalEffectsResponse::TimedOut => panic!("expected the transaction to be executed"),
    }
}

/// A transaction the node has never seen does not resolve, so the endpoint returns `TimedOut`
/// once the (short) client-supplied wait elapses.
#[sim_test]
async fn wait_for_local_effects_times_out_for_unknown_transaction() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;

    let mut client = LocalExecutionServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();
    let response = client
        .wait_for_local_effects(WaitForLocalEffectsRequest {
            transaction_digest: TransactionDigest::random(),
            timeout_ms: Some(500),
            include_details: false,
        })
        .await
        .unwrap()
        .into_inner();

    assert!(matches!(response, WaitForLocalEffectsResponse::TimedOut));
}
