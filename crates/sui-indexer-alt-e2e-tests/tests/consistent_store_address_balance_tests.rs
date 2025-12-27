// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::{
    BatchGetAddressBalancesRequest, GetAddressBalanceRequest, ListAddressBalancesRequest,
    consistent_service_client::ConsistentServiceClient,
};
use sui_indexer_alt_e2e_tests::{FullCluster, find};
use sui_test_transaction_builder::{FundSource, TestTransactionBuilder};
use sui_types::{
    TypeTag,
    base_types::SuiAddress,
    crypto::{AccountKeyPair, get_account_key_pair},
    effects::TransactionEffectsAPI,
    gas_coin::GAS,
    object::Owner,
    transaction::Transaction,
};

/// 5 SUI gas budget
const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

// =============================================================================
// Test Helpers for Address Balances
// =============================================================================

/// Send SUI from a fresh funded account to a recipient's address balance.
/// This uses `coin::send_funds` which triggers the accumulator system.
fn send_to_address_balance(cluster: &mut FullCluster, recipient: SuiAddress, amount: u64) {
    let (sender, kp, gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET + amount)
        .expect("Failed to fund account");

    let tx = TestTransactionBuilder::new(sender, gas, cluster.reference_gas_price())
        .with_gas_budget(DEFAULT_GAS_BUDGET)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(amount, recipient)])
        .build();

    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(tx, vec![&kp]))
        .expect("Failed to execute send_to_address_balance transaction");

    assert!(
        fx.status().is_ok(),
        "send_to_address_balance transaction failed: {:?}",
        fx.status()
    );
}

/// Transfer SUI from sender's address balance to recipient's address balance.
/// Sender must already have an address balance. Uses FundSource::AddressFund.
fn transfer_address_balance(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    sender_kp: &AccountKeyPair,
    recipient: SuiAddress,
    amount: u64,
    gas: sui_types::base_types::ObjectRef,
) {
    let tx = TestTransactionBuilder::new(sender, gas, cluster.reference_gas_price())
        .with_gas_budget(DEFAULT_GAS_BUDGET)
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(amount),
            vec![(amount, recipient)],
        )
        .build();

    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(tx, vec![sender_kp]))
        .expect("Failed to execute transfer_address_balance transaction");

    assert!(
        fx.status().is_ok(),
        "transfer_address_balance transaction failed: {:?}",
        fx.status()
    );
}

/// Basic address balance creation via balance::send_funds
#[tokio::test]
async fn test_send_funds_creates_balance() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (recipient, _) = get_account_key_pair();

    // Send 100 to recipient's address balance
    send_to_address_balance(&mut cluster, recipient, 100);
    cluster.create_checkpoint().await;

    let gas_type = GAS::type_().to_canonical_string(true);

    // Verify via get_balance
    assert_eq!(
        get_balance(&cluster, recipient, &gas_type, None)
            .await
            .unwrap(),
        (gas_type.clone(), 100)
    );

    // Verify via list_balances
    assert_eq!(
        list_balances(&cluster, recipient, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 100)], None)
    );
}

/// Multiple send_funds to same address accumulate
#[tokio::test]
async fn test_send_funds_accumulates() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (a, _) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    // Send multiple amounts to A's address balance
    send_to_address_balance(&mut cluster, a, 100);
    send_to_address_balance(&mut cluster, a, 200);
    send_to_address_balance(&mut cluster, a, 300);

    // Send to B's address balance
    send_to_address_balance(&mut cluster, b, 400);
    send_to_address_balance(&mut cluster, b, 500);

    cluster.create_checkpoint().await;

    let gas_type = GAS::type_().to_canonical_string(true);

    // A has accumulated balance of (100 + 200 + 300) = 600
    assert_eq!(
        get_balance(&cluster, a, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 600)
    );

    assert_eq!(
        list_balances(&cluster, a, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 600)], None)
    );

    // B has accumulated balance of (400 + 500) = 900
    assert_eq!(
        get_balance(&cluster, b, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 900)
    );

    assert_eq!(
        list_balances(&cluster, b, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 900)], None)
    );

    // Verify batch_get works for multiple addresses
    assert_eq!(
        batch_get_balances(
            &cluster,
            vec![(a, gas_type.clone()), (b, gas_type.clone())],
            None
        )
        .await
        .unwrap(),
        vec![
            (a.to_string(), gas_type.clone(), 600),
            (b.to_string(), gas_type.clone(), 900)
        ]
    );
}

