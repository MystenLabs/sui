// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use fastcrypto::encoding::Base64;
use fastcrypto::encoding::Encoding;
use prometheus::Registry;
use serde::Deserialize;
use serde_json::json;
use sui_indexer_alt_consistent_store::ObjectByOwnerKey;
use sui_indexer_alt_e2e_tests::OffchainCluster;
use sui_indexer_alt_e2e_tests::OffchainClusterConfig;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientArgs;
use sui_json_rpc_types::Coin;
use sui_json_rpc_types::Page;
use sui_json_rpc_types::SuiObjectResponse;
use sui_test_transaction_builder::FundSource;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::gas_coin::GAS;
use sui_types::object::Owner;
use tempfile::TempDir;
use test_cluster::addr_balance_test_env::TestEnv;
use test_cluster::addr_balance_test_env::TestEnvBuilder;

#[derive(Deserialize)]
struct CoinsResponse {
    result: Page<Coin, String>,
}

#[derive(Deserialize)]
struct ObjectResponse {
    result: SuiObjectResponse,
}

#[derive(Deserialize)]
struct MultiObjectResponse {
    result: Vec<SuiObjectResponse>,
}

/// Wraps a `TestEnv` (real validators via `TestCluster`) with an `OffchainCluster` (indexer +
/// JSON-RPC) connected by local data ingestion.
struct FullCluster {
    test_env: TestEnv,
    offchain: OffchainCluster,
    _temp_dir: TempDir,
}

impl FullCluster {
    async fn new() -> anyhow::Result<Self> {
        let temp_dir = TempDir::new()?;
        let ingestion_dir = temp_dir.path().to_path_buf();

        let test_env = TestEnvBuilder::new()
            .with_test_cluster_builder_cb(Box::new({
                let dir = ingestion_dir.clone();
                move |builder| builder.with_data_ingestion_dir(dir.clone())
            }))
            .with_proto_override_cb(Box::new(|_, mut cfg| {
                cfg.enable_address_balance_gas_payments_for_testing();
                cfg.enable_coin_reservation_for_testing();
                cfg
            }))
            .build()
            .await;

        let offchain = OffchainCluster::new(
            ClientArgs {
                ingestion: IngestionClientArgs {
                    local_ingestion_path: Some(ingestion_dir),
                    ..Default::default()
                },
                ..Default::default()
            },
            OffchainClusterConfig::default(),
            &Registry::new(),
        )
        .await?;

        offchain
            .wait_for_indexer(0, Duration::from_secs(30))
            .await
            .expect("Timed out waiting for genesis checkpoint");
        offchain
            .wait_for_consistent_store(0, Duration::from_secs(30))
            .await
            .expect("Timed out waiting for consistent store genesis");

        Ok(Self {
            test_env,
            offchain,
            _temp_dir: temp_dir,
        })
    }

    /// Wait for the off-chain stack to catch up with the latest on-chain checkpoint.
    async fn sync(&self) {
        let cp = self
            .test_env
            .cluster
            .fullnode_handle
            .sui_node
            .state()
            .get_latest_checkpoint_sequence_number()
            .unwrap();
        tokio::try_join!(
            self.offchain.wait_for_indexer(cp, Duration::from_secs(60)),
            self.offchain
                .wait_for_consistent_store(cp, Duration::from_secs(60)),
        )
        .expect("Timed out waiting for off-chain services to sync");
    }

    fn jsonrpc_url(&self) -> url::Url {
        self.offchain.jsonrpc_url()
    }

    async fn get_coins(
        &self,
        owner: SuiAddress,
        coin_type: &str,
        cursor: Option<String>,
        limit: usize,
    ) -> CoinsResponse {
        let query = json!({
            "jsonrpc": "2.0",
            "method": "suix_getCoins",
            "params": [owner.to_string(), coin_type, cursor, limit],
            "id": 1
        });

        reqwest::Client::new()
            .post(self.jsonrpc_url().as_str())
            .json(&query)
            .send()
            .await
            .expect("Request to JSON-RPC server failed")
            .json()
            .await
            .expect("Failed to parse JSON-RPC response")
    }

    async fn get_object(&self, object_id: &str) -> ObjectResponse {
        let query = json!({
            "jsonrpc": "2.0",
            "method": "sui_getObject",
            "params": [object_id, { "showContent": true, "showOwner": true, "showType": true }],
            "id": 1
        });

        reqwest::Client::new()
            .post(self.jsonrpc_url().as_str())
            .json(&query)
            .send()
            .await
            .expect("Request to JSON-RPC server failed")
            .json()
            .await
            .expect("Failed to parse JSON-RPC response")
    }

