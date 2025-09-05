// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::{
    consistent_service_client::ConsistentServiceClient, BatchGetBalancesRequest, GetBalanceRequest,
    ListBalancesRequest,
};
use sui_indexer_alt_e2e_tests::{find_address_owned, FullCluster};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::{
    base_types::{ObjectRef, SuiAddress},
    crypto::get_account_key_pair,
    effects::TransactionEffectsAPI,
    gas_coin::GAS,
    object::Owner,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{Transaction, TransactionData},
};

/// 5 SUI gas budget
const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

#[tokio::test]
async fn test_aggregation() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (a, _) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    // Create multiple SUI coins for address A with different amounts
    create_coin(&mut cluster, a, 1);
    create_coin(&mut cluster, a, 2);
    create_coin(&mut cluster, a, 3);

    // Create SUI coins for address B
    create_coin(&mut cluster, b, 4);
    create_coin(&mut cluster, b, 5);

    cluster.create_checkpoint().await;

    let with_prefix = true;
    let gas_type = GAS::type_().to_canonical_string(with_prefix);

    // A has a balance of (1 + 2 + 3) = 6
    assert_eq!(
        list_balances(&cluster, a, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 6)], None)
    );

    assert_eq!(
        get_balance(&cluster, a, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 6)
    );

    // B has a balance of (4 + 5) = 9
    assert_eq!(
        list_balances(&cluster, b, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 9)], None)
    );

    assert_eq!(
        get_balance(&cluster, b, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 9)
    );

    // Perform a multi-get for both addresses
    assert_eq!(
        batch_get_balances(
            &cluster,
            vec![(a, gas_type.clone()), (b, gas_type.clone())],
            None
        )
        .await
        .unwrap(),
        vec![
            (a.to_string(), gas_type.clone(), 6),
            (b.to_string(), gas_type.clone(), 9)
        ]
    )
}

#[tokio::test]
async fn test_multiple_coin_types() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (p, pkp, gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET)
        .expect("Failed to fund publisher account");

    // Publish the custom coin package (which also mints coins)
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["packages", "coin"]);

    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(
            TestTransactionBuilder::new(p, gas, 1000)
                .with_gas_budget(DEFAULT_GAS_BUDGET)
                .publish(path)
                .build(),
            vec![&pkp],
        ))
        .expect("Failed to execute publish transaction");
    cluster.create_checkpoint().await;

    let pkg = fx
        .created()
        .into_iter()
        .find_map(|((pkg, v, _), owner)| {
            (v.value() == 1 && matches!(owner, Owner::Immutable)).then_some(pkg)
        })
        .expect("Failed to find package ID");

    let sui_balance = DEFAULT_GAS_BUDGET as i64 - fx.gas_cost_summary().net_gas_usage();

    let has_prefix = true;
    let gas_type = GAS::type_().to_canonical_string(has_prefix);
    let my_coin_type = format!("{}::my_coin::MY_COIN", pkg.to_canonical_display(has_prefix));

    // P's balances should include SUI (left over from the gas coin) and MY_COIN (1000 + 200 + 30)
    // = 1230 from minting during publish and init.
    let mut balances = vec![
        (gas_type.clone(), sui_balance as u64),
        (my_coin_type.clone(), 1230),
    ];

    // Balance output will be sorted by coin type -- sorting by the string representation will be
    // sufficient for this test.
    balances.sort();
    assert_eq!(
        list_balances(&cluster, p, None, None, Some(10))
            .await
            .unwrap(),
        (balances, None)
    );

    assert_eq!(
        get_balance(&cluster, p, &gas_type, None).await.unwrap(),
        (gas_type.clone(), sui_balance as u64)
    );

    assert_eq!(
        get_balance(&cluster, p, &my_coin_type, None).await.unwrap(),
        (my_coin_type.clone(), 1230)
    );

    assert_eq!(
        batch_get_balances(
            &cluster,
            vec![(p, my_coin_type.clone()), (p, gas_type.clone())],
            None
        )
        .await
        .unwrap(),
        vec![
            (p.to_string(), my_coin_type.clone(), 1230),
            (p.to_string(), gas_type.clone(), sui_balance as u64),
        ]
    );
}