/// Multiple coin types tracked per address
#[tokio::test]
async fn test_multiple_coin_types() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (recipient, _) = get_account_key_pair();

    // First, send some SUI to recipient's address balance
    send_to_address_balance(&mut cluster, recipient, 500);

    // Publish the custom coin package
    let (publisher, pkp, gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET * 2)
        .expect("Failed to fund publisher account");

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["packages", "coin"]);

    let (publish_fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(
            TestTransactionBuilder::new(publisher, gas, cluster.reference_gas_price())
                .with_gas_budget(DEFAULT_GAS_BUDGET)
                .publish(path)
                .build(),
            vec![&pkp],
        ))
        .expect("Failed to execute publish transaction");

    let pkg = publish_fx
        .created()
        .into_iter()
        .find_map(|((pkg, v, _), owner)| {
            (v.value() == 1 && matches!(owner, Owner::Immutable)).then_some(pkg)
        })
        .expect("Failed to find package ID");

    // Find the minted Coin<MY_COIN> objects (1000 + 200 + 30 = 1230 total)
    // The init function creates 3 coins for the publisher
    let my_coin_objects: Vec<_> = publish_fx
        .created()
        .into_iter()
        .filter(|((_, v, _), owner)| {
            v.value() != 1 && matches!(owner, Owner::AddressOwner(addr) if *addr == publisher)
        })
        .map(|((id, version, digest), _)| (id, version, digest))
        .collect();

    assert!(!my_coin_objects.is_empty(), "No MY_COIN objects found");

    // Build the TypeTag for MY_COIN
    let my_coin_type: TypeTag = format!("{}::my_coin::MY_COIN", pkg.to_canonical_display(true))
        .parse()
        .expect("Failed to parse MY_COIN type");

    // Send the first MY_COIN to recipient's address balance
    let my_coin = my_coin_objects[0];
    let gas_after_publish = publish_fx.gas_object().0;

    let tx =
        TestTransactionBuilder::new(publisher, gas_after_publish, cluster.reference_gas_price())
            .with_gas_budget(DEFAULT_GAS_BUDGET)
            .transfer_funds_to_address_balance(
                FundSource::coin(my_coin),
                vec![(1000, recipient)],
                my_coin_type.clone(),
            )
            .build();

    cluster
        .execute_transaction(Transaction::from_data_and_signer(tx, vec![&pkp]))
        .expect("Failed to send MY_COIN to address balance");

    cluster.create_checkpoint().await;

    let gas_type = GAS::type_().to_canonical_string(true);
    let my_coin_type_str = my_coin_type.to_canonical_string(true);

    // Recipient should have both SUI and MY_COIN address balances
    assert_eq!(
        get_balance(&cluster, recipient, &gas_type, None)
            .await
            .unwrap(),
        (gas_type.clone(), 500)
    );

    assert_eq!(
        get_balance(&cluster, recipient, &my_coin_type_str, None)
            .await
            .unwrap(),
        (my_coin_type_str.clone(), 1000)
    );

    // list_balances should return both types
    let (balances, _) = list_balances(&cluster, recipient, None, None, Some(10))
        .await
        .unwrap();
    assert_eq!(balances.len(), 2);

    // Verify batch_get works for multiple types
    assert_eq!(
        batch_get_balances(
            &cluster,
            vec![
                (recipient, gas_type.clone()),
                (recipient, my_coin_type_str.clone())
            ],
            None
        )
        .await
        .unwrap(),
        vec![
            (recipient.to_string(), gas_type.clone(), 500),
            (recipient.to_string(), my_coin_type_str.clone(), 1000)
        ]
    );
}