    async fn multi_get_objects(&self, object_ids: &[&str]) -> MultiObjectResponse {
        let query = json!({
            "jsonrpc": "2.0",
            "method": "sui_multiGetObjects",
            "params": [object_ids, { "showContent": true, "showOwner": true, "showType": true }],
            "id": 1
        });

        reqwest::Client::new()
            .post(self.jsonrpc_url().as_str())
            .json(&query)
            .send()
            .await
            .expect("Request to JSON-RPC server failed")
            .json()
            .await
            .expect("Failed to parse JSON-RPC response")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Fund an address balance and verify that getCoins returns the coin and getObject
/// resolves the address balance coin's masked object ID.
#[tokio::test]
async fn test_get_object_resolves_address_balance_coin_id() {
    let mut cluster = FullCluster::new().await.unwrap();
    let gas_type = GAS::type_().to_canonical_string(true);
    let sender = cluster.test_env.get_sender(0);
    let recipient = SuiAddress::random_for_testing_only();

    let (_, gas) = cluster.test_env.get_sender_and_gas(0);

    // Transfer 42 to recipient's address balance.
    let tx = cluster
        .test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(42, recipient)])
        .build();

    let (digest, fx) = cluster.test_env.exec_tx_directly(tx).await.unwrap();
    assert!(fx.status().is_ok(), "send_funds transaction failed");
    cluster
        .test_env
        .cluster
        .wait_for_tx_settlement(&[digest])
        .await;
    cluster.sync().await;

    // getCoins should return just the AB coin.
    let CoinsResponse {
        result: Page { data: coins, .. },
    } = cluster.get_coins(recipient, &gas_type, None, 10).await;

    assert_eq!(coins.len(), 1, "Expected one coin for recipient");

    let ab_coin = &coins[0];
    assert_eq!(ab_coin.balance, 42);
    let ab_coin_id = ab_coin.coin_object_id;

    // getObject should be able to resolve the AB coin.
    let ObjectResponse {
        result: obj_response,
    } = cluster.get_object(&ab_coin_id.to_string()).await;

    let data = obj_response
        .data
        .as_ref()
        .expect("Expected object data in getObject response");
    assert_eq!(data.object_id, ab_coin_id);
}

/// Fund an address balance and verify that sui_multiGetObjects resolves the address balance
/// coin's masked object IDs.
#[tokio::test]
async fn test_multi_get_objects_resolves_address_balance_coins() {
    let mut cluster = FullCluster::new().await.unwrap();
    let gas_type = GAS::type_().to_canonical_string(true);
    let sender = cluster.test_env.get_sender(0);
    let recipient = SuiAddress::random_for_testing_only();

    let (_, gas) = cluster.test_env.get_sender_and_gas(0);
    let gas_id = gas.0;
    let tx = cluster
        .test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(42, recipient)])
        .build();

    let (digest, fx) = cluster.test_env.exec_tx_directly(tx).await.unwrap();
    assert!(fx.status().is_ok(), "send_funds transaction failed");
    cluster
        .test_env
        .cluster
        .wait_for_tx_settlement(&[digest])
        .await;
    cluster.sync().await;

    let CoinsResponse {
        result: Page { data: coins, .. },
    } = cluster.get_coins(recipient, &gas_type, None, 10).await;

    assert_eq!(coins.len(), 1, "Expected one coin for recipient");

    let ab_coin_id = coins[0].coin_object_id.to_string();

    let MultiObjectResponse { result: responses } = cluster
        .multi_get_objects(&[
            &ab_coin_id,
            // an nonexistent object id.
            "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            gas_id.to_string().as_str(),
        ])
        .await;

    assert_eq!(responses.len(), 3);

    let data = responses[0]
        .data
        .as_ref()
        .expect("Expected object data for address balance coin");
    assert_eq!(data.object_id.to_string(), ab_coin_id);

    assert!(
        responses[1].error.is_some(),
        "Expected error for non-existent object"
    );
    assert!(responses[2].data.is_some(), "Expected data for gas object");
}

