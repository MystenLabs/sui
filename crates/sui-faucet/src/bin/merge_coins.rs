// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use shared_crypto::intent::Intent;
use std::{str::FromStr, time::Duration};
use sui_config::{sui_config_dir, SUI_CLIENT_CONFIG};
use sui_faucet::FaucetError;
use sui_json_rpc_types::SuiTransactionBlockResponseOptions;
use sui_keys::keystore::AccountKeystore;
use sui_sdk::wallet_context::WalletContext;
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::{base_types::ObjectID, gas_coin::GasCoin, transaction::Transaction};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let mut wallet = create_wallet_context(60)?;
    let active_address = wallet
        .active_address()
        .map_err(|err| FaucetError::Wallet(err.to_string()))?;
    println!("SimpleFaucet::new with active address: {active_address}");

    // Example scripts
    // merge_coins(
    //     "0x0215b800acc47d80a50741f0eecfa507fc2c21f5a9aa6140a219686ad20d7f4c",
    //     wallet,
    // )
    // .await?;

    // split_coins_equally(
    //     "0xd42a75242975780037e170486540f28ab3c9be07dbb1f6f2a9430ad268e3b1d1",
    //     wallet,
    //     1000,
    // )
    // .await?;

    Ok(())
}

async fn _split_coins_equally(
    gas_coin: &str,
    mut wallet: WalletContext,
    count: u64,
) -> Result<(), anyhow::Error> {
    let active_address = wallet
        .active_address()
        .map_err(|err| FaucetError::Wallet(err.to_string()))?;
    let client = wallet.get_client().await?;
    let coin_object_id = ObjectID::from_str(gas_coin).unwrap();
    let tx_data = client
        .transaction_builder()
        .split_coin_equal(active_address, coin_object_id, count, None, 50000000000)
        .await?;

    let signature = wallet
        .config
        .keystore
        .sign_secure(&active_address, &tx_data, Intent::sui_transaction())
        .unwrap();
    let tx = Transaction::from_data(tx_data, vec![signature]);
    let resp = client
        .quorum_driver_api()
        .execute_transaction_block(
            tx.clone(),
            SuiTransactionBlockResponseOptions::new().with_effects(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;

    println!("{:?}", resp);
    Ok(())
}

async fn _merge_coins(gas_coin: &str, mut wallet: WalletContext) -> Result<(), anyhow::Error> {
    let active_address = wallet
        .active_address()
        .map_err(|err| FaucetError::Wallet(err.to_string()))?;
    let client = wallet.get_client().await?;
    // Pick a gas coin here that isn't in use by the faucet otherwise there will be some contention.
    let small_coins = wallet
        .gas_objects(active_address)
        .await
        .map_err(|e| FaucetError::Wallet(e.to_string()))?
        .iter()
        // Ok to unwrap() since `get_gas_objects` guarantees gas
        .map(|q| GasCoin::try_from(&q.1).unwrap())
        // Everything less than 1 sui
        .filter(|coin| coin.0.balance.value() <= 10000000000)
        .collect::<Vec<GasCoin>>();

    // Smash coins togethers 254 objects at a time
    for chunk in small_coins.chunks(254) {
        let total_balance: u64 = chunk.iter().map(|coin| coin.0.balance.value()).sum();

        let mut coin_vector = chunk
            .iter()
            .map(|coin| *coin.id())
            .collect::<Vec<ObjectID>>();

        // prepend big gas coin instance to vector
        coin_vector.insert(0, ObjectID::from_str(gas_coin).unwrap());
        let target = vec![active_address];
        let target_amount = vec![total_balance];

        let tx_data = client
            .transaction_builder()
            .pay_sui(active_address, coin_vector, target, target_amount, 1000000)
            .await?;
        let signature = wallet
            .config
            .keystore
            .sign_secure(&active_address, &tx_data, Intent::sui_transaction())
            .unwrap();
        let tx = Transaction::from_data(tx_data, vec![signature]);
        client
            .quorum_driver_api()
            .execute_transaction_block(
                tx.clone(),
                SuiTransactionBlockResponseOptions::new().with_effects(),
                Some(ExecuteTransactionRequestType::WaitForLocalExecution),
            )
            .await?;
    }
    Ok(())
}

pub fn create_wallet_context(timeout_secs: u64) -> Result<WalletContext, anyhow::Error> {
    let wallet_conf = sui_config_dir()?.join(SUI_CLIENT_CONFIG);
    info!("Initialize wallet from config path: {:?}", wallet_conf);
    WalletContext::new(&wallet_conf, Some(Duration::from_secs(timeout_secs)), None)
}
