// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Ports the SUI-only subset of
//! `sui-indexer-alt-e2e-tests/tests/consistent_store_address_balance_tests.rs`.
//!
//! Address balance is gated behind the accumulator feature flag,
//! which lives on the global `ProtocolConfig`. Each test applies
//! the override at the top via
//! `ProtocolConfig::apply_overrides_for_testing(...)` — nextest
//! runs each `#[tokio::test]` in its own process, so the global
//! is isolated per test and we don't need a cross-test mutex.
//!
//! The accumulator tests that need a custom Move coin package
//! (`test_address_to_address_transfer`, `test_list_balances_pagination`,
//! `test_multiple_coin_types`) live in
//! [`multi_coin_address_balance`] alongside the on-disk
//! `tests/packages/coin` source.

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::BatchGetBalancesRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::CHECKPOINT_HEIGHT_METADATA;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::GetBalanceRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::ListBalancesRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_client::ConsistentServiceClient;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::FundSource;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::get_account_key_pair;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::gas_coin::GAS;
use sui_types::object::Owner;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

/// Apply the accumulator-enabling overrides on the process-wide
/// `ProtocolConfig`. Tests `let _guard = accumulator_overrides()`
/// before any cluster construction so Simulacrum picks up the
/// patched config.
fn accumulator_overrides() -> sui_protocol_config::OverrideGuard {
    ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.create_root_accumulator_object_for_testing();
        cfg.enable_accumulators_for_testing();
        cfg
    })
}

/// Drive a single SUI-to-address-balance transfer on
/// `cluster`. Requests gas for `funder` (just-in-time funded),
/// then calls `balance::send_funds<SUI>` through
/// `TestTransactionBuilder::transfer_sui_to_address_balance`.
async fn send_sui_to_address_balance(
    cluster: &LocalCluster,
    funder: SuiAddress,
    funder_kp: &sui_types::crypto::AccountKeyPair,
    recipient: SuiAddress,
    amount: u64,
) {
    let request_gas_fx = cluster
        .request_gas(funder, DEFAULT_GAS_BUDGET + amount)
        .await
        .expect("request_gas");
    let gas = request_gas_fx
        .created()
        .into_iter()
        .find_map(|(oref, o)| matches!(o, Owner::AddressOwner(a) if a == funder).then_some(oref))
        .expect("request_gas should produce an address-owned coin for the funder");

    let tx = TestTransactionBuilder::new(funder, gas, cluster.reference_gas_price().await)
        .with_gas_budget(DEFAULT_GAS_BUDGET)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(amount, recipient)])
        .build();

    let signed = sui_types::utils::to_sender_signed_transaction(tx, funder_kp);
    let (fx, err) = cluster
        .execute_transaction(signed)
        .await
        .expect("execute_transaction");
    assert!(err.is_none(), "transfer to address balance failed: {err:?}");
    assert!(fx.status().is_ok(), "address-balance tx status not ok");
}

async fn client(cluster: &LocalCluster) -> ConsistentServiceClient<Channel> {
    ConsistentServiceClient::connect(cluster.grpc_url().to_string())
        .await
        .unwrap()
}

/// Forward `list_balances` helper, optionally pinning a
/// checkpoint header.
async fn list_balances(
    cluster: &LocalCluster,
    owner: SuiAddress,
    checkpoint: Option<u64>,
    page_size: Option<u32>,
) -> Result<Vec<(String, u64)>, tonic::Status> {
    let mut client = client(cluster).await;
    let mut request = tonic::Request::new(ListBalancesRequest {
        owner: Some(owner.to_string()),
        page_size,
        ..Default::default()
    });
    if let Some(cp) = checkpoint {
        request
            .metadata_mut()
            .insert(CHECKPOINT_HEIGHT_METADATA, cp.to_string().parse().unwrap());
    }
    let response = client.list_balances(request).await?.into_inner();
    Ok(response
        .balances
        .into_iter()
        .map(|b| (b.coin_type().to_owned(), b.total_balance()))
        .collect())
}

async fn get_balance(
    cluster: &LocalCluster,
    owner: SuiAddress,
    coin_type: &str,
    checkpoint: Option<u64>,
) -> Result<u64, tonic::Status> {
    let mut client = client(cluster).await;
    let mut request = tonic::Request::new(GetBalanceRequest {
        owner: Some(owner.to_string()),
        coin_type: Some(coin_type.to_owned()),
    });
    if let Some(cp) = checkpoint {
        request
            .metadata_mut()
            .insert(CHECKPOINT_HEIGHT_METADATA, cp.to_string().parse().unwrap());
    }
    Ok(client
        .get_balance(request)
        .await?
        .into_inner()
        .total_balance())
}