/// Test pagination where the AB coin is on the first page and fetching the next page via cursor
/// returns the correct results.
///
/// Setup: 4 real coins + 1 AB coin (highest balance). With limit=3, the first page should contain
/// the AB coin and 2 real coins. The store also has a 4th real coin that was trimmed because the
/// AB coin took a spot. The next page returns the remaining real coins from the store.
#[tokio::test]
async fn test_pagination_ab_coin_on_first_page() {
    let cluster = FullCluster::new().await.unwrap();
    let gas_type = GAS::type_().to_canonical_string(true);
    let sender = cluster.test_env.get_sender(0);
    let recipient = SuiAddress::random_for_testing_only();

    // Create 4 real coins for recipient with various balances.
    // Track gas refs from effects to avoid races with update_all_gas.
    let mut gas = cluster.test_env.get_sender_and_gas(0).1;
    for amount in [5000, 4000, 3000, 2000] {
        let tx = cluster
            .test_env
            .tx_builder_with_gas(sender, gas)
            .transfer_sui(Some(amount), recipient)
            .build();
        let (digest, fx) = cluster
            .test_env
            .cluster
            .sign_and_execute_transaction_directly(&tx)
            .await
            .unwrap();
        assert!(fx.status().is_ok());
        gas = fx.gas_object().unwrap().0;
        cluster
            .test_env
            .cluster
            .wait_for_tx_settlement(&[digest])
            .await;
    }

    // Fund recipient's address balance with a large amount so the AB coin sorts first
    let tx = cluster
        .test_env
        .tx_builder_with_gas(sender, gas)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(100_000, recipient)])
        .build();
    let (digest, fx) = cluster
        .test_env
        .cluster
        .sign_and_execute_transaction_directly(&tx)
        .await
        .unwrap();
    assert!(fx.status().is_ok());
    cluster
        .test_env
        .cluster
        .wait_for_tx_settlement(&[digest])
        .await;
    cluster.sync().await;

    // Page 1 with limit=3: AB coin (100K) should be first, followed by top real coins
    let CoinsResponse {
        result:
            Page {
                data: page1,
                has_next_page: has_next1,
                next_cursor: cursor1,
            },
    } = cluster.get_coins(recipient, &gas_type, None, 3).await;

    assert_eq!(page1.len(), 3);
    assert_eq!(
        page1[0].balance, 100_000,
        "AB coin should be first with highest balance"
    );
    assert_eq!(page1[1].balance, 5000,);
    assert_eq!(page1[2].balance, 4000,);
    assert!(has_next1, "Should have next page");

    // Page 2: should return remaining real coins
    let CoinsResponse {
        result:
            Page {
                data: page2,
                has_next_page: has_next2,
                ..
            },
    } = cluster.get_coins(recipient, &gas_type, cursor1, 10).await;

    assert_eq!(page2.len(), 2);
    assert_eq!(page2[0].balance, 3000,);
    assert_eq!(page2[1].balance, 2000,);
    assert!(!has_next2, "Should be last page");
}

/// Test pagination where the AB coin is trimmed from the first page due to overflow and reappears
/// on the second page.
///
/// Setup: 5 real coins (5000, 4000, 3000, 2000, 1000) + AB coin (2500). With limit=3, the store
/// returns [5000, 4000, 3000]. The AB coin (2500) is inserted at position 3 but truncated because
/// the page is full. On the next page, the AB coin reappears at the top since its balance (2500)
/// is higher than the remaining real coins (2000, 1000).
#[tokio::test]
async fn test_pagination_ab_coin_trimmed_to_next_page() {
    let cluster = FullCluster::new().await.unwrap();
    let gas_type = GAS::type_().to_canonical_string(true);
    let sender = cluster.test_env.get_sender(0);
    let recipient = SuiAddress::random_for_testing_only();

    let mut gas = cluster.test_env.get_sender_and_gas(0).1;
    for amount in [5000, 4000, 3000, 2000, 1000] {
        let tx = cluster
            .test_env
            .tx_builder_with_gas(sender, gas)
            .transfer_sui(Some(amount), recipient)
            .build();
        let (digest, fx) = cluster
            .test_env
            .cluster
            .sign_and_execute_transaction_directly(&tx)
            .await
            .unwrap();
        assert!(fx.status().is_ok());
        gas = fx.gas_object().unwrap().0;
        cluster
            .test_env
            .cluster
            .wait_for_tx_settlement(&[digest])
            .await;
    }

    let tx = cluster
        .test_env
        .tx_builder_with_gas(sender, gas)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(2500, recipient)])
        .build();
    let (digest, fx) = cluster
        .test_env
        .cluster
        .sign_and_execute_transaction_directly(&tx)
        .await
        .unwrap();
    assert!(fx.status().is_ok());
    cluster
        .test_env
        .cluster
        .wait_for_tx_settlement(&[digest])
        .await;
    cluster.sync().await;

    // Page 1: AB coin (2500) is trimmed; only real coins returned
    let CoinsResponse {
        result:
            Page {
                data: page1,
                has_next_page: has_next1,
                next_cursor: cursor1,
            },
    } = cluster.get_coins(recipient, &gas_type, None, 3).await;

    assert_eq!(page1.len(), 3);
    assert_eq!(page1[0].balance, 5000);
    assert_eq!(page1[1].balance, 4000);
    assert_eq!(page1[2].balance, 3000);
    assert!(has_next1);

    // Page 2: AB coin reappears at the top, followed by remaining real coins
    let CoinsResponse {
        result:
            Page {
                data: page2,
                has_next_page: has_next2,
                ..
            },
    } = cluster.get_coins(recipient, &gas_type, cursor1, 10).await;

    assert_eq!(page2.len(), 3);
    assert_eq!(page2[0].balance, 2500);
    assert_eq!(page2[1].balance, 2000);
    assert_eq!(page2[2].balance, 1000);
    assert!(!has_next2);
}

