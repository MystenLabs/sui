// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Richer scenarios mirroring
//! `sui-indexer-alt-e2e-tests/tests/consistent_store_balance_tests.rs`.
//!
//! Skipped from this port:
//!
//! - `test_multiple_coin_types` — publishes a custom Move coin
//!   package on disk (`crates/sui-indexer-alt-e2e-tests/packages/coin`).
//!   Porting it needs the in-process `sui-move-build` plumbing
//!   we don't wire up here.

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::BatchGetBalancesRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::CHECKPOINT_HEIGHT_METADATA;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::GetBalanceRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::ListBalancesRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_client::ConsistentServiceClient;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::get_account_key_pair;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::gas_coin::GAS;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

/// 5 SUI gas budget — matches the alt-consistent-store tests.
const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

async fn client(cluster: &LocalCluster) -> ConsistentServiceClient<Channel> {
    ConsistentServiceClient::connect(cluster.grpc_url().to_string())
        .await
        .unwrap()
}

/// Drive Simulacrum to fund a fresh account, then transfer
/// `amount` SUI from that account to `owner`. Returns the new
/// address-owned coin's reference. Mirrors
/// `create_coin` in the e2e helpers.
async fn create_coin(cluster: &LocalCluster, owner: SuiAddress, amount: u64) -> ObjectRef {
    let (sender, kp, gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET + amount)
        .await
        .expect("funded_account");

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(owner, Some(amount));

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price().await,
    );

    let (fx, err) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
        .await
        .expect("execute_transaction");
    assert!(err.is_none(), "create_coin failed: {err:?}");
    assert!(fx.status().is_ok(), "create_coin tx status not ok");

    fx.created()
        .into_iter()
        .find_map(|(oref, o)| {
            matches!(o, Owner::AddressOwner(addr) if addr == owner).then_some(oref)
        })
        .expect("created an address-owned coin for the recipient")
}

/// Forward-only `list_balances` with optional checkpoint header.
async fn list_balances(
    cluster: &LocalCluster,
    owner: SuiAddress,
    checkpoint: Option<u64>,
    after_token: Option<Vec<u8>>,
    page_size: Option<u32>,
) -> Result<(Vec<(String, u64)>, Option<Vec<u8>>), tonic::Status> {
    let mut client = client(cluster).await;
    let owner_str = owner.to_string();

    let mut request = tonic::Request::new(ListBalancesRequest {
        owner: Some(owner_str.clone()),
        page_size,
        after_token: after_token.map(Into::into),
        ..Default::default()
    });

    if let Some(cp) = checkpoint {
        request
            .metadata_mut()
            .insert(CHECKPOINT_HEIGHT_METADATA, cp.to_string().parse().unwrap());
    }

    let response = client.list_balances(request).await?.into_inner();

    let after_token = response
        .has_next_page()
        .then(|| response.balances.last().map(|b| b.page_token().to_owned()))
        .flatten();

    let balances = response
        .balances
        .into_iter()
        .map(|b| {
            assert_eq!(b.owner(), &owner_str);
            (b.coin_type().to_owned(), b.total_balance())
        })
        .collect();

    Ok((balances, after_token))
}

async fn get_balance(
    cluster: &LocalCluster,
    owner: SuiAddress,
    coin_type: &str,
    checkpoint: Option<u64>,
) -> Result<(String, u64), tonic::Status> {
    let mut client = client(cluster).await;
    let owner_str = owner.to_string();
    let mut request = tonic::Request::new(GetBalanceRequest {
        owner: Some(owner_str.clone()),
        coin_type: Some(coin_type.to_owned()),
    });
    if let Some(cp) = checkpoint {
        request
            .metadata_mut()
            .insert(CHECKPOINT_HEIGHT_METADATA, cp.to_string().parse().unwrap());
    }
    let response = client.get_balance(request).await?.into_inner();
    assert_eq!(response.owner(), &owner_str);
    Ok((response.coin_type().to_owned(), response.total_balance()))
}

async fn batch_get_balances(
    cluster: &LocalCluster,
    requests: Vec<(SuiAddress, String)>,
    checkpoint: Option<u64>,
) -> Result<Vec<(String, String, u64)>, tonic::Status> {
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
        .map(|b| {
            (
                b.owner().to_owned(),
                b.coin_type().to_owned(),
                b.total_balance(),
            )
        })
        .collect())
}