#[tokio::test]
async fn test_snapshot_consistency() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (a, _) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    // Checkpoint 1: Initial balances
    send_to_address_balance(&mut cluster, a, 100);
    send_to_address_balance(&mut cluster, a, 200);
    send_to_address_balance(&mut cluster, b, 300);
    cluster.create_checkpoint().await;

    let gas_type = GAS::type_().to_canonical_string(true);

    // A has 300, B has 300 at checkpoint 1
    assert_eq!(
        get_balance(&cluster, a, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 300)
    );
    assert_eq!(
        get_balance(&cluster, b, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 300)
    );

    // Checkpoint 2: Add more
    send_to_address_balance(&mut cluster, a, 400);
    send_to_address_balance(&mut cluster, b, 500);
    cluster.create_checkpoint().await;

    // Current checkpoint: A has 700, B has 800
    assert_eq!(
        get_balance(&cluster, a, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 700)
    );
    assert_eq!(
        get_balance(&cluster, b, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 800)
    );

    // Historical query at checkpoint 1: should see old values
    assert_eq!(
        get_balance(&cluster, a, &gas_type, Some(1)).await.unwrap(),
        (gas_type.clone(), 300)
    );
    assert_eq!(
        get_balance(&cluster, b, &gas_type, Some(1)).await.unwrap(),
        (gas_type.clone(), 300)
    );

    assert_eq!(
        list_balances(&cluster, a, Some(1), None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 300)], None)
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
            (a.to_string(), gas_type.clone(), 300),
            (b.to_string(), gas_type.clone(), 300)
        ]
    );
}

/// Test: Address-to-address transfer (A withdraws from address balance, sends to B's address balance)
#[tokio::test]
async fn test_address_to_address_transfer() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (a, akp) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    // Fund A with address balance and a gas coin
    send_to_address_balance(&mut cluster, a, 1000);
    let gas_effects = cluster
        .request_gas(a, DEFAULT_GAS_BUDGET)
        .expect("Failed to request gas for A");
    let gas = find::address_owned(&gas_effects).expect("Failed to find gas object");
    cluster.create_checkpoint().await;

    let gas_type = GAS::type_().to_canonical_string(true);

    // A has 1000, B has 0
    assert_eq!(
        get_balance(&cluster, a, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 1000)
    );
    assert_eq!(
        get_balance(&cluster, b, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 0)
    );

    // Transfer 400 from A's address balance to B's address balance
    transfer_address_balance(&mut cluster, a, &akp, b, 400, gas);
    cluster.create_checkpoint().await;

    // A has 600, B has 400
    assert_eq!(
        get_balance(&cluster, a, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 600)
    );
    assert_eq!(
        get_balance(&cluster, b, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 400)
    );

    assert_eq!(
        list_balances(&cluster, a, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 600)], None)
    );
    assert_eq!(
        list_balances(&cluster, b, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 400)], None)
    );
}

