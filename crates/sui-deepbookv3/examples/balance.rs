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

    let assets = vec!["SUI", "USDC", "WUSDT", "WUSDC", "BETH", "DEEP"];
    let manager = "MANAGER_1";
    println!("Manager: {}", manager);

    for asset in assets {
        let balance = db_client.check_manager_balance(manager, asset).await?;
        println!("{:?}", balance);
    }

    Ok(())
}