/// Ports `test_aggregation`: multiple SUI coins owned by the
/// same address aggregate into a single `Balance` row.
#[tokio::test]
async fn aggregation_across_multiple_coins() {
    let cluster = LocalCluster::new().await.unwrap();
    let (a, _) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    create_coin(&cluster, a, 1).await;
    create_coin(&cluster, a, 2).await;
    create_coin(&cluster, a, 3).await;
    create_coin(&cluster, b, 4).await;
    create_coin(&cluster, b, 5).await;
    cluster.create_checkpoint().await.unwrap();

    let with_prefix = true;
    let gas_type = GAS::type_().to_canonical_string(with_prefix);

    assert_eq!(
        list_balances(&cluster, a, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 6)], None),
    );
    assert_eq!(
        get_balance(&cluster, a, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 6),
    );
    assert_eq!(
        list_balances(&cluster, b, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 9)], None),
    );
    assert_eq!(
        get_balance(&cluster, b, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 9),
    );
    assert_eq!(
        batch_get_balances(
            &cluster,
            vec![(a, gas_type.clone()), (b, gas_type.clone())],
            None,
        )
        .await
        .unwrap(),
        vec![
            (a.to_string(), gas_type.clone(), 6),
            (b.to_string(), gas_type.clone(), 9),
        ],
    );
}

/// Ports `test_snapshot_consistency`: a balance read at a past
/// checkpoint stays anchored to that checkpoint even after
/// subsequent transactions change the live state.
#[tokio::test]
async fn snapshot_consistency_at_past_checkpoint() {
    let cluster = LocalCluster::new().await.unwrap();
    let (a, _) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    create_coin(&cluster, a, 1).await;
    create_coin(&cluster, a, 2).await;
    create_coin(&cluster, b, 3).await;
    let cp1 = cluster.create_checkpoint().await.unwrap();

    let with_prefix = true;
    let gas_type = GAS::type_().to_canonical_string(with_prefix);

    // Current state at cp1: A=3, B=3.
    assert_eq!(
        get_balance(&cluster, a, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 3),
    );
    assert_eq!(
        get_balance(&cluster, b, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 3),
    );

    create_coin(&cluster, a, 4).await;
    create_coin(&cluster, b, 5).await;
    cluster.create_checkpoint().await.unwrap();

    // Latest: A=7, B=8.
    assert_eq!(
        get_balance(&cluster, a, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 7),
    );
    assert_eq!(
        get_balance(&cluster, b, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 8),
    );

    // ...but reads anchored at cp1 still see the old totals.
    let cp1_seq = cp1.sequence_number;
    assert_eq!(
        get_balance(&cluster, a, &gas_type, Some(cp1_seq))
            .await
            .unwrap(),
        (gas_type.clone(), 3),
    );
    assert_eq!(
        get_balance(&cluster, b, &gas_type, Some(cp1_seq))
            .await
            .unwrap(),
        (gas_type.clone(), 3),
    );
    assert_eq!(
        list_balances(&cluster, a, Some(cp1_seq), None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 3)], None),
    );
    assert_eq!(
        batch_get_balances(
            &cluster,
            vec![(a, gas_type.clone()), (b, gas_type.clone())],
            Some(cp1_seq),
        )
        .await
        .unwrap(),
        vec![
            (a.to_string(), gas_type.clone(), 3),
            (b.to_string(), gas_type.clone(), 3),
        ],
    );
}

/// Ports `test_transfers`: a SUI transfer between accounts moves
/// the balance from sender to recipient on both APIs, and
/// emptying out an account leaves it with no balance rows.
#[tokio::test]
async fn transfers_move_balance_between_accounts() {
    let cluster = LocalCluster::new().await.unwrap();
    let (a, akp) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    // Fund A; B starts empty.
    let mut gas_budget = DEFAULT_GAS_BUDGET;
    let request_gas_fx = cluster.request_gas(a, gas_budget).await.unwrap();
    let mut a_gas = request_gas_fx
        .created()
        .into_iter()
        .find_map(|(oref, o)| matches!(o, Owner::AddressOwner(addr) if addr == a).then_some(oref))
        .expect("request_gas should produce an address-owned coin");
    cluster.create_checkpoint().await.unwrap();

    let with_prefix = true;
    let gas_type = GAS::type_().to_canonical_string(with_prefix);

    assert_eq!(
        list_balances(&cluster, a, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), gas_budget)], None),
    );
    assert_eq!(
        list_balances(&cluster, b, None, None, Some(10))
            .await
            .unwrap(),
        (vec![], None),
    );
    assert_eq!(
        get_balance(&cluster, b, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 0),
    );

    // A→B partial transfer: split off 1000 mist.
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(b, Some(1000));
    let data = TransactionData::new_programmable(
        a,
        vec![a_gas],
        builder.finish(),
        gas_budget - 1000,
        cluster.reference_gas_price().await,
    );
    let (fx, err) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&akp]))
        .await
        .unwrap();
    assert!(err.is_none(), "partial transfer failed: {err:?}");
    cluster.create_checkpoint().await.unwrap();

    gas_budget = (gas_budget as i64 - 1000 - fx.gas_cost_summary().net_gas_usage()) as u64;
    a_gas = fx.gas_object().unwrap().0;

    assert_eq!(
        get_balance(&cluster, a, &gas_type, None).await.unwrap(),
        (gas_type.clone(), gas_budget),
    );
    assert_eq!(
        get_balance(&cluster, b, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 1000),
    );

    // A→B full transfer: hand B the entire remaining gas coin.
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(b, None);
    let data = TransactionData::new_programmable(
        a,
        vec![a_gas],
        builder.finish(),
        gas_budget,
        cluster.reference_gas_price().await,
    );
    let (fx, err) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&akp]))
        .await
        .unwrap();
    assert!(err.is_none(), "full transfer failed: {err:?}");
    cluster.create_checkpoint().await.unwrap();
    gas_budget = (gas_budget as i64 + 1000 - fx.gas_cost_summary().net_gas_usage()) as u64;

    // A is now empty.
    assert_eq!(
        list_balances(&cluster, a, None, None, Some(10))
            .await
            .unwrap(),
        (vec![], None),
    );
    assert_eq!(
        get_balance(&cluster, a, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 0),
    );
    // B holds the remainder.
    assert_eq!(
        get_balance(&cluster, b, &gas_type, None).await.unwrap(),
        (gas_type.clone(), gas_budget),
    );
}