/// Test pagination where the AB coin naturally falls on the second page because its balance is
/// lower than all coins on the first page.
///
/// Setup: 4 real coins (5000, 4000, 3000, 2000) + AB coin (500). With limit=3, the first page
/// contains only the top 3 real coins. The AB coin appears on the second page after the remaining
/// real coin.
#[tokio::test]
async fn test_pagination_ab_coin_on_second_page() {
    let cluster = FullCluster::new().await.unwrap();
    let gas_type = GAS::type_().to_canonical_string(true);
    let sender = cluster.test_env.get_sender(0);
    let recipient = SuiAddress::random_for_testing_only();

    let mut gas = cluster.test_env.get_sender_and_gas(0).1;
    for amount in [5000, 4000, 3000, 2000] {
        let tx = cluster
            .test_env
            .tx_builder_with_gas(sender, gas)
            .transfer_sui(Some(amount), recipient)
            .build();
        let (digest, fx) = cluster
            .test_env
            .cluster
            .sign_and_execute_transaction_directly(&tx)
            .await
            .unwrap();
        assert!(fx.status().is_ok());
        gas = fx.gas_object().unwrap().0;
        cluster
            .test_env
            .cluster
            .wait_for_tx_settlement(&[digest])
            .await;
    }

    let tx = cluster
        .test_env
        .tx_builder_with_gas(sender, gas)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(500, recipient)])
        .build();
    let (digest, fx) = cluster
        .test_env
        .cluster
        .sign_and_execute_transaction_directly(&tx)
        .await
        .unwrap();
    assert!(fx.status().is_ok());
    cluster
        .test_env
        .cluster
        .wait_for_tx_settlement(&[digest])
        .await;
    cluster.sync().await;

    // Page 1: only real coins, AB coin (500) sorts well below page boundary
    let CoinsResponse {
        result:
            Page {
                data: page1,
                has_next_page: has_next1,
                next_cursor: cursor1,
            },
    } = cluster.get_coins(recipient, &gas_type, None, 3).await;

    assert_eq!(page1.len(), 3);
    assert_eq!(page1[0].balance, 5000);
    assert_eq!(page1[1].balance, 4000);
    assert_eq!(page1[2].balance, 3000);
    assert!(has_next1);

    // Page 2: remaining real coin followed by AB coin
    let CoinsResponse {
        result:
            Page {
                data: page2,
                has_next_page: has_next2,
                ..
            },
    } = cluster.get_coins(recipient, &gas_type, cursor1, 10).await;

    assert_eq!(page2.len(), 2);
    assert_eq!(page2[0].balance, 2000);
    assert_eq!(page2[1].balance, 500);
    assert!(!has_next2);
}

