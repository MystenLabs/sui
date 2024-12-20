use std::{collections::HashMap, str::FromStr};

use sui_deepbookv3::{
    transactions::balance_manager::BalanceManagerContract,
    utils::{
        config::{DeepBookConfig, Environment},
        types::BalanceManager,
    },
};
use sui_sdk::{
    types::{
        base_types::SuiAddress, programmable_transaction_builder::ProgrammableTransactionBuilder,
    },
    SuiClientBuilder,
};
use utils::dry_run_transaction;

mod utils;

#[tokio::test]
async fn test_create_and_share_balance_manager() {
    let sui_client = SuiClientBuilder::default().build_testnet().await.unwrap();

    let config = deep_book_config();
    println!("config: {:#?}", config);

    let balance_manager = BalanceManagerContract::new(sui_client.clone(), config);

    let mut ptb = ProgrammableTransactionBuilder::new();

    let _ = balance_manager.create_and_share_balance_manager(&mut ptb);
    // execute_transaction(ptb).await;
    let result = dry_run_transaction(&sui_client, ptb).await.unwrap();
    println!("result: {:#?}", result);
}

#[tokio::test]
async fn test_balance_manager_owner() {
    let sui_client = SuiClientBuilder::default().build_testnet().await.unwrap();

    let config = deep_book_config();

    let balance_manager = BalanceManagerContract::new(sui_client.clone(), config);

    let mut ptb = ProgrammableTransactionBuilder::new();

    let _ = balance_manager.owner(&mut ptb, "DEEP").await;

    let result = dry_run_transaction(&sui_client, ptb).await.unwrap();
    let result = result.first().unwrap();
    println!(
        "owner: {:#?}",
        bcs::from_bytes::<SuiAddress>(&result.0).unwrap()
    );
    assert_eq!(
        bcs::from_bytes::<SuiAddress>(&result.0).unwrap(),
        SuiAddress::from_str("0x7731f9c105f3c2bde96f0eca645e718465394d609139342f3196383b823890a9")
            .unwrap()
    );
}

#[tokio::test]
async fn test_balance_manager_id() {
    let sui_client = SuiClientBuilder::default().build_testnet().await.unwrap();

    let config = deep_book_config();

    let balance_manager = BalanceManagerContract::new(sui_client.clone(), config);

    let mut ptb = ProgrammableTransactionBuilder::new();

    let _ = balance_manager.id(&mut ptb, "DEEP").await;

    let result = dry_run_transaction(&sui_client, ptb).await.unwrap();
    let result = result.first().unwrap();
    println!(
        "id: {:#?}",
        bcs::from_bytes::<SuiAddress>(&result.0).unwrap()
    );
    assert_eq!(
        bcs::from_bytes::<SuiAddress>(&result.0).unwrap(),
        SuiAddress::from_str("0x722c39b7b79831d534fbfa522e07101cb881f8807c28b9cf03a58b04c6c5ca9a")
            .unwrap()
    );
}

fn deep_book_config() -> DeepBookConfig {
    let balance_managers = HashMap::from([(
        "DEEP",
        BalanceManager {
            address: "0x722c39b7b79831d534fbfa522e07101cb881f8807c28b9cf03a58b04c6c5ca9a"
                .to_string(),
            trade_cap: None,
        },
    )]);

    DeepBookConfig::new(
        Environment::Testnet,
        SuiAddress::random_for_testing_only(),
        None,
        Some(balance_managers),
        None,
        None,
    )
}
