// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Ports the read-only subset of
//! `sui-e2e-tests/tests/rpc/v2/state_service/balance.rs`. We
//! skip:
//!
//! - the address-balance accumulator tests, which need
//!   `ProtocolConfig::apply_overrides_for_testing` to enable
//!   accumulators (process-global state; doesn't play with the
//!   per-test shared Simulacrum); and
//! - the multi-address coin tests, which depend on
//!   `TestClusterBuilder`'s implicit funding of `address_{0..N}`
//!   that Simulacrum doesn't replicate.

use sui_rpc::proto::sui::rpc::v2::GetBalanceRequest;
use sui_rpc::proto::sui::rpc::v2::ListBalancesRequest;
use sui_rpc::proto::sui::rpc::v2::state_service_client::StateServiceClient;
use sui_types::base_types::SuiAddress;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

const SUI_COIN_TYPE: &str =
    "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI";

async fn state_client(cluster: &LocalCluster) -> StateServiceClient<Channel> {
    StateServiceClient::connect(cluster.grpc_url().to_string())
        .await
        .unwrap()
}

/// `get_balance` against an address with no coins returns
/// `balance = 0`, and `list_balances` returns an empty list.
#[tokio::test]
async fn fresh_address_returns_zero_balance() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut client = state_client(&cluster).await;

    let fresh = SuiAddress::random_for_testing_only();

    let response = client
        .get_balance({
            let mut req = GetBalanceRequest::default();
            req.owner = Some(fresh.to_string());
            req.coin_type = Some(SUI_COIN_TYPE.to_string());
            req
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.balance.unwrap().balance.unwrap(), 0);

    let list_response = client
        .list_balances({
            let mut req = ListBalancesRequest::default();
            req.owner = Some(fresh.to_string());
            req
        })
        .await
        .unwrap()
        .into_inner();
    assert!(list_response.balances.is_empty());
    assert!(list_response.next_page_token.is_none());
}

/// After Simulacrum funds an account, the SUI balance reported
/// over the RPC matches the funded amount.
#[tokio::test]
async fn funded_account_balance_reflects_initial_grant() {
    let cluster = LocalCluster::new().await.unwrap();
    let funded = 10_000_000_000u64;
    let (address, _kp, _gas) = cluster.funded_account(funded).await.unwrap();
    cluster.create_checkpoint().await.unwrap();

    let mut client = state_client(&cluster).await;

    let balance = client
        .get_balance({
            let mut req = GetBalanceRequest::default();
            req.owner = Some(address.to_string());
            req.coin_type = Some(SUI_COIN_TYPE.to_string());
            req
        })
        .await
        .unwrap()
        .into_inner()
        .balance
        .unwrap();
    assert_eq!(
        balance.balance.unwrap(),
        funded,
        "SUI balance for funded account should be the requested amount",
    );
    assert_eq!(balance.coin_balance.unwrap(), funded);
    assert_eq!(
        balance.coin_type.as_deref(),
        Some(SUI_COIN_TYPE),
        "coin_type should round-trip",
    );

    let list = client
        .list_balances({
            let mut req = ListBalancesRequest::default();
            req.owner = Some(address.to_string());
            req
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        list.balances.len(),
        1,
        "funded account should hold exactly one coin type",
    );
    assert_eq!(list.balances[0], balance);
}

/// The InvalidArgument-coded error paths from the e2e test:
/// missing owner, missing coin_type, malformed owner, malformed
/// coin_type, and a corrupted page_token.
#[tokio::test]
async fn invalid_requests_surface_invalid_argument() {
    let cluster = LocalCluster::new().await.unwrap();
    let (address, _kp, _gas) = cluster.funded_account(10_000_000_000).await.unwrap();
    cluster.create_checkpoint().await.unwrap();

    let mut client = state_client(&cluster).await;

    // Missing owner.
    let err = client
        .get_balance({
            let mut req = GetBalanceRequest::default();
            req.coin_type = Some(SUI_COIN_TYPE.to_string());
            req
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(
        err.message().contains("missing owner"),
        "unexpected error message: {}",
        err.message(),
    );

    // Missing coin_type.
    let err = client
        .get_balance({
            let mut req = GetBalanceRequest::default();
            req.owner = Some(address.to_string());
            req
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(
        err.message().contains("missing coin_type"),
        "unexpected error message: {}",
        err.message(),
    );

    // Invalid address.
    let err = client
        .get_balance({
            let mut req = GetBalanceRequest::default();
            req.owner = Some("not_a_hex_address".to_string());
            req.coin_type = Some(SUI_COIN_TYPE.to_string());
            req
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(
        err.message().contains("invalid owner"),
        "unexpected error message: {}",
        err.message(),
    );

    // Invalid coin type.
    let err = client
        .get_balance({
            let mut req = GetBalanceRequest::default();
            req.owner = Some(address.to_string());
            req.coin_type = Some("invalid::coin::type::format".to_string());
            req
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(
        err.message().contains("invalid coin_type"),
        "unexpected error message: {}",
        err.message(),
    );

    // `list_balances` missing owner.
    let err = client
        .list_balances(ListBalancesRequest::default())
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(
        err.message().contains("missing owner"),
        "unexpected error message: {}",
        err.message(),
    );

    // Corrupt page token.
    let err = client
        .list_balances({
            let mut req = ListBalancesRequest::default();
            req.owner = Some(address.to_string());
            req.page_token = Some(vec![0xFF, 0xDE, 0xAD, 0xBE, 0xEF].into());
            req
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}
