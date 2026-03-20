// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests in this file use the JSON-RPC client (rpc_client), not the gRPC client.
// The rpc_client is deprecated but we need it to test JSON-RPC endpoints.
#![allow(deprecated)]

use jsonrpsee::core::client::ClientT;
use jsonrpsee::rpc_params;
use sui_json_rpc_types::{
    Balance as RpcBalance, CoinPage, SuiData, SuiObjectDataOptions, SuiObjectResponse,
};
use sui_macros::*;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    coin_reservation::ParsedDigest,
};
use test_cluster::addr_balance_test_env::{CoinTypeConfig, TestEnv, TestEnvBuilder};

/// A test scenario specifying the coin setup for SUI and optionally a custom coin type.
#[derive(Clone, Debug)]
struct TestScenario {
    /// SUI coin configuration. Note: test cluster creates 5 gas coins by default.
    sui: CoinTypeConfig,
    /// Optional custom coin type configuration.
    custom_coin: Option<CoinTypeConfig>,
}

/// Expected results after running a scenario.
#[derive(Debug, PartialEq, Eq)]
struct ExpectedCounts {
    real_coins: usize,
    fake_coins: usize,
}

impl TestScenario {
    /// Calculate expected counts for getCoins (SUI only).
    fn expected_sui_counts(&self, base_sui_coins: usize) -> ExpectedCounts {
        let real = base_sui_coins + self.sui.real_coins;
        let fake = if self.sui.has_address_balance { 1 } else { 0 };
        ExpectedCounts {
            real_coins: real,
            fake_coins: fake,
        }
    }

    /// Calculate expected counts for getAllCoins (all types).
    fn expected_all_counts(&self, base_sui_coins: usize) -> ExpectedCounts {
        let mut real = base_sui_coins + self.sui.real_coins;
        let mut fake = if self.sui.has_address_balance { 1 } else { 0 };

        if let Some(ref custom) = self.custom_coin {
            real += custom.real_coins;
            if custom.has_address_balance {
                fake += 1;
            }
        }

        ExpectedCounts {
            real_coins: real,
            fake_coins: fake,
        }
    }
}

/// Set up a test scenario by transferring coins to a fresh address.
async fn setup_scenario(
    test_env: &mut TestEnv,
    scenario: &TestScenario,
) -> (SuiAddress, Option<String>) {
    let (funder, _) = test_env.get_sender_and_gas(0);

    // Create a fresh address to receive coins
    let recipient = SuiAddress::random_for_testing_only();

    // Transfer SUI coins to recipient
    for _ in 0..scenario.sui.real_coins {
        test_env
            .transfer_sui(funder, recipient, 1_000_000_000)
            .await;
    }

    // Fund SUI address balance if configured
    if scenario.sui.has_address_balance {
        test_env
            .transfer_sui_to_address_balance(funder, recipient, 1_000_000_000)
            .await;
    }

    // Set up custom coin type if configured
    let custom_coin_type = if let Some(ref custom) = scenario.custom_coin {
        let (_, coin_type) = test_env
            .publish_trusted_coin_and_setup(funder, recipient, custom, 1_000_000)
            .await;
        Some(coin_type.to_string())
    } else {
        None
    };

    (recipient, custom_coin_type)
}

/// Query getCoins and return counts of real and fake coins.
async fn get_coins_counts(test_env: &TestEnv, owner: SuiAddress) -> ExpectedCounts {
    let params = rpc_params![
        owner,
        Option::<String>::None,
        Option::<String>::None,
        Option::<usize>::None
    ];
    let coins: CoinPage = test_env
        .cluster
        .fullnode_handle
        .rpc_client
        .request("suix_getCoins", params)
        .await
        .unwrap();

    count_real_and_fake(&coins)
}

/// Query getAllCoins and return counts of real and fake coins.
async fn get_all_coins_counts(test_env: &TestEnv, owner: SuiAddress) -> ExpectedCounts {
    let params = rpc_params![owner, Option::<String>::None, Option::<usize>::None];
    let coins: CoinPage = test_env
        .cluster
        .fullnode_handle
        .rpc_client
        .request("suix_getAllCoins", params)
        .await
        .unwrap();

    count_real_and_fake(&coins)
}

fn count_real_and_fake(coins: &CoinPage) -> ExpectedCounts {
    let fake_coins = coins
        .data
        .iter()
        .filter(|c| ParsedDigest::is_coin_reservation_digest(&c.digest))
        .count();
    let real_coins = coins.data.len() - fake_coins;
    ExpectedCounts {
        real_coins,
        fake_coins,
    }
}

