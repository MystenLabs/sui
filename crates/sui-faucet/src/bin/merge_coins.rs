// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use rand::seq::SliceRandom;
use shared_crypto::intent::Intent;
use std::{str::FromStr, time::Duration};
use sui_config::{sui_config_dir, SUI_CLIENT_CONFIG};
use sui_faucet::FaucetError;
use sui_json_rpc_types::{
    SuiObjectDataOptions, SuiObjectResponse, SuiObjectResponseQuery,
    SuiTransactionBlockResponseOptions,
};
use sui_keys::keystore::AccountKeystore;
use sui_sdk::types::base_types::SuiAddress;
use sui_sdk::wallet_context::WalletContext;
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::{
    base_types::ObjectID, gas_coin::GasCoin, object::Object, transaction::Transaction,
};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let mut wallet = create_wallet_context(60)?;
    let active_address = wallet
        .active_address()
        .map_err(|err| FaucetError::Wallet(err.to_string()))?;
    println!("SimpleFaucet::new with active address: {active_address}");

    // let coins_before = get_owned_objects(wallet).await?;

    // println!("{:?}", coins);
    let transfer_balance = 5000000000000000;
    //1878225937500000

    // get the total balance of the coins owned by the faucet
    // let total_balance_before: u64 = coins_before.iter().map(|c| c.0).sum();
    // println!("Total balance before: {}", total_balance_before);

    let recipient =
        SuiAddress::from_str("0x6511dd9aa26e2f49cf0c7011478d74c745e1e575cd3341f58ebd4e64bfbf1dd4")?;

    // let wallet = create_wallet_context(60)?;
    // let mut rng = rand::thread_rng(); // Initialize the random number generator
    // let mut coins_vec = coins_before.iter().map(|c| c.1).collect::<Vec<_>>();

    // coins_vec.shuffle(&mut rng); // Shuffle the elements randomly

    // let random_10_elements: Vec<_> = coins_vec.iter().take(10).cloned().collect();
    let mut success_count = 0;
    for _ in 0..3 {
        let wallet = create_wallet_context(60)?;
        let result = transfer_sui(wallet, recipient, transfer_balance).await;
        if result.is_ok() {
            success_count += 1;
        }
    }

    println!("Success count: {} attempt count 3", success_count);
    // let wallet = create_wallet_context(60)?;
    // let coins_after = get_owned_objects(wallet).await?;

    // let total_balance_after: u64 = coins_after.iter().map(|c| c.0).sum();
    // println!("Total balance after: {}", total_balance_after);

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

async fn _get_owned_objects(
    mut wallet: WalletContext,
) -> Result<Vec<(u64, ObjectID)>, anyhow::Error> {
    let active_address = wallet
        .active_address()
        .map_err(|err| FaucetError::Wallet(err.to_string()))?;
    let client = wallet.get_client().await?;
    let mut objects: Vec<SuiObjectResponse> = Vec::new();
    let mut cursor = None;
    loop {
        let response = client
            .read_api()
            .get_owned_objects(
                active_address,
                Some(SuiObjectResponseQuery::new_with_options(
                    SuiObjectDataOptions::bcs_lossless(),
                )),
                cursor,
                None,
            )
            .await?;

        objects.extend(response.data);

        if response.has_next_page {
            cursor = response.next_cursor;
        } else {
            break;
        }
    }

    let mut values_objects = Vec::new();

    for object in objects {
        let o = object.data;
        if let Some(o) = o {
            if o.is_gas_coin() {
                let temp: Object = o.clone().try_into()?;
                let gas_coin = GasCoin::try_from(&temp)?;
                values_objects.push((gas_coin.value(), gas_coin.id().clone()));
            }
            // else {
            //     println!("Skipping non-gas coin object: {:?}", o.type_);
            // }
        }
    }

    Ok(values_objects)
}

async fn transfer_sui(
    mut wallet: WalletContext,
    recipient: SuiAddress,
    amount: u64,
) -> Result<(), anyhow::Error> {
    let active_address = wallet
        .active_address()
        .map_err(|err| FaucetError::Wallet(err.to_string()))?;
    let client = wallet.get_client().await?;

    // Pick a gas coin here that isn't in use by the faucet otherwise there will be some contention.

    let mut rng = rand::thread_rng(); // Initialize the random number generator
    let mut gas_coins = wallet
        .gas_objects(active_address)
        .await
        .map_err(|e| FaucetError::Wallet(e.to_string()))?;
    gas_coins.shuffle(&mut rng); // Shuffle the elements randomly
    let filtered_coins = gas_coins
        .iter()
        // Ok to unwrap() since `get_gas_objects` guarantees gas
        .map(|q| GasCoin::try_from(&q.1).unwrap())
        // Everything greater than 1M sui
        .filter(|coin| coin.0.balance.value() > 1000000000000000)
        .take(5)
        .collect::<Vec<GasCoin>>();

    let tx_data = client
        .transaction_builder()
        .pay_sui(
            active_address,
            filtered_coins
                .iter()
                .map(|coin| *coin.id())
                .collect::<Vec<ObjectID>>(),
            vec![recipient],
            vec![amount],
            50000000000,
        )
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
        .await;

    if resp.is_ok() {
        println!("Transfered {} to {}", amount, recipient);
    }
    resp?;
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