/// Test: Transfer ALL from A to B - A's balance becomes 0 and is removed from list
#[tokio::test]
async fn test_transfer_all_deletes_source() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (a, akp) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    // Fund A with address balance and a gas coin
    send_to_address_balance(&mut cluster, a, 1000);
    let gas_effects = cluster
        .request_gas(a, DEFAULT_GAS_BUDGET)
        .expect("Failed to request gas for A");
    let gas = find::address_owned(&gas_effects).expect("Failed to find gas object");
    cluster.create_checkpoint().await;

    let gas_type = GAS::type_().to_canonical_string(true);

    // Verify A has 1000
    assert_eq!(
        get_balance(&cluster, a, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 1000)
    );

    // Transfer ALL 1000 from A to B
    transfer_address_balance(&mut cluster, a, &akp, b, 1000, gas);
    cluster.create_checkpoint().await;

    // A's balance is now 0 and should not appear in list
    assert_eq!(
        get_balance(&cluster, a, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 0)
    );
    assert_eq!(
        list_balances(&cluster, a, None, None, Some(10))
            .await
            .unwrap(),
        (vec![], None) // Empty - no balance entries
    );
    // Historical query at checkpoint 1 should still show balance of 1000 for A.
    assert_eq!(
        list_balances(&cluster, a, Some(1), None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 1000)], None)
    );

    // B has all 1000
    assert_eq!(
        get_balance(&cluster, b, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 1000)
    );
    assert_eq!(
        list_balances(&cluster, b, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 1000)], None)
    );
    // Historical query at checkpoint 1 should show balance of 0 for B.
    assert_eq!(
        list_balances(&cluster, b, Some(1), None, Some(10))
            .await
            .unwrap(),
        (vec![], None)
    );
}