/// Verify fake coin ordering: fake coins should be at position 1 within each type,
/// or at position 0 if no real coins exist for that type.
fn verify_fake_coin_ordering(coins: &CoinPage) {
    let mut current_type: Option<String> = None;
    let mut position_in_type = 0;
    let mut has_real_for_type = false;

    for coin in &coins.data {
        let is_fake = ParsedDigest::is_coin_reservation_digest(&coin.digest);

        if current_type.as_ref() != Some(&coin.coin_type) {
            current_type = Some(coin.coin_type.clone());
            position_in_type = 0;
            has_real_for_type = false;
        }

        if is_fake {
            if has_real_for_type {
                assert_eq!(
                    position_in_type, 1,
                    "Fake coin for type {} should be at position 1 (after first real), but was at {}",
                    coin.coin_type, position_in_type
                );
            } else {
                assert_eq!(
                    position_in_type, 0,
                    "Fake coin for type {} (no real coins) should be at position 0, but was at {}",
                    coin.coin_type, position_in_type
                );
            }
        } else {
            has_real_for_type = true;
        }

        position_in_type += 1;
    }
}

// =============================================================================
// Data-driven scenario tests
// =============================================================================

#[sim_test]
async fn test_scenario_sui_real_only() {
    // SUI: real coins only, no address balance
    let scenario = TestScenario {
        sui: CoinTypeConfig {
            real_coins: 2,
            has_address_balance: false,
        },
        custom_coin: None,
    };
    run_scenario(scenario).await;
}

#[sim_test]
async fn test_scenario_sui_with_address_balance() {
    // SUI: real coins + address balance
    let scenario = TestScenario {
        sui: CoinTypeConfig {
            real_coins: 2,
            has_address_balance: true,
        },
        custom_coin: None,
    };
    run_scenario(scenario).await;
}

#[sim_test]
async fn test_scenario_sui_address_balance_only() {
    // SUI: address balance only, no additional real coins (just base coins)
    let scenario = TestScenario {
        sui: CoinTypeConfig {
            real_coins: 0,
            has_address_balance: true,
        },
        custom_coin: None,
    };
    run_scenario(scenario).await;
}

#[sim_test]
async fn test_scenario_two_types_both_with_real_and_fake() {
    // SUI + custom: both have real coins and address balance
    let scenario = TestScenario {
        sui: CoinTypeConfig {
            real_coins: 1,
            has_address_balance: true,
        },
        custom_coin: Some(CoinTypeConfig {
            real_coins: 1,
            has_address_balance: true,
        }),
    };
    run_scenario(scenario).await;
}

#[sim_test]
async fn test_scenario_two_types_custom_address_balance_only() {
    // SUI: real + fake, Custom: address balance only (no real coins)
    // This tests the bug fix where fake coins for types without real coins were omitted
    let scenario = TestScenario {
        sui: CoinTypeConfig {
            real_coins: 1,
            has_address_balance: true,
        },
        custom_coin: Some(CoinTypeConfig {
            real_coins: 0,
            has_address_balance: true,
        }),
    };
    run_scenario(scenario).await;
}

#[sim_test]
async fn test_scenario_two_types_custom_real_only() {
    // SUI: real + fake, Custom: real only (no address balance)
    let scenario = TestScenario {
        sui: CoinTypeConfig {
            real_coins: 1,
            has_address_balance: true,
        },
        custom_coin: Some(CoinTypeConfig {
            real_coins: 2,
            has_address_balance: false,
        }),
    };
    run_scenario(scenario).await;
}

#[sim_test]
async fn test_scenario_two_types_no_address_balances() {
    // SUI + custom: both have real coins only, no address balances
    let scenario = TestScenario {
        sui: CoinTypeConfig {
            real_coins: 1,
            has_address_balance: false,
        },
        custom_coin: Some(CoinTypeConfig {
            real_coins: 1,
            has_address_balance: false,
        }),
    };
    run_scenario(scenario).await;
}

async fn run_scenario(scenario: TestScenario) {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (recipient, _custom_type) = setup_scenario(&mut test_env, &scenario).await;

    // For a fresh recipient, base SUI coins = 0 (unless we transferred some)
    let base_sui_coins = 0;

    // Test getCoins (SUI only)
    let sui_counts = get_coins_counts(&test_env, recipient).await;
    let expected_sui = scenario.expected_sui_counts(base_sui_coins);
    assert_eq!(
        sui_counts, expected_sui,
        "getCoins mismatch for scenario {:?}",
        scenario
    );

    // Test getAllCoins (all types)
    let all_counts = get_all_coins_counts(&test_env, recipient).await;
    let expected_all = scenario.expected_all_counts(base_sui_coins);
    assert_eq!(
        all_counts, expected_all,
        "getAllCoins mismatch for scenario {:?}",
        scenario
    );

    // Verify ordering
    let params = rpc_params![recipient, Option::<String>::None, Option::<usize>::None];
    let coins: CoinPage = test_env
        .cluster
        .fullnode_handle
        .rpc_client
        .request("suix_getAllCoins", params)
        .await
        .unwrap();
    verify_fake_coin_ordering(&coins);
}

// =============================================================================
// Pagination tests
// =============================================================================

