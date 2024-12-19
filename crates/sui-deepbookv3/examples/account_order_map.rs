// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use std::collections::HashMap;
use sui_deepbookv3::utils::types::BalanceManager;
use sui_sdk::types::base_types::SuiAddress;
use sui_sdk::SuiClientBuilder;

use sui_deepbookv3::client::DeepBookClient;
use sui_deepbookv3::utils::config::Environment;

#[tokio::main]
async fn main() -> Result<()> {
    let env = Environment::Mainnet;
    let fullnode_url = "https://fullnode.mainnet.sui.io:443"; // Mainnet URL

    let mut balance_managers = HashMap::new();
    balance_managers.insert(
        "MANAGER_1",
        BalanceManager {
            address: "0x344c2734b1d211bd15212bfb7847c66a3b18803f3f5ab00f5ff6f87b6fe6d27d"
                .to_string(),
            trade_cap: None,
        },
    );

    let sui_client = SuiClientBuilder::default()
        .build(fullnode_url)
        .await
        .unwrap();
    let db_client = DeepBookClient::new(
        sui_client,
        SuiAddress::random_for_testing_only(),
        env,
        Some(balance_managers),
        None,
        None,
        None,
    );

    let pools = vec![
        "SUI_USDC",
        "DEEP_SUI",
        "DEEP_USDC",
        "WUSDT_USDC",
        "WUSDC_USDC",
        "BETH_USDC",
    ];
    let manager = "MANAGER_1";
    println!("Manager: {}", manager);

    for pool in pools {
        let orders = db_client.account_open_orders(pool, manager).await?;
        let mut bid_orders: Vec<(f64, f64)> = Vec::new();
        let mut ask_orders: Vec<(f64, f64)> = Vec::new();

        for order_id in orders {
            if let Some(order) = db_client
                .get_order_normalized(pool, order_id)
                .await?
            {
                let remaining_quantity = order.quantity.parse::<f64>().unwrap()
                    - order.filled_quantity.parse::<f64>().unwrap();
                let order_price = order.normalized_price.parse::<f64>().unwrap();

                if order.is_bid {
                    bid_orders.push((order_price, remaining_quantity));
                } else {
                    ask_orders.push((order_price, remaining_quantity));
                }
            }
        }

        // Sort bids in descending order
        bid_orders.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        // Sort asks in ascending order
        ask_orders.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        println!("{} bid orders: {:?}", pool, bid_orders);
        println!("{} ask orders: {:?}", pool, ask_orders);
    }

    Ok(())
}