#[tokio::test]
async fn test_snapshot_consistency() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (a, _) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    // Create initial coins
    create_coin(&mut cluster, a, 1);
    create_coin(&mut cluster, a, 2);
    create_coin(&mut cluster, b, 3);
    cluster.create_checkpoint().await;

    let with_prefix = true;
    let gas_type = GAS::type_().to_canonical_string(with_prefix);

    // A has a balance of (1 + 2) = 3
    assert_eq!(
        list_balances(&cluster, a, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 3)], None)
    );

    assert_eq!(
        get_balance(&cluster, a, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 3)
    );

    // B has a balance of 3
    assert_eq!(
        list_balances(&cluster, b, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 3)], None)
    );

    assert_eq!(
        get_balance(&cluster, b, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 3)
    );

    assert_eq!(
        batch_get_balances(
            &cluster,
            vec![(a, gas_type.clone()), (b, gas_type.clone())],
            None
        )
        .await
        .unwrap(),
        vec![
            (a.to_string(), gas_type.clone(), 3),
            (b.to_string(), gas_type.clone(), 3)
        ]
    );

    // Add more coins
    create_coin(&mut cluster, a, 4);
    create_coin(&mut cluster, b, 5);
    cluster.create_checkpoint().await;

    // A now has a balance of (1 + 2 + 4) = 7
    assert_eq!(
        list_balances(&cluster, a, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 7)], None)
    );

    assert_eq!(
        get_balance(&cluster, a, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 7)
    );

    // B now has a balance of (3 + 5) = 8
    assert_eq!(
        list_balances(&cluster, b, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 8)], None)
    );

    assert_eq!(
        get_balance(&cluster, b, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 8)
    );

    assert_eq!(
        batch_get_balances(
            &cluster,
            vec![(a, gas_type.clone()), (b, gas_type.clone())],
            None
        )
        .await
        .unwrap(),
        vec![
            (a.to_string(), gas_type.clone(), 7),
            (b.to_string(), gas_type.clone(), 8)
        ]
    );

    // The data from checkpoint 1 is still available
    assert_eq!(
        list_balances(&cluster, a, Some(1), None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 3)], None)
    );

    assert_eq!(
        get_balance(&cluster, a, &gas_type, Some(1)).await.unwrap(),
        (gas_type.clone(), 3)
    );

    assert_eq!(
        list_balances(&cluster, b, Some(1), None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 3)], None)
    );

    assert_eq!(
        get_balance(&cluster, b, &gas_type, Some(1)).await.unwrap(),
        (gas_type.clone(), 3)
    );

    assert_eq!(
        batch_get_balances(
            &cluster,
            vec![(a, gas_type.clone()), (b, gas_type.clone())],
            Some(1)
        )
        .await
        .unwrap(),
        vec![
            (a.to_string(), gas_type.clone(), 3),
            (b.to_string(), gas_type.clone(), 3)
        ]
    );
}

