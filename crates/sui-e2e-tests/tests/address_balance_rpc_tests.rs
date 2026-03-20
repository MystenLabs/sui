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
use sui_simulator::has_mainnet_protocol_config_override;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    coin_reservation,
};
use test_cluster::addr_balance_test_env::TestEnvBuilder;

#[sim_test]
async fn test_rpc_get_object_returns_fake_coin() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    // Test that the JSON-RPC getObject endpoint returns a fake coin object
    // when given a masked object ID representing an address balance.

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);
    let address_balance_amount = 1_000_000_000u64;

    // Fund sender's address balance
    test_env
        .fund_one_address_balance(sender, address_balance_amount)
        .await;

    // Get the fake coin object ref (masked ID)
    let fake_coin_ref = test_env.encode_coin_reservation(sender, 0, address_balance_amount);
    let masked_object_id = fake_coin_ref.0;

    // The masked ID should be different from the unmasked accumulator object ID
    let unmasked_id = coin_reservation::mask_or_unmask_id(masked_object_id, test_env.chain_id);
    assert_ne!(masked_object_id, unmasked_id);

    // Query the RPC endpoint with the masked object ID
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

    // The response should contain the fake coin object
    let object_data = response.data.expect("Expected object data");
    assert_eq!(object_data.object_id, masked_object_id);

    // Verify the object is a coin and has the expected balance
    let content = object_data.content.expect("Expected content");
    let fields = content.try_into_move().expect("Expected move object");
    assert!(
        fields
            .type_
            .to_string()
            .contains("0x2::coin::Coin<0x2::sui::SUI>")
    );

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_rpc_get_coins_includes_fake_coin_at_position_1() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    // Test that the JSON-RPC getCoins endpoint includes the fake coin
    // at position 1 (second position, after the first real coin).

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);
    let address_balance_amount = 5_000_000_000u64;

    // Get the initial coin count
    let params = rpc_params![
        sender,
        Option::<String>::None,
        Option::<String>::None,
        Option::<usize>::None
    ];
    let initial_coins: CoinPage = test_env
        .cluster
        .fullnode_handle
        .rpc_client
        .request("suix_getCoins", params)
        .await
        .unwrap();
    let initial_coin_count = initial_coins.data.len();
    assert!(initial_coin_count >= 1, "Need at least one real coin");

    // Fund sender's address balance
    test_env
        .fund_one_address_balance(sender, address_balance_amount)
        .await;

    // Get the fake coin object ref
    let fake_coin_ref = test_env.encode_coin_reservation(sender, 0, address_balance_amount);
    let masked_object_id = fake_coin_ref.0;

    // Query the RPC endpoint for coins
    let params = rpc_params![
        sender,
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

    // Should have one more coin than before (the fake coin)
    assert_eq!(
        coins.data.len(),
        initial_coin_count + 1,
        "Should have one additional fake coin"
    );

    // The fake coin should be at position 1 (second position)
    assert_eq!(
        coins.data[1].coin_object_id, masked_object_id,
        "Fake coin should be at position 1"
    );
    assert_eq!(coins.data[1].balance, address_balance_amount);

    // The first coin should be a real coin (not the fake one)
    assert_ne!(
        coins.data[0].coin_object_id, masked_object_id,
        "First coin should be a real coin, not the fake one"
    );

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_rpc_get_coins_pagination_handles_fake_coin() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    // Test that pagination works correctly with the fake coin at position 1.
    // When paginating past the fake coin, it should not appear again.

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);
    let address_balance_amount = 5_000_000_000u64;

    // Fund sender's address balance
    test_env
        .fund_one_address_balance(sender, address_balance_amount)
        .await;

    // Get the fake coin object ref
    let fake_coin_ref = test_env.encode_coin_reservation(sender, 0, address_balance_amount);
    let masked_object_id = fake_coin_ref.0;

    // Get first page with limit 2 (should be [real, fake])
    let params = rpc_params![
        sender,
        Option::<String>::None,
        Option::<String>::None,
        Some(2usize)
    ];
    let page1: CoinPage = test_env
        .cluster
        .fullnode_handle
        .rpc_client
        .request("suix_getCoins", params)
        .await
        .unwrap();

    assert_eq!(page1.data.len(), 2, "First page should have 2 coins");
    assert_eq!(
        page1.data[1].coin_object_id, masked_object_id,
        "Fake coin should be at position 1"
    );

    // Get second page using cursor from first page (should not include fake coin)
    if let Some(cursor) = page1.next_cursor {
        let params = rpc_params![sender, Option::<String>::None, Some(cursor), Some(10usize)];
        let page2: CoinPage = test_env
            .cluster
            .fullnode_handle
            .rpc_client
            .request("suix_getCoins", params)
            .await
            .unwrap();

        // The fake coin should NOT appear in page 2
        let has_fake = page2
            .data
            .iter()
            .any(|c| c.coin_object_id == masked_object_id);
        assert!(
            !has_fake,
            "Fake coin should not appear again in subsequent pages"
        );
    }

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_rpc_get_balance_includes_address_balance() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    // Test that the JSON-RPC getBalance endpoint includes address balance
    // in the total balance.

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);
    let address_balance_amount = 3_000_000_000u64;

    // Get the initial balance
    let params = rpc_params![sender, Option::<String>::None];
    let initial_balance: RpcBalance = test_env
        .cluster
        .fullnode_handle
        .rpc_client
        .request("suix_getBalance", params)
        .await
        .unwrap();
    let initial_total = initial_balance.total_balance;
    let initial_coin_count = initial_balance.coin_object_count;

    // Fund sender's address balance
    test_env
        .fund_one_address_balance(sender, address_balance_amount)
        .await;

    // Get the updated balance
    let params = rpc_params![sender, Option::<String>::None];
    let updated_balance: RpcBalance = test_env
        .cluster
        .fullnode_handle
        .rpc_client
        .request("suix_getBalance", params)
        .await
        .unwrap();

    // The total balance should be roughly the same (minus gas costs) since we're
    // just moving funds from coin to address balance.
    assert!(
        updated_balance.total_balance >= initial_total - 10_000_000,
        "Total balance should be roughly the same (allowing for gas costs). \
        Initial: {}, Updated: {}",
        initial_total,
        updated_balance.total_balance
    );

    // Coin count should have increased by 1 (the fake coin representing the address balance)
    assert_eq!(
        updated_balance.coin_object_count,
        initial_coin_count + 1,
        "Coin count should have increased by 1 (fake coin). \
        Initial: {}, Updated: {}",
        initial_coin_count,
        updated_balance.coin_object_count
    );

    // The funds_in_address_balance field should reflect the address balance
    assert_eq!(
        updated_balance.funds_in_address_balance, address_balance_amount as u128,
        "Address balance should be reported"
    );

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_rpc_get_coins_pagination_consistency() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    // Test that pagination returns identical results regardless of page size.
    // We fetch all coins with different page sizes (1, 2, 3, ...) and verify
    // the final collected results are identical.

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);
    let address_balance_amount = 5_000_000_000u64;

    // Fund sender's address balance to add the fake coin
    test_env
        .fund_one_address_balance(sender, address_balance_amount)
        .await;

    // Helper function to fetch all coins with a given page size
    async fn fetch_all_coins(
        test_env: &test_cluster::addr_balance_test_env::TestEnv,
        sender: SuiAddress,
        page_size: usize,
    ) -> Vec<ObjectID> {
        let mut all_coins = vec![];
        let mut cursor: Option<String> = None;

        loop {
            let params = rpc_params![
                sender,
                Option::<String>::None,
                cursor.clone(),
                Some(page_size)
            ];
            let page: CoinPage = test_env
                .cluster
                .fullnode_handle
                .rpc_client
                .request("suix_getCoins", params)
                .await
                .unwrap();

            for coin in &page.data {
                all_coins.push(coin.coin_object_id);
            }

            if page.has_next_page {
                cursor = page.next_cursor;
            } else {
                break;
            }
        }

        all_coins
    }

    // Fetch all coins with page size = total (no pagination)
    let all_at_once = fetch_all_coins(&test_env, sender, 100).await;
    assert!(!all_at_once.is_empty(), "Should have at least one coin");

    // Verify results are identical for different page sizes
    for page_size in 1..=all_at_once.len() + 1 {
        let paginated = fetch_all_coins(&test_env, sender, page_size).await;
        assert_eq!(
            all_at_once,
            paginated,
            "Results should be identical with page_size={}, expected {} coins, got {}",
            page_size,
            all_at_once.len(),
            paginated.len()
        );
    }

    test_env.cluster.trigger_reconfiguration().await;
}
