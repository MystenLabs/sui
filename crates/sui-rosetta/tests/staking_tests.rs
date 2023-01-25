// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;
use sui_config::utils;
use sui_keys::keystore::AccountKeystore;
use sui_rosetta::types::{
    AccountBalanceRequest, AccountBalanceResponse, AccountIdentifier, NetworkIdentifier,
    SubAccount, SubAccountType, SuiEnv,
};
use sui_rosetta::RosettaOnlineServer;
use sui_sdk::json::SuiJsonValue;
use sui_sdk::SuiClient;
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{parse_sui_type_tag, SUI_FRAMEWORK_OBJECT_ID};
use test_utils::network::TestClusterBuilder;
use tokio::task::JoinHandle;

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
    let response: AccountBalanceResponse = rosetta_client.call("account/balance", &request).await;
    assert!(response.balances.is_empty());

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
    let response: AccountBalanceResponse = rosetta_client.call("account/balance", &request).await;
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

    let response: AccountBalanceResponse = rosetta_client.call("account/balance", &request).await;
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
    let response: AccountBalanceResponse = rosetta_client.call("account/balance", &request).await;
    assert_eq!(0, response.balances.len());

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
    let response: AccountBalanceResponse = rosetta_client.call("account/balance", &request).await;
    assert_eq!(1, response.balances.len());
    assert_eq!(100000, response.balances[0].value);

    // TODO: add DelegatedSui test when we can advance epoch.
}

async fn start_rosetta_test_server(
    client: SuiClient,
    dir: &Path,
) -> (RosettaClient, JoinHandle<hyper::Result<()>>) {
    let rosetta_server =
        RosettaOnlineServer::new(SuiEnv::LocalNet, client, &dir.join("rosetta_data"));
    let local_ip = utils::get_local_ip_for_tests().to_string();
    let port = utils::get_available_port(&local_ip);
    let rosetta_address = format!("{}:{}", local_ip, port);
    let _handle = rosetta_server.serve(SocketAddr::from_str(&rosetta_address).unwrap());

    // wait for rosetta to process the genesis block.
    tokio::time::sleep(Duration::from_millis(100)).await;
    (RosettaClient::new(port), _handle)
}

struct RosettaClient {
    client: Client,
    port: u16,
}

impl RosettaClient {
    fn new(port: u16) -> Self {
        let client = Client::new();
        Self { client, port }
    }
    async fn call<R: Serialize, T: DeserializeOwned>(&self, endpoint: &str, request: &R) -> T {
        let response = self
            .client
            .post(format!("http://127.0.0.1:{}/{endpoint}", self.port))
            .json(&json!(request))
            .send()
            .await
            .unwrap();

        let json: Value = response.json().await.unwrap();
        if let Ok(v) = serde_json::from_value(json.clone()) {
            v
        } else {
            panic!("Failed to deserialize json value: {json:#?}")
        }
    }
}