/// Test pagination where the AB coin's encoded key is used as the cursor.
///
/// When the cursor equals the AB coin's key, the AB coin should be excluded (the cursor comparison
/// is strictly greater-than). Only coins that sort after the AB coin should be returned.
#[tokio::test]
async fn test_pagination_ab_coin_as_cursor() {
    let cluster = FullCluster::new().await.unwrap();
    let gas_type = GAS::type_().to_canonical_string(true);
    let sender = cluster.test_env.get_sender(0);
    let recipient = SuiAddress::random_for_testing_only();

    // Create 2 real coins: one with higher balance, one with lower.
    // Track gas refs from effects to avoid races with update_all_gas.
    let mut gas = cluster.test_env.get_sender_and_gas(0).1;
    for amount in [5000, 1000] {
        let tx = cluster
            .test_env
            .tx_builder_with_gas(sender, gas)
            .transfer_sui(Some(amount), recipient)
            .build();
        let (digest, fx) = cluster
            .test_env
            .cluster
            .sign_and_execute_transaction_directly(&tx)
            .await
            .unwrap();
        assert!(fx.status().is_ok());
        gas = fx.gas_object().unwrap().0;
        cluster
            .test_env
            .cluster
            .wait_for_tx_settlement(&[digest])
            .await;
    }

    // Fund address balance between the two real coins
    let tx = cluster
        .test_env
        .tx_builder_with_gas(sender, gas)
        .transfer_sui_to_address_balance(FundSource::coin(gas), vec![(3000, recipient)])
        .build();
    let (digest, fx) = cluster
        .test_env
        .cluster
        .sign_and_execute_transaction_directly(&tx)
        .await
        .unwrap();
    assert!(fx.status().is_ok());
    cluster
        .test_env
        .cluster
        .wait_for_tx_settlement(&[digest])
        .await;
    cluster.sync().await;

    // Page 1 with limit = 2: real coin with 5000 balance and AB coin with 3000 balance
    let CoinsResponse {
        result:
            Page {
                data: page1,
                has_next_page: has_next1,
                next_cursor: cursor1,
            },
    } = cluster.get_coins(recipient, &gas_type, None, 2).await;

    assert_eq!(page1.len(), 2);
    assert_eq!(page1[0].balance, 5000,);
    assert_eq!(page1[1].balance, 3000,);
    assert!(cursor1.is_some());
    assert!(has_next1, "Should have next page");

    let ab_coin = &page1[1];

    // Construct cursor from the AB coin's encoded key
    // and check that it is equal to the returned next_cursor
    let ab_key = ObjectByOwnerKey::from_coin_parts(
        &Owner::AddressOwner(recipient),
        sui_types::coin::Coin::type_(GAS::type_tag()),
        ab_coin.balance,
        ab_coin.coin_object_id,
    )
    .encode();
    let ab_cursor = Base64::encode(bcs::to_bytes(&ab_key).unwrap());
    assert_eq!(
        ab_cursor,
        cursor1.clone().unwrap(),
        "Returned cursor should equal AB coin's encoded key"
    );

    // Page 2: should return remaining real coins
    let CoinsResponse {
        result:
            Page {
                data: page2,
                has_next_page: has_next2,
                ..
            },
    } = cluster.get_coins(recipient, &gas_type, cursor1, 1).await;

    // Only the coin that sort after the AB coin (lower balance) should be returned
    assert_eq!(page2.len(), 1);
    assert_eq!(page2[0].balance, 1000,);
    assert!(!has_next2, "Should be last page");
}

