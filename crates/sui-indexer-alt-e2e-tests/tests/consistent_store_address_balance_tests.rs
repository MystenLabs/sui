// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::str::FromStr;

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::BatchGetBalancesRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::GetBalanceRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::ListBalancesRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::ListObjectsByTypeRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_client::ConsistentServiceClient;
use sui_test_transaction_builder::FundSource;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::Identifier;
use sui_types::TypeTag;
use sui_types::base_types::ObjectDigest;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::AccountKeyPair;
use sui_types::crypto::get_account_key_pair;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::gas_coin::GAS;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::CallArg;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionData;

use tonic::transport::Channel;

use sui_indexer_alt_e2e_tests::FullCluster;
use sui_indexer_alt_e2e_tests::find;

/// 5 SUI gas budget
const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

/// Correctly index as address balance accumulates over multiple sends
#[tokio::test]
async fn test_index_address_balance_accumulates() {
    let mut cluster = BalanceCluster::new().await;
    let (a, _) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    // Send multiple amounts to A's address balance
    cluster.send_sui_to_address_balance(a, 100);
    cluster.send_sui_to_address_balance(a, 200);
    cluster.send_sui_to_address_balance(a, 300);

    // Send to B's address balance
    cluster.send_sui_to_address_balance(b, 400);
    cluster.send_sui_to_address_balance(b, 500);

    cluster.cluster.create_checkpoint().await;

    let gas_type = GAS::type_().to_canonical_string(true);

    // A has accumulated balance of (100 + 200 + 300) = 600
    assert_eq!(
        cluster.get_balance(a, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 600)
    );

    assert_eq!(
        cluster
            .list_balances(a, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 600)], None)
    );

    // B has accumulated balance of (400 + 500) = 900
    assert_eq!(
        cluster.get_balance(b, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 900)
    );

    assert_eq!(
        cluster
            .list_balances(b, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 900)], None)
    );

    // Verify batch_get works for multiple addresses
    assert_eq!(
        cluster
            .batch_get_balances(vec![(a, gas_type.clone()), (b, gas_type.clone())], None)
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
    // Use the BalanceCluster, which publishes the coin package for us
    let mut cluster = BalanceCluster::new().await;
    let (recipient, _) = get_account_key_pair();

    cluster.send_sui_to_address_balance(recipient, 500);
    cluster.cluster.create_checkpoint().await;

    // Have the publisher mint and send MY_COIN to test address balance
    let my_coin_type = cluster.my_coin_type();
    cluster
        .send_balance_to_address_balance(recipient, 1000, &my_coin_type)
        .await;
    cluster.cluster.create_checkpoint().await;

    let gas_type = GAS::type_().to_canonical_string(true);
    let my_coin_type_str = my_coin_type.to_canonical_string(true);

    // Recipient should have both SUI and MY_COIN address balances
    assert_eq!(
        cluster
            .get_balance(recipient, &gas_type, None)
            .await
            .unwrap(),
        (gas_type.clone(), 500)
    );

    assert_eq!(
        cluster
            .get_balance(recipient, &my_coin_type_str, None)
            .await
            .unwrap(),
        (my_coin_type_str.clone(), 1000)
    );

    // list_balances should return both types
    let (balances, _) = cluster
        .list_balances(recipient, None, None, Some(10))
        .await
        .unwrap();
    assert_eq!(balances.len(), 2);

    // Verify batch_get works for multiple types
    assert_eq!(
        cluster
            .batch_get_balances(
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
    let mut cluster = BalanceCluster::new().await;
    let (a, _) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    // Checkpoint 1: Initial balances
    cluster.send_sui_to_address_balance(a, 100);
    cluster.send_sui_to_address_balance(a, 200);
    cluster.send_sui_to_address_balance(b, 300);
    cluster.cluster.create_checkpoint().await;

    let gas_type = GAS::type_().to_canonical_string(true);

    // A has 300, B has 300 at checkpoint 1
    assert_eq!(
        cluster.get_balance(a, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 300)
    );
    assert_eq!(
        cluster.get_balance(b, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 300)
    );

    // Checkpoint 2: Add more
    cluster.send_sui_to_address_balance(a, 400);
    cluster.send_sui_to_address_balance(b, 500);
    cluster.cluster.create_checkpoint().await;

    // Current checkpoint: A has 700, B has 800
    assert_eq!(
        cluster.get_balance(a, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 700)
    );
    assert_eq!(
        cluster.get_balance(b, &gas_type, None).await.unwrap(),
        (gas_type.clone(), 800)
    );

    // Historical query at checkpoint 1: should see old values
    assert_eq!(
        cluster.get_balance(a, &gas_type, Some(1)).await.unwrap(),
        (gas_type.clone(), 300)
    );
    assert_eq!(
        cluster.get_balance(b, &gas_type, Some(1)).await.unwrap(),
        (gas_type.clone(), 300)
    );
    assert_eq!(
        cluster
            .list_balances(a, Some(1), None, Some(10))
            .await
            .unwrap(),
        (vec![(gas_type.clone(), 300)], None)
    );
    assert_eq!(
        cluster
            .batch_get_balances(vec![(a, gas_type.clone()), (b, gas_type.clone())], Some(1))
            .await
            .unwrap(),
        vec![
            (a.to_string(), gas_type.clone(), 300),
            (b.to_string(), gas_type.clone(), 300)
        ]
    );
}

/// Test: Address-to-address transfer (A withdraws from address balance, sends to B's address balance).
/// Additionally test that transferring ALL from A to B yields empty balance for A.
#[tokio::test]
async fn test_address_to_address_transfer() {
    let mut cluster = BalanceCluster::new().await;
    let (a, akp) = get_account_key_pair();
    let (b, _) = get_account_key_pair();
    let my_coin_type = cluster.my_coin_type();
    let my_coin_type_str = my_coin_type.to_canonical_string(true);
    cluster.cluster.create_checkpoint().await;

    // Fund A with address balance of MY_COIN
    cluster
        .send_balance_to_address_balance(a, 1000, &my_coin_type)
        .await;
    cluster.cluster.create_checkpoint().await;

    // Verify A has 1000
    assert_eq!(
        cluster
            .get_balance(a, &my_coin_type_str, None)
            .await
            .unwrap(),
        (my_coin_type_str.clone(), 1000)
    );
    // B has 0
    assert_eq!(
        cluster
            .get_balance(b, &my_coin_type_str, None)
            .await
            .unwrap(),
        (my_coin_type_str.clone(), 0)
    );

    cluster.transfer_address_balance(a, &akp, None, b, 500, my_coin_type.clone());
    cluster.cluster.create_checkpoint().await;

    // A has 500
    assert_eq!(
        cluster
            .get_balance(a, &my_coin_type_str, None)
            .await
            .unwrap(),
        (my_coin_type_str.clone(), 500)
    );
    // B has 500
    assert_eq!(
        cluster
            .get_balance(b, &my_coin_type_str, None)
            .await
            .unwrap(),
        (my_coin_type_str.clone(), 500)
    );

    // Transfer remaining address balance to B.
    cluster.transfer_address_balance(a, &akp, None, b, 500, my_coin_type.clone());
    cluster.cluster.create_checkpoint().await;

    // A's balance is now 0.
    assert_eq!(
        cluster
            .get_balance(a, &my_coin_type_str, None)
            .await
            .unwrap(),
        (my_coin_type_str.clone(), 0)
    );

    let (balances, _) = cluster
        .list_balances(a, None, None, Some(10))
        .await
        .unwrap();
    // Single balance entry for SUI/ gas
    assert_eq!(balances.len(), 1);
    assert_eq!(balances[0].0, GAS::type_().to_canonical_string(true));

    // Historical query at checkpoint 3 should show two balances for A, with one of them being a
    // balance of 500 for MY_COIN
    let (balances, _) = cluster
        .list_balances(a, Some(3), None, Some(10))
        .await
        .unwrap();
    assert_eq!(balances.len(), 2);
    assert!(
        balances
            .iter()
            .any(|(ty, amt)| ty == &my_coin_type_str && *amt == 500)
    );

    // Historical query at checkpoint 2 should show 1 balance for A of 1000 for MY_COIN
    let (balances, _) = cluster
        .list_balances(a, Some(2), None, Some(10))
        .await
        .unwrap();
    assert_eq!(balances.len(), 1);
    assert!(balances[0].0 == my_coin_type_str && balances[0].1 == 1000);

    // B has all 1000
    assert_eq!(
        cluster
            .get_balance(b, &my_coin_type_str, None)
            .await
            .unwrap(),
        (my_coin_type_str.clone(), 1000)
    );
    assert_eq!(
        cluster
            .list_balances(b, None, None, Some(10))
            .await
            .unwrap(),
        (vec![(my_coin_type_str.clone(), 1000)], None)
    );
    // Historical query at checkpoint 3 should show 1 balance for B of 500 for MY_COIN
    assert_eq!(
        cluster
            .list_balances(b, Some(3), None, Some(10))
            .await
            .unwrap(),
        (vec![(my_coin_type_str.clone(), 500)], None)
    );
    // Historical query at checkpoint 2 should show no balance for B
    assert_eq!(
        cluster
            .list_balances(b, Some(2), None, Some(10))
            .await
            .unwrap(),
        (vec![], None)
    );
}

/// Test list_balances pagination from end and front.
///
/// The test is setup with A_COIN (coin balance), B_COIN (coin + address), C_COIN (address), MY_COIN
/// (coin)
///
/// Paginate "forwards" from the end, and then forwards from the front.
///
/// Additionally test that adding balances to merge do not perturb pagination logic.
#[tokio::test]
async fn test_list_balances_pagination() {
    let mut cluster = BalanceCluster::new().await;
    let (recipient, _) = get_account_key_pair();
    cluster.cluster.create_checkpoint().await;

    // Build TypeTags (alphabetical order: A_COIN, B_COIN, C_COIN, MY_COIN)
    let a_coin_type = cluster.coin_type("a");
    let b_coin_type = cluster.coin_type("b");
    let c_coin_type = cluster.coin_type("c");
    let my_coin_type = cluster.my_coin_type();

    // Setup balances:
    // A_COIN coin balance of 1000
    // B_COIN coin balance of 500 + address balance of 300 = 800 total
    // C_COIN address balance of 200
    // MY_COIN coin balance of 100

    // A_COIN coin balance of 1000
    cluster
        .send_coin_to_address(recipient, 1000, &a_coin_type)
        .await;
    cluster
        .send_coin_to_address(recipient, 500, &b_coin_type)
        .await;
    // Advance checkpoint to ensure consistent store picks up latest treasury cap
    cluster.cluster.create_checkpoint().await;
    cluster
        .send_balance_to_address_balance(recipient, 300, &b_coin_type)
        .await;
    cluster
        .send_balance_to_address_balance(recipient, 200, &c_coin_type)
        .await;
    cluster
        .send_coin_to_address(recipient, 100, &my_coin_type)
        .await;
    cluster.cluster.create_checkpoint().await;

    let a_type_str = a_coin_type.to_canonical_string(true);
    let b_type_str = b_coin_type.to_canonical_string(true);
    let c_type_str = c_coin_type.to_canonical_string(true);
    let my_type_str = my_coin_type.to_canonical_string(true);

    // State: A(coin=1000), B(coin=500,addr=300,total=800), C(addr=200), MY(coin=100)
    // page_size=2 from back should get [C_COIN, MY_COIN]
    let resp = paginate_list_balances(&cluster.cluster, recipient, None, None, Some(2), Some(2))
        .await
        .unwrap();
    assert_eq!(resp.balances.len(), 2);
    assert_eq!(resp.balances[0].coin_type(), c_type_str);
    assert_eq!(resp.balances[0].total_balance(), 200);
    assert_eq!(resp.balances[1].coin_type(), my_type_str);
    assert_eq!(resp.balances[1].total_balance(), 100);
    assert!(resp.has_previous_page());
    assert!(!resp.has_next_page());

    // Continue backward: get [A_COIN, B_COIN]
    let cursor = resp.balances[0].page_token().to_owned();
    let resp = paginate_list_balances(
        &cluster.cluster,
        recipient,
        None,
        Some(cursor),
        Some(2),
        Some(2),
    )
    .await
    .unwrap();
    assert_eq!(resp.balances.len(), 2);
    assert_eq!(resp.balances[0].coin_type(), a_type_str);
    assert_eq!(resp.balances[0].total_balance(), 1000);
    assert_eq!(resp.balances[1].coin_type(), b_type_str);
    assert_eq!(resp.balances[1].total_balance(), 800); // merged: 500 + 300
    assert!(!resp.has_previous_page());
    assert!(resp.has_next_page());

    // page_size=2 from front should get [A_COIN, B_COIN]
    let resp = paginate_list_balances(&cluster.cluster, recipient, None, None, Some(2), Some(1))
        .await
        .unwrap();
    assert_eq!(resp.balances.len(), 2);
    assert_eq!(resp.balances[0].coin_type(), a_type_str);
    assert_eq!(resp.balances[0].total_balance(), 1000);
    assert_eq!(resp.balances[1].coin_type(), b_type_str);
    assert_eq!(resp.balances[1].total_balance(), 800); // merged: 500 + 300
    assert!(!resp.has_previous_page());
    assert!(resp.has_next_page());

    // Continue forward: get [C_COIN, MY_COIN]
    let cursor = resp.balances[1].page_token().to_owned();
    let resp = paginate_list_balances(
        &cluster.cluster,
        recipient,
        Some(cursor),
        None,
        Some(2),
        Some(1),
    )
    .await
    .unwrap();
    assert_eq!(resp.balances.len(), 2);
    assert_eq!(resp.balances[0].coin_type(), c_type_str);
    assert_eq!(resp.balances[0].total_balance(), 200);
    assert_eq!(resp.balances[1].coin_type(), my_type_str);
    assert_eq!(resp.balances[1].total_balance(), 100);
    assert!(resp.has_previous_page());
    assert!(!resp.has_next_page());

    // Test that coin_balance only correctly merges with address balance
    cluster
        .send_balance_to_address_balance(recipient, 50, &my_coin_type)
        .await;
    cluster.cluster.create_checkpoint().await;

    // page_size=2 from back should get [C_COIN, MY_COIN (now merged: 100+50=150)]
    let resp = paginate_list_balances(&cluster.cluster, recipient, None, None, Some(2), Some(2))
        .await
        .unwrap();
    assert_eq!(resp.balances.len(), 2);
    assert_eq!(resp.balances[0].coin_type(), c_type_str);
    assert_eq!(resp.balances[1].coin_type(), my_type_str);
    assert_eq!(resp.balances[1].total_balance(), 150);
    assert!(resp.has_previous_page());
    assert!(!resp.has_next_page()); // MY_COIN is still last

    // Add address balance A_COIN to be merged
    cluster
        .send_balance_to_address_balance(recipient, 25, &a_coin_type)
        .await;
    cluster.cluster.create_checkpoint().await;

    // Paginate all the way back to A_COIN, verify no prev page
    // page_size=1 from back: MY_COIN
    let resp = paginate_list_balances(&cluster.cluster, recipient, None, None, Some(1), Some(2))
        .await
        .unwrap();
    assert_eq!(resp.balances[0].coin_type(), my_type_str);
    let cursor = resp.balances[0].page_token().to_owned();

    // C_COIN
    let resp = paginate_list_balances(
        &cluster.cluster,
        recipient,
        None,
        Some(cursor),
        Some(1),
        Some(2),
    )
    .await
    .unwrap();
    assert_eq!(resp.balances[0].coin_type(), c_type_str);
    let cursor = resp.balances[0].page_token().to_owned();

    // B_COIN
    let resp = paginate_list_balances(
        &cluster.cluster,
        recipient,
        None,
        Some(cursor),
        Some(1),
        Some(2),
    )
    .await
    .unwrap();
    assert_eq!(resp.balances[0].coin_type(), b_type_str);
    let cursor = resp.balances[0].page_token().to_owned();

    // A_COIN - should have no prev page
    let resp = paginate_list_balances(
        &cluster.cluster,
        recipient,
        None,
        Some(cursor),
        Some(1),
        Some(2),
    )
    .await
    .unwrap();
    assert_eq!(resp.balances[0].coin_type(), a_type_str);
    assert_eq!(resp.balances[0].total_balance(), 1025); // 1000 + 25
    assert!(
        !resp.has_previous_page(),
        "A_COIN is first, should not have previous page"
    );
    assert!(resp.has_next_page(), "Should have next page after A_COIN");
}

#[tokio::test]
async fn test_edge_cases() {
    let mut cluster = BalanceCluster::new().await;
    let (a, _) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    cluster.cluster.create_checkpoint().await;

    // Querying for an address with no balance should return empty list and 0 balance
    assert_eq!(
        cluster
            .list_balances(a, None, None, Some(10))
            .await
            .unwrap(),
        (vec![], None)
    );
    assert_eq!(
        cluster
            .get_balance(b, &GAS::type_().to_canonical_string(true), None)
            .await
            .unwrap(),
        (GAS::type_().to_canonical_string(true), 0)
    );

    // Missing owner parameter
    let mut client = cluster.client;

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

struct BalanceCluster {
    cluster: FullCluster,
    /// Client to the consistent service
    client: ConsistentServiceClient<Channel>,
    publisher: SuiAddress,
    pkp: AccountKeyPair,
    pkg: ObjectID,
}

impl BalanceCluster {
    /// Initialize a BalanceCluster which publishes the coin package.
    async fn new() -> Self {
        let mut cluster = FullCluster::new().await.unwrap();
        let client = ConsistentServiceClient::connect(cluster.consistent_store_url().to_string())
            .await
            .expect("Failed to connect to Consistent Store");

        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.extend(["packages", "coin"]);
        let (publisher, pkp, gas) = cluster
            .funded_account(DEFAULT_GAS_BUDGET * 2)
            .expect("Failed to fund account");

        let (publish_fx, _) = cluster
            .execute_transaction(Transaction::from_data_and_signer(
                TestTransactionBuilder::new(publisher, gas, cluster.reference_gas_price())
                    .with_gas_budget(DEFAULT_GAS_BUDGET)
                    .publish(path)
                    .build(),
                vec![&pkp],
            ))
            .expect("Failed to publish coin package");

        let pkg = publish_fx
            .created()
            .into_iter()
            .find_map(|((pkg, v, _), owner)| {
                (v.value() == 1 && matches!(owner, Owner::Immutable)).then_some(pkg)
            })
            .expect("Failed to find package ID");

        Self {
            cluster,
            client,
            publisher,
            pkp,
            pkg,
        }
    }

    fn request_gas(&mut self, requester: SuiAddress, amount: u64) -> ObjectRef {
        let gas_effects = self
            .cluster
            .request_gas(requester, DEFAULT_GAS_BUDGET + amount)
            .expect("Failed to request gas");
        find::address_owned(&gas_effects).expect("Failed to find gas object")
    }

    /// Publisher mints SUI to send to a recipient's address balance.
    fn send_sui_to_address_balance(&mut self, recipient: SuiAddress, amount: u64) {
        let gas = self.request_gas(self.publisher, amount);
        let tx =
            TestTransactionBuilder::new(self.publisher, gas, self.cluster.reference_gas_price())
                .with_gas_budget(DEFAULT_GAS_BUDGET)
                .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(amount, recipient)])
                .build();

        run_tx_and_return_gas(&mut self.cluster, tx, &self.pkp);
    }

    /// Publisher mints a coin and transfers the coin object to the recipient. The treasury cap is
    /// expected to be owned by the BalanceCluster's publisher.
    async fn send_coin_to_address(
        &mut self,
        recipient: SuiAddress,
        amount: u64,
        coin_type: &TypeTag,
    ) {
        let (module, func) = self.mint_coin_func(coin_type);
        let gas = self.request_gas(self.publisher, 0);

        let tx = mint_tx(
            self.publisher,
            gas,
            self.pkg,
            module,
            func,
            self.treasury_cap(coin_type).await,
            amount,
            recipient,
            self.cluster.reference_gas_price(),
        );

        run_tx_and_return_gas(&mut self.cluster, tx, &self.pkp);
    }

    /// Publisher mints a balance to send to a recipient's address balance. The treasury cap is
    /// expected to be owned by the BalanceCluster's publisher.
    async fn send_balance_to_address_balance(
        &mut self,
        recipient: SuiAddress,
        amount: u64,
        coin_type: &TypeTag,
    ) {
        let (module, func) = self.mint_balance_func(coin_type);
        let gas = self.request_gas(self.publisher, 0);

        let tx = mint_tx(
            self.publisher,
            gas,
            self.pkg,
            module,
            func,
            self.treasury_cap(coin_type).await,
            amount,
            recipient,
            self.cluster.reference_gas_price(),
        );
        run_tx_and_return_gas(&mut self.cluster, tx, &self.pkp);
    }

    /// Transfer a balance from sender's address balance to recipient's address balance. Sender must
    /// already have an address balance. Uses FundSource::AddressFund.
    fn transfer_address_balance(
        &mut self,
        sender: SuiAddress,
        signer: &AccountKeyPair,
        gas: Option<ObjectRef>,
        recipient: SuiAddress,
        amount: u64,
        coin_type: TypeTag,
    ) {
        let gas = gas.unwrap_or_else(|| self.request_gas(sender, amount));
        let tx = TestTransactionBuilder::new(sender, gas, self.cluster.reference_gas_price())
            .with_gas_budget(DEFAULT_GAS_BUDGET)
            .transfer_funds_to_address_balance(
                FundSource::address_fund_with_reservation(amount),
                vec![(amount, recipient)],
                coin_type,
            )
            .build();

        let (fx, _) = self
            .cluster
            .execute_transaction(Transaction::from_data_and_signer(tx, vec![signer]))
            .expect("Failed to execute transfer_address_balance transaction");

        assert!(
            fx.status().is_ok(),
            "transfer_address_balance transaction failed: {:?}",
            fx.status()
        );
    }

    /// Load the treasury cap object reference from consistent store.
    async fn treasury_cap(&mut self, coin_type: &TypeTag) -> ObjectRef {
        let request = tonic::Request::new(ListObjectsByTypeRequest {
            object_type: Some(format!(
                "0x2::coin::TreasuryCap<{}>",
                coin_type.to_canonical_string(true)
            )),
            page_size: Some(100),
            ..Default::default()
        });

        let response = self
            .client
            .list_objects_by_type(request)
            .await
            .expect("Failed to list treasury cap from consistent store")
            .into_inner();

        let caps: Vec<ObjectRef> = response
            .objects
            .into_iter()
            .map(|o| {
                let id = ObjectID::from_str(o.object_id()).expect("Invalid object ID");
                let version = SequenceNumber::from_u64(o.version());
                let digest = ObjectDigest::from_str(o.digest()).expect("Invalid digest");
                (id, version, digest)
            })
            .collect();

        assert!(
            !caps.is_empty(),
            "No treasury cap found for coin type {}",
            coin_type
        );
        caps[0]
    }

    fn my_coin_type(&self) -> TypeTag {
        self.coin_type("my")
    }

    fn coin_type(&self, module_prefix: &str) -> TypeTag {
        format!(
            "{}::{}_coin::{}_COIN",
            self.pkg.to_canonical_display(true),
            module_prefix,
            module_prefix.to_uppercase()
        )
        .parse()
        .expect("Failed to parse coin type")
    }

    fn mint_coin_func(&self, coin_type: &TypeTag) -> (&'static str, &'static str) {
        match coin_type {
            TypeTag::Struct(s) => {
                let module_name = s.module.as_str();
                match module_name {
                    "a_coin" => ("a_coin", "mint_coin"),
                    "b_coin" => ("b_coin", "mint_coin"),
                    "c_coin" => ("c_coin", "mint_coin"),
                    "my_coin" => ("my_coin", "mint"),
                    _ => panic!("Unsupported coin type for mint_coin_func: {}", coin_type),
                }
            }
            _ => panic!("Unsupported coin type for mint_coin_func: {}", coin_type),
        }
    }

    fn mint_balance_func(&self, coin_type: &TypeTag) -> (&'static str, &'static str) {
        match coin_type {
            TypeTag::Struct(s) => {
                let module_name = s.module.as_str();
                match module_name {
                    "a_coin" => ("a_coin", "mint_balance"),
                    "b_coin" => ("b_coin", "mint_balance"),
                    "c_coin" => ("c_coin", "mint_balance"),
                    "my_coin" => ("my_coin", "mint_balance"),
                    _ => panic!("Unsupported coin type for mint_balance_func: {}", coin_type),
                }
            }
            _ => panic!("Unsupported coin type for mint_balance_func: {}", coin_type),
        }
    }

    /// Helper to perform forward pagination over balances.
    async fn list_balances(
        &mut self,
        owner: SuiAddress,
        checkpoint: Option<u64>,
        after_token: Option<Vec<u8>>,
        page_size: Option<u32>,
    ) -> Result<(Vec<(String, u64)>, Option<Vec<u8>>), tonic::Status> {
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

        let response = self.client.list_balances(request).await?.into_inner();

        let after_token = response
            .has_next_page()
            .then(|| response.balances.last().map(|b| b.page_token().to_owned()))
            .flatten();

        let balances = response
            .balances
            .into_iter()
            .map(|b| {
                assert_eq!(b.owner(), &owner, "Owner mismatch in balance response");
                (b.coin_type().to_owned(), b.total_balance())
            })
            .collect();

        Ok((balances, after_token))
    }

    /// Helper to perform a single balance lookup
    async fn get_balance(
        &mut self,
        owner: SuiAddress,
        coin_type: &str,
        checkpoint: Option<u64>,
    ) -> Result<(String, u64), tonic::Status> {
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

        let response = self.client.get_balance(request).await?.into_inner();

        assert_eq!(
            response.owner(),
            &owner,
            "Owner mismatch in balance response"
        );

        Ok((response.coin_type().to_owned(), response.total_balance()))
    }

    async fn batch_get_balances(
        &mut self,
        requests: Vec<(SuiAddress, String)>,
        checkpoint: Option<u64>,
    ) -> Result<Vec<(String, String, u64)>, tonic::Status> {
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

        Ok(self
            .client
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
}

/// Helper that returns the raw ListBalancesResponse for flexible assertions.
async fn paginate_list_balances(
    cluster: &FullCluster,
    owner: SuiAddress,
    after_token: Option<Vec<u8>>,
    before_token: Option<Vec<u8>>,
    page_size: Option<u32>,
    end: Option<i32>,
) -> Result<
    sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::ListBalancesResponse,
    tonic::Status,
> {
    let mut client = ConsistentServiceClient::connect(cluster.consistent_store_url().to_string())
        .await
        .expect("Failed to connect to Consistent Store");

    let request = tonic::Request::new(ListBalancesRequest {
        owner: Some(owner.to_string()),
        page_size,
        after_token: after_token.map(Into::into),
        before_token: before_token.map(Into::into),
        end,
    });

    client.list_balances(request).await.map(|r| r.into_inner())
}

/// Convenience to construct a transaction to call pkg::module::mint_coin or mint_balance
fn mint_tx(
    publisher: SuiAddress,
    gas: ObjectRef,
    pkg: ObjectID,
    module: &str,
    func: &str,
    treasury_cap: ObjectRef,
    amount: u64,
    recipient: SuiAddress,
    reference_gas_price: u64,
) -> TransactionData {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            pkg,
            Identifier::new(module).unwrap(),
            Identifier::new(func).unwrap(),
            vec![],
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(treasury_cap)),
                CallArg::Pure(bcs::to_bytes(&amount).unwrap()),
                CallArg::Pure(bcs::to_bytes(&recipient).unwrap()),
            ],
        )
        .unwrap();

    TransactionData::new_programmable(
        publisher,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        reference_gas_price,
    )
}

/// Helper fn to get around mut and immutable borrow issues.
fn run_tx_and_return_gas(
    cluster: &mut FullCluster,
    data: TransactionData,
    signer: &AccountKeyPair,
) -> ObjectRef {
    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![signer]))
        .expect("Failed to execute transaction");
    assert!(fx.status().is_ok(), "Transaction failed: {:?}", fx.status());
    fx.gas_object().0
}
