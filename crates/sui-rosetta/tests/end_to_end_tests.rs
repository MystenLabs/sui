// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod rosetta_client;

use crate::rosetta_client::RosettaEndpoint;
use rosetta_client::{get_random_sui, start_rosetta_test_server};
use serde_json::json;
use sui_keys::keystore::AccountKeystore;
use sui_rosetta::operations::Operations;
use sui_rosetta::types::{
    AccountBalanceRequest, AccountBalanceResponse, AccountIdentifier, NetworkIdentifier,
    SubAccount, SubAccountType, SuiEnv,
};
use sui_sdk::json::SuiJsonValue;
use sui_sdk::rpc_types::{SuiExecutionStatus, SuiTransactionEffectsAPI};
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{parse_sui_type_tag, SUI_FRAMEWORK_OBJECT_ID};
use test_utils::network::TestClusterBuilder;

#[tokio::test]
async fn test_locked_sui() {
    let test_cluster = TestClusterBuilder::new().build().await.unwrap();
    let address = test_cluster.accounts[0];
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let (rosetta_client, _handle) =
        start_rosetta_test_server(client.clone(), test_cluster.swarm.dir()).await;

    let network_identifier = NetworkIdentifier {
        blockchain: "sui".to_string(),
        network: SuiEnv::LocalNet,
    };

    // verify no coins are locked
    let coins = client
        .coin_read_api()
        .get_coins(address, None, None, None)
        .await
        .unwrap();
    assert!(!coins
        .data
        .iter()
        .any(|coin| coin.locked_until_epoch.is_some()));

    let request = AccountBalanceRequest {
        network_identifier: network_identifier.clone(),
        account_identifier: AccountIdentifier {
            address,
            sub_account: Some(SubAccount {
                account_type: SubAccountType::LockedSui,
            }),
        },
        block_identifier: Default::default(),
        currencies: vec![],
    };
    let response: AccountBalanceResponse = rosetta_client
        .call(RosettaEndpoint::Balance, &request)
        .await;
    assert_eq!(response.balances[0].value, 0);

    // Lock some sui
    let call_args = vec![
        SuiJsonValue::from_object_id(coins.data[0].coin_object_id),
        SuiJsonValue::from_object_id(address.into()),
        SuiJsonValue::new(json!("100")).unwrap(),
    ];
    let tx = client
        .transaction_builder()
        .move_call(
            address,
            SUI_FRAMEWORK_OBJECT_ID,
            "locked_coin",
            "lock_coin",
            vec![parse_sui_type_tag("0x2::sui::SUI").unwrap().into()],
            call_args,
            None,
            2000,
        )
        .await
        .unwrap();

    let tx = to_sender_signed_transaction(tx, keystore.get_key(&address).unwrap());
    client
        .quorum_driver()
        .execute_transaction(tx, None)
        .await
        .unwrap();

    // Check the balance again after locking the coin
    let request = AccountBalanceRequest {
        network_identifier: network_identifier.clone(),
        account_identifier: AccountIdentifier {
            address,
            sub_account: Some(SubAccount {
                account_type: SubAccountType::LockedSui,
            }),
        },
        block_identifier: Default::default(),
        currencies: vec![],
    };
    let response: AccountBalanceResponse = rosetta_client
        .call(RosettaEndpoint::Balance, &request)
        .await;
    assert_eq!(1, response.balances.len());
    assert_eq!(
        100,
        response.balances[0]
            .metadata
            .as_ref()
            .unwrap()
            .lock_until_epoch
    );
    assert_eq!(100000000000000, response.balances[0].value);
}

#[tokio::test]
async fn test_get_delegated_sui() {
    let test_cluster = TestClusterBuilder::new().build().await.unwrap();
    let address = test_cluster.accounts[0];
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let (rosetta_client, _handle) =
        start_rosetta_test_server(client.clone(), test_cluster.swarm.dir()).await;

    let network_identifier = NetworkIdentifier {
        blockchain: "sui".to_string(),
        network: SuiEnv::LocalNet,
    };
    // Verify initial balance and delegation
    let request = AccountBalanceRequest {
        network_identifier: network_identifier.clone(),
        account_identifier: AccountIdentifier {
            address,
            sub_account: None,
        },
        block_identifier: Default::default(),
        currencies: vec![],
    };

    let response: AccountBalanceResponse = rosetta_client
        .call(RosettaEndpoint::Balance, &request)
        .await;
    assert_eq!(1, response.balances.len());
    assert_eq!(500000000000000, response.balances[0].value);

    let request = AccountBalanceRequest {
        network_identifier: network_identifier.clone(),
        account_identifier: AccountIdentifier {
            address,
            sub_account: Some(SubAccount {
                account_type: SubAccountType::PendingDelegation,
            }),
        },
        block_identifier: Default::default(),
        currencies: vec![],
    };
    let response: AccountBalanceResponse = rosetta_client
        .call(RosettaEndpoint::Balance, &request)
        .await;
    assert_eq!(response.balances[0].value, 0);

    // Delegate some sui
    let validators = client.governance_api().get_validators().await.unwrap();
    let coins = client
        .coin_read_api()
        .get_coins(address, None, None, None)
        .await
        .unwrap()
        .data;
    let delegation_tx = client
        .transaction_builder()
        .request_add_delegation(
            address,
            vec![coins[0].coin_object_id],
            Some(100000),
            validators[0].sui_address,
            None,
            10000,
        )
        .await
        .unwrap();
    let tx = to_sender_signed_transaction(delegation_tx, keystore.get_key(&address).unwrap());
    client
        .quorum_driver()
        .execute_transaction(tx, None)
        .await
        .unwrap();

    let request = AccountBalanceRequest {
        network_identifier: network_identifier.clone(),
        account_identifier: AccountIdentifier {
            address,
            sub_account: Some(SubAccount {
                account_type: SubAccountType::PendingDelegation,
            }),
        },
        block_identifier: Default::default(),
        currencies: vec![],
    };
    let response: AccountBalanceResponse = rosetta_client
        .call(RosettaEndpoint::Balance, &request)
        .await;
    assert_eq!(1, response.balances.len());
    assert_eq!(100000, response.balances[0].value);

    // TODO: add DelegatedSui test when we can advance epoch.
}

#[tokio::test]
async fn test_delegation() {
    let test_cluster = TestClusterBuilder::new().build().await.unwrap();
    let sender = test_cluster.accounts[0];
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;
    let coin1 = get_random_sui(&client, sender, vec![]).await;

    let (rosetta_client, _handle) =
        start_rosetta_test_server(client.clone(), test_cluster.swarm.dir()).await;

    let validator = client
        .governance_api()
        .get_validators()
        .await
        .unwrap()
        .first()
        .unwrap()
        .sui_address;
    let ops = client
        .transaction_builder()
        .request_add_delegation(sender, vec![coin1.0], Some(100000), validator, None, 10000)
        .await
        .unwrap();

    let ops = Operations::try_from(ops).unwrap();

    let response = rosetta_client.rosetta_flow(ops, keystore).await;

    let tx = client
        .read_api()
        .get_transaction(response.transaction_identifier.hash)
        .await
        .unwrap();

    println!("Sui TX: {tx:?}");

    assert_eq!(SuiExecutionStatus::Success, *tx.effects.status())
}