async fn batch_get_balances(
    cluster: &LocalCluster,
    requests: Vec<(SuiAddress, String)>,
    checkpoint: Option<u64>,
) -> Result<Vec<(String, u64)>, tonic::Status> {
    let mut client = client(cluster).await;
    let mut request = tonic::Request::new(BatchGetBalancesRequest {
        requests: requests
            .into_iter()
            .map(|(owner, coin_type)| GetBalanceRequest {
                owner: Some(owner.to_string()),
                coin_type: Some(coin_type),
            })
            .collect(),
    });
    if let Some(cp) = checkpoint {
        request
            .metadata_mut()
            .insert(CHECKPOINT_HEIGHT_METADATA, cp.to_string().parse().unwrap());
    }
    Ok(client
        .batch_get_balances(request)
        .await?
        .into_inner()
        .balances
        .into_iter()
        .map(|b| (b.owner().to_owned(), b.total_balance()))
        .collect())
}

/// Ports `test_index_address_balance_accumulates`: repeated
/// sends to the same address balance accumulate into a single
/// total on `list_balances` / `get_balance`.
#[tokio::test]
async fn address_balance_accumulates() {
    let _guard = accumulator_overrides();
    let cluster = LocalCluster::new().await.unwrap();
    let (a, _) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    // The funder is just a third funded account; its identity
    // doesn't matter as long as it has enough SUI to send.
    let (funder, fkp, _) = cluster
        .funded_account(DEFAULT_GAS_BUDGET * 10 + 1500)
        .await
        .unwrap();

    send_sui_to_address_balance(&cluster, funder, &fkp, a, 100).await;
    send_sui_to_address_balance(&cluster, funder, &fkp, a, 200).await;
    send_sui_to_address_balance(&cluster, funder, &fkp, a, 300).await;
    send_sui_to_address_balance(&cluster, funder, &fkp, b, 400).await;
    send_sui_to_address_balance(&cluster, funder, &fkp, b, 500).await;
    cluster.create_checkpoint().await.unwrap();

    let gas_type = GAS::type_().to_canonical_string(true);

    assert_eq!(
        get_balance(&cluster, a, &gas_type, None).await.unwrap(),
        600
    );
    assert_eq!(
        list_balances(&cluster, a, None, Some(10)).await.unwrap(),
        vec![(gas_type.clone(), 600)],
    );

    assert_eq!(
        get_balance(&cluster, b, &gas_type, None).await.unwrap(),
        900
    );
    assert_eq!(
        list_balances(&cluster, b, None, Some(10)).await.unwrap(),
        vec![(gas_type.clone(), 900)],
    );

    assert_eq!(
        batch_get_balances(
            &cluster,
            vec![(a, gas_type.clone()), (b, gas_type.clone())],
            None,
        )
        .await
        .unwrap(),
        vec![(a.to_string(), 600), (b.to_string(), 900)],
    );
}

/// Ports `test_snapshot_consistency` (address-balance variant):
/// reads at a past checkpoint anchor the address-balance total
/// to that point in time, even after further sends bump the
/// live total.
#[tokio::test]
async fn address_balance_snapshot_consistency() {
    let _guard = accumulator_overrides();
    let cluster = LocalCluster::new().await.unwrap();
    let (a, _) = get_account_key_pair();
    let (b, _) = get_account_key_pair();
    let (funder, fkp, _) = cluster
        .funded_account(DEFAULT_GAS_BUDGET * 10 + 1500)
        .await
        .unwrap();

    send_sui_to_address_balance(&cluster, funder, &fkp, a, 100).await;
    send_sui_to_address_balance(&cluster, funder, &fkp, a, 200).await;
    send_sui_to_address_balance(&cluster, funder, &fkp, b, 300).await;
    let cp1 = cluster.create_checkpoint().await.unwrap();
    let cp1 = cp1.sequence_number;

    let gas_type = GAS::type_().to_canonical_string(true);

    // Current state at cp1.
    assert_eq!(
        get_balance(&cluster, a, &gas_type, None).await.unwrap(),
        300
    );
    assert_eq!(
        get_balance(&cluster, b, &gas_type, None).await.unwrap(),
        300
    );

    send_sui_to_address_balance(&cluster, funder, &fkp, a, 400).await;
    send_sui_to_address_balance(&cluster, funder, &fkp, b, 500).await;
    cluster.create_checkpoint().await.unwrap();

    // Live: A=700, B=800.
    assert_eq!(
        get_balance(&cluster, a, &gas_type, None).await.unwrap(),
        700
    );
    assert_eq!(
        get_balance(&cluster, b, &gas_type, None).await.unwrap(),
        800
    );

    // ...but cp1 still reports the old totals.
    assert_eq!(
        get_balance(&cluster, a, &gas_type, Some(cp1))
            .await
            .unwrap(),
        300,
    );
    assert_eq!(
        get_balance(&cluster, b, &gas_type, Some(cp1))
            .await
            .unwrap(),
        300,
    );
    assert_eq!(
        list_balances(&cluster, a, Some(cp1), Some(10))
            .await
            .unwrap(),
        vec![(gas_type.clone(), 300)],
    );
    assert_eq!(
        batch_get_balances(
            &cluster,
            vec![(a, gas_type.clone()), (b, gas_type.clone())],
            Some(cp1),
        )
        .await
        .unwrap(),
        vec![(a.to_string(), 300), (b.to_string(), 300)],
    );
}