#[tokio::test]
async fn test_edge_cases() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (a, _) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    // Create an address balance for A
    send_to_address_balance(&mut cluster, a, 100);
    cluster.create_checkpoint().await;

    // Querying for an address with no balance should return empty list and 0 balance
    assert_eq!(
        list_balances(&cluster, b, None, None, Some(10))
            .await
            .unwrap(),
        (vec![], None)
    );
    assert_eq!(
        get_balance(&cluster, b, &GAS::type_().to_canonical_string(true), None)
            .await
            .unwrap(),
        (GAS::type_().to_canonical_string(true), 0)
    );

    // Missing owner parameter
    let mut client = ConsistentServiceClient::connect(cluster.consistent_store_url().to_string())
        .await
        .expect("Failed to connect to Consistent Store");

    let request = tonic::Request::new(ListAddressBalancesRequest {
        owner: None,
        page_size: Some(10),
        ..Default::default()
    });

    let err = client.list_address_balances(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(err.message(), "Missing 'owner'");

    let request = tonic::Request::new(GetAddressBalanceRequest {
        owner: None,
        coin_type: Some(GAS::type_().to_string()),
    });

    let err = client.get_address_balance(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(err.message(), "Missing 'owner'");

    let request = tonic::Request::new(BatchGetAddressBalancesRequest {
        requests: vec![
            GetAddressBalanceRequest {
                owner: Some(a.to_string()),
                coin_type: Some(GAS::type_().to_string()),
            },
            GetAddressBalanceRequest {
                owner: None,
                coin_type: Some(GAS::type_().to_string()),
            },
        ],
    });

    let err = client
        .batch_get_address_balances(request)
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(err.message(), "Missing 'owner'");

    // Invalid owner address
    let request = tonic::Request::new(ListAddressBalancesRequest {
        owner: Some("invalid_address".to_string()),
        page_size: Some(10),
        ..Default::default()
    });

    let err = client.list_address_balances(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(err.message(), r#"Invalid 'owner': "invalid_address""#);

    let request = tonic::Request::new(GetAddressBalanceRequest {
        owner: Some("invalid_address".to_string()),
        coin_type: Some(GAS::type_().to_string()),
    });

    let err = client.get_address_balance(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(err.message(), r#"Invalid 'owner': "invalid_address""#);

    let request = tonic::Request::new(BatchGetAddressBalancesRequest {
        requests: vec![
            GetAddressBalanceRequest {
                owner: Some("invalid_address".to_string()),
                coin_type: Some(GAS::type_().to_string()),
            },
            GetAddressBalanceRequest {
                owner: Some(b.to_string()),
                coin_type: Some(GAS::type_().to_string()),
            },
        ],
    });

    let err = client
        .batch_get_address_balances(request)
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(err.message(), r#"Invalid 'owner': "invalid_address""#);

    // Missing coin type
    let request = tonic::Request::new(GetAddressBalanceRequest {
        owner: Some(a.to_string()),
        coin_type: None,
    });

    let err = client.get_address_balance(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(err.message(), "Missing 'coin_type'");

    let request = tonic::Request::new(BatchGetAddressBalancesRequest {
        requests: vec![
            GetAddressBalanceRequest {
                owner: Some(a.to_string()),
                coin_type: None,
            },
            GetAddressBalanceRequest {
                owner: Some(b.to_string()),
                coin_type: Some(GAS::type_().to_string()),
            },
        ],
    });

    let err = client
        .batch_get_address_balances(request)
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(err.message(), "Missing 'coin_type'");

    // Invalid coin type
    let request = tonic::Request::new(GetAddressBalanceRequest {
        owner: Some(a.to_string()),
        coin_type: Some("invalid_coin_type".to_string()),
    });

    let err = client.get_address_balance(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(err.message(), r#"Invalid 'coin_type': "invalid_coin_type""#);

    let request = tonic::Request::new(BatchGetAddressBalancesRequest {
        requests: vec![
            GetAddressBalanceRequest {
                owner: Some(a.to_string()),
                coin_type: Some(GAS::type_().to_string()),
            },
            GetAddressBalanceRequest {
                owner: Some(b.to_string()),
                coin_type: Some("invalid_coin_type".to_string()),
            },
        ],
    });

    let err = client
        .batch_get_address_balances(request)
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert_eq!(err.message(), r#"Invalid 'coin_type': "invalid_coin_type""#);

    // Not in range
    let mut request = tonic::Request::new(ListAddressBalancesRequest {
        owner: Some(a.to_string()),
        ..Default::default()
    });

    request
        .metadata_mut()
        .insert("x-sui-checkpoint", "10".parse().unwrap());

    let err = client.list_address_balances(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::OutOfRange);
    assert_eq!(err.message(), "Checkpoint 10 not in the consistent range");

    let mut request = tonic::Request::new(GetAddressBalanceRequest {
        owner: Some(a.to_string()),
        coin_type: Some(GAS::type_().to_string()),
    });

    request
        .metadata_mut()
        .insert("x-sui-checkpoint", "10".parse().unwrap());

    let err = client.get_address_balance(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::OutOfRange);
    assert_eq!(err.message(), "Checkpoint 10 not in the consistent range");

    let mut request = tonic::Request::new(BatchGetAddressBalancesRequest {
        requests: vec![
            GetAddressBalanceRequest {
                owner: Some(a.to_string()),
                coin_type: Some(GAS::type_().to_string()),
            },
            GetAddressBalanceRequest {
                owner: Some(b.to_string()),
                coin_type: Some(GAS::type_().to_string()),
            },
        ],
    });

    request
        .metadata_mut()
        .insert("x-sui-checkpoint", "10".parse().unwrap());

    let err = client
        .batch_get_address_balances(request)
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::OutOfRange);
    assert_eq!(err.message(), "Checkpoint 10 not in the consistent range");
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
    let mut request = tonic::Request::new(ListAddressBalancesRequest {
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

    let response = client.list_address_balances(request).await?.into_inner();

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
    let mut request = tonic::Request::new(GetAddressBalanceRequest {
        owner: Some(owner.clone()),
        coin_type: Some(coin_type.to_owned()),
    });

    if let Some(checkpoint) = checkpoint {
        request
            .metadata_mut()
            .insert("x-sui-checkpoint", checkpoint.to_string().parse().unwrap());
    }

    let response = client.get_address_balance(request).await?.into_inner();

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

    let mut request = tonic::Request::new(BatchGetAddressBalancesRequest {
        requests: requests
            .into_iter()
            .map(|(owner, coin_type)| GetAddressBalanceRequest {
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
        .batch_get_address_balances(request)
        .await?
        .into_inner()
        .balances
        .into_iter()
        .map(|b| (b.owner().to_owned(), b.coin_type().to_owned(), b.balance()))
        .collect())
}