#[sim_test]
async fn test_pagination_no_duplicate_fake_coins() {
    // Verify fake coins don't appear again in subsequent pages
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);

    // Fund address balance
    test_env
        .fund_one_address_balance(sender, 5_000_000_000)
        .await;

    // Fetch all coins with pagination using page size 2
    let mut all_coin_ids: Vec<ObjectID> = vec![];
    let mut cursor: Option<String> = None;

    loop {
        let params = rpc_params![sender, Option::<String>::None, cursor.clone(), Some(2usize)];
        let page: CoinPage = test_env
            .cluster
            .fullnode_handle
            .rpc_client
            .request("suix_getCoins", params)
            .await
            .unwrap();

        for coin in &page.data {
            assert!(
                !all_coin_ids.contains(&coin.coin_object_id),
                "Duplicate coin found: {:?}",
                coin.coin_object_id
            );
            all_coin_ids.push(coin.coin_object_id);
        }

        if page.has_next_page {
            cursor = page.next_cursor;
        } else {
            break;
        }
    }

    // Verify we got exactly one fake coin
    let fake_count = all_coin_ids.len() - get_coins_counts(&test_env, sender).await.real_coins;
    assert_eq!(fake_count, 1, "Should have exactly one fake coin");
}

#[sim_test]
async fn test_pagination_consistency_get_all_coins() {
    // Verify paginated getAllCoins returns same results as fetching all at once
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (funder, _) = test_env.get_sender_and_gas(0);

    // Create custom coin type with address balance
    let recipient = SuiAddress::random_for_testing_only();
    test_env
        .transfer_sui(funder, recipient, 1_000_000_000)
        .await;
    test_env
        .transfer_sui_to_address_balance(funder, recipient, 1_000_000_000)
        .await;

    let custom_config = CoinTypeConfig {
        real_coins: 1,
        has_address_balance: true,
    };
    test_env
        .publish_trusted_coin_and_setup(funder, recipient, &custom_config, 1_000_000)
        .await;

    // Fetch all at once
    let params = rpc_params![recipient, Option::<String>::None, Some(100usize)];
    let all_at_once: CoinPage = test_env
        .cluster
        .fullnode_handle
        .rpc_client
        .request("suix_getAllCoins", params)
        .await
        .unwrap();

    // Fetch with pagination
    let mut paginated_ids: Vec<ObjectID> = vec![];
    let mut cursor: Option<String> = None;
    loop {
        let params = rpc_params![recipient, cursor.clone(), Some(2usize)];
        let page: CoinPage = test_env
            .cluster
            .fullnode_handle
            .rpc_client
            .request("suix_getAllCoins", params)
            .await
            .unwrap();

        for coin in &page.data {
            paginated_ids.push(coin.coin_object_id);
        }

        if page.has_next_page {
            cursor = page.next_cursor;
        } else {
            break;
        }
    }

    let all_at_once_ids: Vec<ObjectID> =
        all_at_once.data.iter().map(|c| c.coin_object_id).collect();
    assert_eq!(
        all_at_once_ids, paginated_ids,
        "Paginated results should match all-at-once results"
    );
}

// =============================================================================
// Other specific behavior tests
// =============================================================================

#[sim_test]
async fn test_get_object_returns_fake_coin() {
    // Test that sui_getObject returns a fake coin object for a masked object ID
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);
    let amount = 1_000_000_000u64;

    test_env.fund_one_address_balance(sender, amount).await;

    let fake_coin_ref = test_env.encode_coin_reservation(sender, 0, amount);
    let masked_object_id = fake_coin_ref.0;

    let params = rpc_params![
        masked_object_id,
        SuiObjectDataOptions::new().with_content().with_owner()
    ];
    let response: SuiObjectResponse = test_env
        .cluster
        .fullnode_handle
        .rpc_client
        .request("sui_getObject", params)
        .await
        .unwrap();

    let object_data = response.data.expect("Expected object data");
    assert_eq!(object_data.object_id, masked_object_id);

    let content = object_data.content.expect("Expected content");
    let fields = content.try_into_move().expect("Expected move object");
    assert!(
        fields
            .type_
            .to_string()
            .contains("0x2::coin::Coin<0x2::sui::SUI>")
    );
}

#[sim_test]
async fn test_get_balance_includes_address_balance() {
    // Test that getBalance includes address balance in the total
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);
    let amount = 3_000_000_000u64;

    let params = rpc_params![sender, Option::<String>::None];
    let initial: RpcBalance = test_env
        .cluster
        .fullnode_handle
        .rpc_client
        .request("suix_getBalance", params)
        .await
        .unwrap();

    test_env.fund_one_address_balance(sender, amount).await;

    let params = rpc_params![sender, Option::<String>::None];
    let updated: RpcBalance = test_env
        .cluster
        .fullnode_handle
        .rpc_client
        .request("suix_getBalance", params)
        .await
        .unwrap();

    // Total should be roughly the same (minus gas)
    assert!(
        updated.total_balance >= initial.total_balance - 10_000_000,
        "Total balance changed unexpectedly"
    );

    // Coin count should increase by 1 (the fake coin)
    assert_eq!(
        updated.coin_object_count,
        initial.coin_object_count + 1,
        "Coin count should increase by 1"
    );

    // Address balance should be reported
    assert_eq!(
        updated.funds_in_address_balance, amount as u128,
        "Address balance should be reported"
    );
}