/// Ports the cross-method coverage from `test_edge_cases` —
/// missing owner / invalid owner / missing coin_type / invalid
/// coin_type / out-of-range checkpoint surfaced consistently
/// across `list_balances`, `get_balance`, and
/// `batch_get_balances`.
#[tokio::test]
async fn edge_cases_uniform_errors_across_methods() {
    let cluster = LocalCluster::new().await.unwrap();
    let (a, _) = get_account_key_pair();
    let (b, _) = get_account_key_pair();
    create_coin(&cluster, a, 1).await;
    cluster.create_checkpoint().await.unwrap();

    let mut svc = client(&cluster).await;
    let gas_type = GAS::type_().to_string();

    // ---- Missing owner ----
    let err = svc
        .list_balances(ListBalancesRequest {
            page_size: Some(10),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    let err = svc
        .get_balance(GetBalanceRequest {
            owner: None,
            coin_type: Some(gas_type.clone()),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    let err = svc
        .batch_get_balances(BatchGetBalancesRequest {
            requests: vec![
                GetBalanceRequest {
                    owner: Some(a.to_string()),
                    coin_type: Some(gas_type.clone()),
                },
                GetBalanceRequest {
                    owner: None,
                    coin_type: Some(gas_type.clone()),
                },
            ],
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    // ---- Invalid owner ----
    let err = svc
        .list_balances(ListBalancesRequest {
            owner: Some("invalid_address".to_string()),
            page_size: Some(10),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    let err = svc
        .get_balance(GetBalanceRequest {
            owner: Some("invalid_address".to_string()),
            coin_type: Some(gas_type.clone()),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    let err = svc
        .batch_get_balances(BatchGetBalancesRequest {
            requests: vec![GetBalanceRequest {
                owner: Some("invalid_address".to_string()),
                coin_type: Some(gas_type.clone()),
            }],
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    // ---- Missing / invalid coin_type ----
    let err = svc
        .get_balance(GetBalanceRequest {
            owner: Some(a.to_string()),
            coin_type: None,
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    let err = svc
        .get_balance(GetBalanceRequest {
            owner: Some(a.to_string()),
            coin_type: Some("invalid_coin_type".to_string()),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    // ---- Out-of-range checkpoint ----
    let mut request = tonic::Request::new(ListBalancesRequest {
        owner: Some(a.to_string()),
        ..Default::default()
    });
    request
        .metadata_mut()
        .insert(CHECKPOINT_HEIGHT_METADATA, "10".parse().unwrap());
    let err = svc.list_balances(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::OutOfRange);

    let mut request = tonic::Request::new(GetBalanceRequest {
        owner: Some(a.to_string()),
        coin_type: Some(gas_type.clone()),
    });
    request
        .metadata_mut()
        .insert(CHECKPOINT_HEIGHT_METADATA, "10".parse().unwrap());
    let err = svc.get_balance(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::OutOfRange);

    let mut request = tonic::Request::new(BatchGetBalancesRequest {
        requests: vec![
            GetBalanceRequest {
                owner: Some(a.to_string()),
                coin_type: Some(gas_type.clone()),
            },
            GetBalanceRequest {
                owner: Some(b.to_string()),
                coin_type: Some(gas_type),
            },
        ],
    });
    request
        .metadata_mut()
        .insert(CHECKPOINT_HEIGHT_METADATA, "10".parse().unwrap());
    let err = svc.batch_get_balances(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::OutOfRange);

    // ---- Empty owner returns empty list, not an error ----
    assert_eq!(
        list_balances(&cluster, b, None, None, Some(10))
            .await
            .unwrap(),
        (vec![], None),
    );
}