#[tokio::test]
async fn test_transfers() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (a, akp) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    // Fund A with gas
    let mut gas_budget = DEFAULT_GAS_BUDGET;
    let mut a_gas = find_address_owned(
        &cluster
            .request_gas(a, gas_budget)
            .expect("Failed to request gas"),
    )
    .expect("Failed to find gas object");
    cluster.create_checkpoint().await;

    let with_prefix = true;
    let gas_type = GAS::type_().to_canonical_string(with_prefix);

    // A has a balance
    assert_eq!(
        list_balances(&cluster, a, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), gas_budget)], None)
    );

    assert_eq!(
        get_balance(&cluster, a, &gas_type, None).await.unwrap(),
        (gas_type.clone(), gas_budget)
    );

    // B does not
    assert_eq!(
        list_balances(&cluster, b, None, None, Some(10))
            .await
            .unwrap(),
        (vec![], None)
    );

    assert_eq!(
        get_balance(&cluster, b, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 0)
    );

    assert_eq!(
        batch_get_balances(
            &cluster,
            vec![(a, gas_type.clone()), (b, gas_type.clone())],
            None
        )
        .await
        .unwrap(),
        vec![
            (a.to_string(), gas_type.clone(), gas_budget),
            (b.to_string(), gas_type.clone(), 0)
        ]
    );

    // Split off some of A's gas and transfer it to B
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(b, Some(1000));

    let data = TransactionData::new_programmable(
        a,
        vec![a_gas],
        builder.finish(),
        gas_budget - 1000,
        cluster.reference_gas_price(),
    );

    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&akp]))
        .expect("Failed to execute split transaction");
    cluster.create_checkpoint().await;

    gas_budget = (gas_budget as i64 - 1000 - fx.gas_cost_summary().net_gas_usage()) as u64;
    a_gas = fx.gas_object().0;

    // A still controls the budget
    assert_eq!(
        list_balances(&cluster, a, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), gas_budget)], None)
    );

    assert_eq!(
        get_balance(&cluster, a, &gas_type, None).await.unwrap(),
        (gas_type.clone(), gas_budget)
    );

    // B has been given some of it
    assert_eq!(
        list_balances(&cluster, b, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 1000)], None)
    );

    assert_eq!(
        get_balance(&cluster, b, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 1000)
    );

    assert_eq!(
        batch_get_balances(
            &cluster,
            vec![(a, gas_type.clone()), (b, gas_type.clone())],
            None
        )
        .await
        .unwrap(),
        vec![
            (a.to_string(), gas_type.clone(), gas_budget),
            (b.to_string(), gas_type.clone(), 1000)
        ]
    );

    // Send the gas coin from A to B
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(b, None);

    let data = TransactionData::new_programmable(
        a,
        vec![a_gas],
        builder.finish(),
        gas_budget,
        cluster.reference_gas_price(),
    );

    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&akp]))
        .expect("Failed to execute split transaction");
    cluster.create_checkpoint().await;

    gas_budget = (gas_budget as i64 + 1000 - fx.gas_cost_summary().net_gas_usage()) as u64;

    // A has no gas left, so it should return no balance records.
    assert_eq!(
        list_balances(&cluster, a, None, None, Some(10))
            .await
            .unwrap(),
        (vec![], None)
    );

    assert_eq!(
        get_balance(&cluster, a, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 0)
    );

    // B controls the budget now
    assert_eq!(
        list_balances(&cluster, b, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), gas_budget)], None)
    );

    assert_eq!(
        get_balance(&cluster, b, &gas_type, None).await.unwrap(),
        (gas_type.clone(), gas_budget)
    );

    assert_eq!(
        batch_get_balances(
            &cluster,
            vec![(a, gas_type.clone()), (b, gas_type.clone())],
            None
        )
        .await
        .unwrap(),
        vec![
            (a.to_string(), gas_type.clone(), 0),
            (b.to_string(), gas_type.clone(), gas_budget)
        ]
    );
}

