// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Mirrors the `consistent_store_balance_tests` from
//! `sui-indexer-alt-e2e-tests` against our `LocalCluster`.

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::BatchGetBalancesRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::GetBalanceRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::ListBalancesRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_client::ConsistentServiceClient;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

const SUI_COIN_TYPE: &str =
    "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI";

async fn client(cluster: &LocalCluster) -> ConsistentServiceClient<Channel> {
    ConsistentServiceClient::connect(cluster.grpc_url().to_string())
        .await
        .unwrap()
}

#[tokio::test]
async fn get_balance_reports_funded_amount() {
    let cluster = LocalCluster::new().await.unwrap();
    let funded = 10_000_000_000u64;
    let (owner, _kp, _gas) = cluster.funded_account(funded).await.unwrap();
    cluster.create_checkpoint().await.unwrap();

    let mut svc = client(&cluster).await;

    let balance = svc
        .get_balance(GetBalanceRequest {
            owner: Some(owner.to_string()),
            coin_type: Some(SUI_COIN_TYPE.to_string()),
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(balance.owner.as_deref(), Some(&*owner.to_string()));
    assert_eq!(balance.coin_balance, Some(funded));
    // No accumulator-side contribution yet — address half is zero.
    assert_eq!(balance.address_balance, Some(0));
    assert_eq!(balance.total_balance, Some(funded));
}

#[tokio::test]
async fn get_balance_missing_owner_is_invalid_argument() {
    let cluster = LocalCluster::new().await.unwrap();
    cluster.create_checkpoint().await.unwrap();
    let mut svc = client(&cluster).await;

    let err = svc
        .get_balance(GetBalanceRequest {
            owner: None,
            coin_type: Some(SUI_COIN_TYPE.to_string()),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn batch_get_balances_returns_one_per_request() {
    let cluster = LocalCluster::new().await.unwrap();
    let funded = 10_000_000_000u64;
    let (a, _kp_a, _gas_a) = cluster.funded_account(funded).await.unwrap();
    let (b, _kp_b, _gas_b) = cluster.funded_account(funded).await.unwrap();
    cluster.create_checkpoint().await.unwrap();
    let mut svc = client(&cluster).await;

    let resp = svc
        .batch_get_balances(BatchGetBalancesRequest {
            requests: vec![
                GetBalanceRequest {
                    owner: Some(a.to_string()),
                    coin_type: Some(SUI_COIN_TYPE.to_string()),
                },
                GetBalanceRequest {
                    owner: Some(b.to_string()),
                    coin_type: Some(SUI_COIN_TYPE.to_string()),
                },
            ],
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.balances.len(), 2);
    assert_eq!(resp.balances[0].owner.as_deref(), Some(&*a.to_string()));
    assert_eq!(resp.balances[0].coin_balance, Some(funded));
    assert_eq!(resp.balances[1].owner.as_deref(), Some(&*b.to_string()));
    assert_eq!(resp.balances[1].coin_balance, Some(funded));
}

#[tokio::test]
async fn list_balances_returns_owners_holdings() {
    let cluster = LocalCluster::new().await.unwrap();
    let funded = 10_000_000_000u64;
    let (owner, _kp, _gas) = cluster.funded_account(funded).await.unwrap();
    cluster.create_checkpoint().await.unwrap();
    let mut svc = client(&cluster).await;

    let resp = svc
        .list_balances(ListBalancesRequest {
            owner: Some(owner.to_string()),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(resp.balances.len(), 1);
    let bal = &resp.balances[0];
    assert_eq!(bal.coin_type.as_deref(), Some(SUI_COIN_TYPE));
    assert_eq!(bal.coin_balance, Some(funded));
    assert!(bal.page_token.is_some());
    assert_eq!(resp.has_previous_page, Some(false));
    assert_eq!(resp.has_next_page, Some(false));
}