/// End-to-end test: get an AB coin from the RPC, extract its masked ObjectRef, and use it as a
/// coin reservation input in a transaction submitted to real validators.
///
/// Address balance changes are verified directly via the fullnode's accumulator state.
#[tokio::test]
async fn test_use_ab_coin_as_transaction_input() {
    let mut cluster = FullCluster::new().await.unwrap();
    let gas_type = GAS::type_().to_canonical_string(true);
    let sender = cluster.test_env.get_sender(0);
    let recipient = SuiAddress::random_for_testing_only();

    // Fund sender's address balance
    let fund_amount = 123_000_000u64;
    cluster
        .test_env
        .fund_one_address_balance(sender, fund_amount)
        .await;
    cluster.sync().await;

    // Verify address balance via fullnode state
    assert_eq!(
        cluster.test_env.get_sui_balance_ab(sender),
        fund_amount,
        "Sender's address balance should equal funded amount"
    );

    // Get the AB coin from the JSON-RPC alt stack
    let CoinsResponse {
        result: Page { data: coins, .. },
    } = cluster.get_coins(sender, &gas_type, None, 10).await;

    let ab_coin = coins
        .iter()
        .find(|c| c.balance == fund_amount)
        .expect("Expected AB coin in getCoins response");

    // The digest field contains the coin reservation digest, not a regular object digest.
    let ab_ref: ObjectRef = (ab_coin.coin_object_id, ab_coin.version, ab_coin.digest);

    // Use the masked ObjectRef as a coin reservation input
    let transfer_amount = 42;
    let tx = cluster
        .test_env
        .tx_builder(sender)
        .transfer_sui_to_address_balance(
            FundSource::coin(ab_ref),
            vec![(transfer_amount, recipient)],
        )
        .build();

    let (digest, fx) = cluster.test_env.exec_tx_directly(tx).await.unwrap();
    assert!(
        fx.status().is_ok(),
        "Transfer with coin reservation input failed: {:?}",
        fx.status()
    );
    cluster
        .test_env
        .cluster
        .wait_for_tx_settlement(&[digest])
        .await;
    cluster.sync().await;

    // Verify address balance changes via fullnode state
    assert_eq!(
        cluster.test_env.get_sui_balance_ab(sender),
        fund_amount - transfer_amount,
        "Sender's address balance should have decreased by the transfer amount"
    );
    assert_eq!(
        cluster.test_env.get_sui_balance_ab(recipient),
        transfer_amount,
        "Recipient's address balance should equal the transfer amount"
    );

    // Secondary verification via getCoins
    let CoinsResponse {
        result: Page {
            data: coins_after, ..
        },
    } = cluster.get_coins(sender, &gas_type, None, 10).await;

    let ab_after = coins_after
        .iter()
        .find(|c| c.coin_object_id == ab_coin.coin_object_id)
        .expect("Expected AB coin after transfer");
    assert_eq!(ab_after.balance, fund_amount - transfer_amount);

    let CoinsResponse {
        result: Page {
            data: recipient_coins,
            ..
        },
    } = cluster.get_coins(recipient, &gas_type, None, 10).await;
    let recipient_balance: u64 = recipient_coins.iter().map(|c| c.balance).sum();
    assert_eq!(recipient_balance, transfer_amount);
}

/// End-to-end test: get an AB coin from the RPC and use its masked ObjectRef as the gas payment.
/// The real validator resolves the coin reservation and deducts gas from the address balance.
///
/// Address balance changes are verified directly via the fullnode's accumulator state.
#[tokio::test]
async fn test_use_ab_coin_as_gas() {
    let mut cluster = FullCluster::new().await.unwrap();
    let gas_type = GAS::type_().to_canonical_string(true);
    let sender = cluster.test_env.get_sender(0);
    let recipient = SuiAddress::random_for_testing_only();

    // Fund sender's address balance with enough to cover gas + transfer
    let fund_amount = 10_000_000_000u64;
    cluster
        .test_env
        .fund_one_address_balance(sender, fund_amount)
        .await;
    cluster.sync().await;

    // Verify address balance via fullnode state
    assert_eq!(
        cluster.test_env.get_sui_balance_ab(sender),
        fund_amount,
        "Sender's address balance should equal funded amount"
    );

    // Get the AB coin from the JSON-RPC alt stack
    let CoinsResponse {
        result: Page { data: coins, .. },
    } = cluster.get_coins(sender, &gas_type, None, 10).await;

    let ab_coin = coins
        .iter()
        .find(|c| c.balance == fund_amount)
        .expect("Expected AB coin");

    let ab_ref: ObjectRef = (ab_coin.coin_object_id, ab_coin.version, ab_coin.digest);

    // Use the AB coin as gas for a simple transfer_sui (SplitCoins from GasCoin).
    let transfer_amount = 1_000_000u64;
    let tx = cluster
        .test_env
        .tx_builder_with_gas(sender, ab_ref)
        .transfer_sui(Some(transfer_amount), recipient)
        .build();

    let (digest, fx) = cluster.test_env.exec_tx_directly(tx).await.unwrap();
    assert!(
        fx.status().is_ok(),
        "Transfer with AB coin as gas failed: {:?}",
        fx.status()
    );
    let gas_used = fx.gas_cost_summary().gas_used();

    cluster
        .test_env
        .cluster
        .wait_for_tx_settlement(&[digest])
        .await;
    cluster.sync().await;

    // Verify address balance via fullnode state
    assert_eq!(
        cluster.test_env.get_sui_balance_ab(sender),
        fund_amount - gas_used - transfer_amount,
        "Address balance should decrease by gas used + transfer amount"
    );

    // Verify recipient received funds as a regular coin
    let CoinsResponse {
        result: Page {
            data: recipient_coins,
            ..
        },
    } = cluster.get_coins(recipient, &gas_type, None, 10).await;
    let recipient_balance: u64 = recipient_coins.iter().map(|c| c.balance).sum();
    assert_eq!(recipient_balance, transfer_amount);
}