#[tokio::test]
async fn test_edge_cases() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (a, _) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    // Create a coin for A
    create_coin(&mut cluster, a, 1);
    cluster.create_checkpoint().await;

    // Querying for an address with no coins should return an empty list
    assert_eq!(
        list_balances(&cluster, b, None, None, Some(10))
            .await
            .unwrap(),
        (vec![], None)
    );

    // Missing owner parameter
    let mut client = ConsistentServiceClient::connect(cluster.consistent_store_url().to_string())
        .await
        .expect("Failed to connect to Consistent Store");

    let request = tonic::Request::new(ListBalancesRequest {
        owner: None,
        page_size: Some(10),
        ..Default::default()
    });

    let err = client.list_balances(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(err.message(), "Missing 'owner'");

    let request = tonic::Request::new(GetBalanceRequest {
        owner: None,
        coin_type: Some(GAS::type_().to_string()),
    });

    let err = client.get_balance(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(err.message(), "Missing 'owner'");

    let request = tonic::Request::new(BatchGetBalancesRequest {
        requests: vec![
            GetBalanceRequest {
                owner: Some(a.to_string()),
                coin_type: Some(GAS::type_().to_string()),
            },
            GetBalanceRequest {
                owner: None,
                coin_type: Some(GAS::type_().to_string()),
            },
        ],
    });

    let err = client.batch_get_balances(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(err.message(), "Missing 'owner'");

    // Invalid owner address
    let request = tonic::Request::new(ListBalancesRequest {
        owner: Some("invalid_address".to_string()),
        page_size: Some(10),
        ..Default::default()
    });

    let err = client.list_balances(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(err.message(), r#"Invalid 'owner': "invalid_address""#);

    let request = tonic::Request::new(GetBalanceRequest {
        owner: Some("invalid_address".to_string()),
        coin_type: Some(GAS::type_().to_string()),
    });

    let err = client.get_balance(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(err.message(), r#"Invalid 'owner': "invalid_address""#);

    let request = tonic::Request::new(BatchGetBalancesRequest {
        requests: vec![
            GetBalanceRequest {
                owner: Some("invalid_address".to_string()),
                coin_type: Some(GAS::type_().to_string()),
            },
            GetBalanceRequest {
                owner: Some(b.to_string()),
                coin_type: Some(GAS::type_().to_string()),
            },
        ],
    });

    let err = client.batch_get_balances(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(err.message(), r#"Invalid 'owner': "invalid_address""#);

    // Missing coin type
    let request = tonic::Request::new(GetBalanceRequest {
        owner: Some(a.to_string()),
        coin_type: None,
    });

    let err = client.get_balance(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(err.message(), "Missing 'coin_type'");

    let request = tonic::Request::new(BatchGetBalancesRequest {
        requests: vec![
            GetBalanceRequest {
                owner: Some(a.to_string()),
                coin_type: None,
            },
            GetBalanceRequest {
                owner: Some(b.to_string()),
                coin_type: Some(GAS::type_().to_string()),
            },
        ],
    });

    let err = client.batch_get_balances(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(err.message(), "Missing 'coin_type'");

    // Invalid coin type
    let request = tonic::Request::new(GetBalanceRequest {
        owner: Some(a.to_string()),
        coin_type: Some("invalid_coin_type".to_string()),
    });

    let err = client.get_balance(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(err.message(), r#"Invalid 'coin_type': "invalid_coin_type""#);

    let request = tonic::Request::new(BatchGetBalancesRequest {
        requests: vec![
            GetBalanceRequest {
                owner: Some(a.to_string()),
                coin_type: Some(GAS::type_().to_string()),
            },
            GetBalanceRequest {
                owner: Some(b.to_string()),
                coin_type: Some("invalid_coin_type".to_string()),
            },
        ],
    });

    let err = client.batch_get_balances(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(err.message(), r#"Invalid 'coin_type': "invalid_coin_type""#);

    // Not in range
    let mut request = tonic::Request::new(ListBalancesRequest {
        owner: Some(a.to_string()),
        ..Default::default()
    });

    request
        .metadata_mut()
        .insert("x-sui-checkpoint", "10".parse().unwrap());

    let err = client.list_balances(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::OutOfRange);
    assert_eq!(err.message(), "Checkpoint 10 not in the consistent range");

    let mut request = tonic::Request::new(GetBalanceRequest {
        owner: Some(a.to_string()),
        coin_type: Some(GAS::type_().to_string()),
    });

    request
        .metadata_mut()
        .insert("x-sui-checkpoint", "10".parse().unwrap());

    let err = client.get_balance(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::OutOfRange);
    assert_eq!(err.message(), "Checkpoint 10 not in the consistent range");

    let mut request = tonic::Request::new(BatchGetBalancesRequest {
        requests: vec![
            GetBalanceRequest {
                owner: Some(a.to_string()),
                coin_type: Some(GAS::type_().to_string()),
            },
            GetBalanceRequest {
                owner: Some(b.to_string()),
                coin_type: Some(GAS::type_().to_string()),
            },
        ],
    });

    request
        .metadata_mut()
        .insert("x-sui-checkpoint", "10".parse().unwrap());

    let err = client.batch_get_balances(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::OutOfRange);
    assert_eq!(err.message(), "Checkpoint 10 not in the consistent range");
}

/// Run a transaction on `cluster` signed by a fresh funded account that sends a coin with value
/// `amount` to `owner`.
fn create_coin(cluster: &mut FullCluster, owner: SuiAddress, amount: u64) -> ObjectRef {
    let (sender, kp, gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET + amount)
        .expect("Failed to fund account");

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(owner, Some(amount));

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );

    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
        .expect("Failed to execute transaction");

    assert!(fx.status().is_ok(), "create coin transaction failed");
    sui_indexer_alt_e2e_tests::find_address_owned(&fx).expect("Failed to find created coin")
}

/// Helper to perform forward pagination over balances.
async fn list_balances(
    cluster: &FullCluster,
    owner: SuiAddress,
    checkpoint: Option<u64>,
    after_token: Option<Vec<u8>>,
    page_size: Option<u32>,
) -> Result<(Vec<(String, u64)>, Option<Vec<u8>>), tonic::Status> {
    let mut client = ConsistentServiceClient::connect(cluster.consistent_store_url().to_string())
        .await
        .expect("Failed to connect to Consistent Store");

    let owner = owner.to_string();
    let mut request = tonic::Request::new(ListBalancesRequest {
        owner: Some(owner.clone()),
        page_size,
        after_token: after_token.map(Into::into),
        ..Default::default()
    });

    if let Some(checkpoint) = checkpoint {
        request
            .metadata_mut()
            .insert("x-sui-checkpoint", checkpoint.to_string().parse().unwrap());
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
            assert_eq!(b.owner(), &owner, "Owner mismatch in balance response");
            (b.coin_type().to_owned(), b.balance())
        })
        .collect();

    Ok((balances, after_token))
}

/// Helper to perform a single balance lookup
async fn get_balance(
    cluster: &FullCluster,
    owner: SuiAddress,
    coin_type: &str,
    checkpoint: Option<u64>,
) -> Result<(String, u64), tonic::Status> {
    let mut client = ConsistentServiceClient::connect(cluster.consistent_store_url().to_string())
        .await
        .expect("Failed to connect to Consistent Store");

    let owner = owner.to_string();
    let mut request = tonic::Request::new(GetBalanceRequest {
        owner: Some(owner.clone()),
        coin_type: Some(coin_type.to_owned()),
    });

    if let Some(checkpoint) = checkpoint {
        request
            .metadata_mut()
            .insert("x-sui-checkpoint", checkpoint.to_string().parse().unwrap());
    }

    let response = client.get_balance(request).await?.into_inner();

    assert_eq!(
        response.owner(),
        &owner,
        "Owner mismatch in balance response"
    );

    Ok((response.coin_type().to_owned(), response.balance()))
}

async fn batch_get_balances(
    cluster: &FullCluster,
    requests: Vec<(SuiAddress, String)>,
    checkpoint: Option<u64>,
) -> Result<Vec<(String, String, u64)>, tonic::Status> {
    let mut client = ConsistentServiceClient::connect(cluster.consistent_store_url().to_string())
        .await
        .expect("Failed to connect to Consistent Store");

    let mut request = tonic::Request::new(BatchGetBalancesRequest {
        requests: requests
            .into_iter()
            .map(|(owner, coin_type)| GetBalanceRequest {
                owner: Some(owner.to_string()),
                coin_type: Some(coin_type),
            })
            .collect(),
    });

    if let Some(checkpoint) = checkpoint {
        request
            .metadata_mut()
            .insert("x-sui-checkpoint", checkpoint.to_string().parse().unwrap());
    }

    Ok(client
        .batch_get_balances(request)
        .await?
        .into_inner()
        .balances
        .into_iter()
        .map(|b| (b.owner().to_owned(), b.coin_type().to_owned(), b.balance()))
        .collect())
}
