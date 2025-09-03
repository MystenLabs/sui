// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};

use shared_crypto::intent::Intent;
use sui_json_rpc_types::{
    Coin, ObjectChange, SuiExecutionStatus, SuiMoveValue, SuiObjectDataOptions, SuiObjectRef,
    SuiParsedData, SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponse,
    SuiTransactionBlockResponseOptions,
};
use sui_keys::keystore::{AccountKeystore, Keystore};
use sui_sdk::{SuiClient, SUI_COIN_TYPE};
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::transaction::{
    Argument, Command, ObjectArg, Transaction, TransactionData, TransactionDataAPI,
};
use test_cluster::TestClusterBuilder;

pub const DEFAULT_GAS_BUDGET: u64 = 900_000_000;
const DEFAULT_INIT_COIN_BALANCE: u64 = 30_000_000_000_000_000;
const MAX_NEW_COINS: usize = 511; // maximum arguments in a programmable transaction command is 511

pub async fn split_coins(
    client: &SuiClient,
    keystore: &Keystore,
    sender: SuiAddress,
    coin: ObjectRef,
    amounts: &[u64],
    gas: Option<ObjectRef>,
    reference_gas_price: Option<u64>,
    budget: Option<u64>,
) -> Result<SuiTransactionBlockResponse> {
    if amounts.len() > MAX_NEW_COINS {
        return Err(anyhow!("Max new coins: {}", MAX_NEW_COINS));
    }
    let budget = budget.unwrap_or(DEFAULT_GAS_BUDGET);
    let reference_gas_price = match reference_gas_price {
        Some(price) => price,
        None => client.read_api().get_reference_gas_price().await?,
    };
    let mut ptb = ProgrammableTransactionBuilder::new();

    let amounts_len = amounts.len();
    let amounts = amounts
        .iter()
        .map(|amount| ptb.pure(amount))
        .collect::<Result<Vec<_>>>()?;
    let (split_coin, gas) = match gas {
        Some(gas) => (ptb.obj(ObjectArg::ImmOrOwnedObject(coin))?, gas),
        None => (Argument::GasCoin, coin),
    };
    ptb.command(Command::SplitCoins(split_coin, amounts));
    let sender_arg = ptb.pure(sender)?;
    let results = (0..amounts_len)
        .map(|i| Argument::NestedResult(0, i as u16))
        .collect::<Vec<_>>();
    ptb.command(Command::TransferObjects(results, sender_arg));
    let builder = ptb.finish();

    // Sign transaction
    let tx_data =
        TransactionData::new_programmable(sender, vec![gas], builder, budget, reference_gas_price);
    let sig = keystore
        .sign_secure(&tx_data.sender(), &tx_data, Intent::sui_transaction())
        .await?;

    let res = client
        .quorum_driver_api()
        .execute_transaction_block(
            Transaction::from_data(tx_data, vec![sig]),
            SuiTransactionBlockResponseOptions::new()
                .with_effects()
                .with_object_changes(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;

    Ok(res)
}

pub async fn make_change(
    client: &SuiClient,
    keystore: &Keystore,
    sender: SuiAddress,
    coin: Coin,
    gas: Option<ObjectRef>,
    amount_per_change: u64,
) -> Result<Vec<SuiTransactionBlockResponse>> {
    let remainder = coin.balance % amount_per_change;
    let n_new_coins = (coin.balance / amount_per_change) as usize - (remainder == 0) as usize;
    let vecs_remainder = n_new_coins % MAX_NEW_COINS;
    let n_vecs = n_new_coins / MAX_NEW_COINS;
    assert!(n_new_coins as u64 * amount_per_change < coin.balance);

    let mut amounts_vec = vec![vec![amount_per_change; MAX_NEW_COINS]; n_vecs];
    amounts_vec.push(vec![amount_per_change; vecs_remainder]);

    let mut responses = Vec::with_capacity(amounts_vec.len());
    let mut coin_ref = coin.object_ref();
    let mut gas_ref = gas;
    let ref_gas_price = client.read_api().get_reference_gas_price().await?;
    let mut progress = 0;
    let len = amounts_vec.len();
    for amounts in amounts_vec.into_iter() {
        let resp = split_coins(
            client,
            keystore,
            sender,
            coin_ref,
            &amounts,
            gas_ref,
            Some(ref_gas_price),
            None,
        )
        .await?;
        progress += 1;
        if progress % 4 == 0 {
            println!(
                "Splitting progress: {}%",
                progress as f32 * 100. / len as f32
            );
        }
        if !resp.status_ok().ok_or(anyhow!("Expected effects"))? {
            println!("resp: {resp:#?}");
            return Err(anyhow!("split_coins errored"));
        }
        coin_ref = resp
            .object_changes
            .as_ref()
            .ok_or(anyhow!("Expected object_changes"))?
            .iter()
            .find(|&chng| chng.object_id() == coin.coin_object_id)
            .ok_or(anyhow!("Expected object_changes to contain coin_object_id"))?
            .object_ref();
        gas_ref = match gas_ref {
            Some(_) => {
                let SuiObjectRef {
                    object_id,
                    version,
                    digest,
                } = resp
                    .effects
                    .as_ref()
                    .ok_or(anyhow!("Expected balance_changes"))?
                    .gas_object()
                    .reference;
                Some((object_id, version, digest))
            }
            None => None,
        };

        // Make sure the tx has executed locally
        client
            .read_api()
            .get_transaction_with_options(
                resp.digest,
                SuiTransactionBlockResponseOptions::default(),
            )
            .await?;
        responses.push(resp);
    }
    Ok(responses)
}

#[tokio::test]
async fn test_make_change_exact_div() -> Result<()> {
    const SUI_100: u64 = 100_000_000_000;
    const SUI_10: u64 = 10_000_000_000;

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let coin = client
        .coin_read_api()
        .get_all_coins(sender, None, Some(1))
        .await?
        .data
        .into_iter()
        .next()
        .ok_or(anyhow!("Expected 1 coin"))?;

    assert!(
        coin.balance == DEFAULT_INIT_COIN_BALANCE,
        "Coin did not match INIT_COIN_BALANCE"
    );

    let split_resp = split_coins(
        &client,
        keystore,
        sender,
        coin.object_ref(),
        &[SUI_100],
        None,
        None,
        None,
    )
    .await?;

    let tx_digest = split_resp.digest;
    if split_resp
        .effects
        .as_ref()
        .ok_or(anyhow!("Expected effects"))?
        .status()
        .clone()
        != SuiExecutionStatus::Success
    {
        return Err(anyhow!("Transaction failed!: {:#?}", split_resp));
    }

    // let digest = (split_resp.digest).clone();
    let (initial, splitted) = match split_resp
        .object_changes
        .ok_or(anyhow!("Expected object_changes"))?
        .as_slice()
    {
        [coin0, coin1] if coin0.object_id() == coin.coin_object_id => {
            (coin0.object_ref(), coin1.object_ref())
        }
        [coin0, coin1] => (coin1.object_ref(), coin0.object_ref()),
        obj_chngs => {
            println!("object_changes: {obj_chngs:#?}");
            return Err(anyhow!("Expected two items in object changes"));
        }
    };

    let splitted = Coin {
        coin_type: SUI_COIN_TYPE.to_string(),
        coin_object_id: splitted.0,
        version: splitted.1,
        digest: splitted.2,
        balance: SUI_100,
        previous_transaction: tx_digest,
    };

    let txs = make_change(&client, keystore, sender, splitted, Some(initial), SUI_10).await?;

    assert!(txs.len() == 1, "Should only have 1 tx");
    let new_coins = txs
        .into_iter()
        .next()
        .unwrap()
        .object_changes
        .ok_or(anyhow!("Expected object_changes"))?
        .into_iter()
        .filter(|chng| {
            if let ObjectChange::Created { .. } = chng {
                return true;
            }
            false
        })
        .collect::<Vec<_>>();
    assert!(
        new_coins.len() == (SUI_100 / SUI_10) as usize - 1,
        "Expected {} new coins. New coins: {new_coins:#?}",
        SUI_100 / SUI_10 - 1
    );

    Ok(())
}

#[tokio::test]
async fn test_make_change_remainder_div() -> Result<()> {
    const SUI_100: u64 = 100_000_000_000;
    const SUI_12: u64 = 12_000_000_000;

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let coin = client
        .coin_read_api()
        .get_all_coins(sender, None, Some(1))
        .await?
        .data
        .into_iter()
        .next()
        .ok_or(anyhow!("Expected 1 coin"))?;

    assert!(
        coin.balance == DEFAULT_INIT_COIN_BALANCE,
        "Coin did not match INIT_COIN_BALANCE"
    );

    let split_resp = split_coins(
        &client,
        keystore,
        sender,
        coin.object_ref(),
        &[SUI_100],
        None,
        None,
        None,
    )
    .await?;

    let tx_digest = split_resp.digest;
    if split_resp
        .effects
        .as_ref()
        .ok_or(anyhow!("Expected effects"))?
        .status()
        .clone()
        != SuiExecutionStatus::Success
    {
        return Err(anyhow!("Transaction failed!: {:#?}", split_resp));
    }

    let (initial, splitted) = match split_resp
        .object_changes
        .ok_or(anyhow!("Expected object_changes"))?
        .as_slice()
    {
        [coin0, coin1] if coin0.object_id() == coin.coin_object_id => {
            (coin0.object_ref(), coin1.object_ref())
        }
        [coin0, coin1] => (coin1.object_ref(), coin0.object_ref()),
        obj_chngs => {
            println!("object_changes: {obj_chngs:#?}");
            return Err(anyhow!("Expected two items in object changes"));
        }
    };

    let splitted = Coin {
        coin_type: SUI_COIN_TYPE.to_string(),
        coin_object_id: splitted.0,
        version: splitted.1,
        digest: splitted.2,
        balance: SUI_100,
        previous_transaction: tx_digest,
    };

    let splitted_id = splitted.coin_object_id;
    let txs = make_change(&client, keystore, sender, splitted, Some(initial), SUI_12).await?;

    assert!(txs.len() == 1, "Should only have 1 tx");
    let new_coins = txs
        .into_iter()
        .next()
        .unwrap()
        .object_changes
        .ok_or(anyhow!("Expected object_changes"))?
        .into_iter()
        .filter(|chng| {
            if let ObjectChange::Created { .. } = chng {
                return true;
            }
            false
        })
        .collect::<Vec<_>>();
    assert!(
        new_coins.len() == (SUI_100 / SUI_12) as usize,
        "Expected {} new coins. New coins: {new_coins:#?}",
        SUI_100 / SUI_12
    );

    let mut all_coins: Vec<ObjectID> = new_coins.into_iter().map(|c| c.object_id()).collect();
    all_coins.push(splitted_id);
    let coins_with_data = client
        .read_api()
        .multi_get_object_with_options(all_coins, SuiObjectDataOptions::full_content())
        .await?;

    let (mut twelve_count, mut four_count) = (0, 0);
    for coin in coins_with_data {
        let SuiParsedData::MoveObject(object) = coin
            .data
            .ok_or(anyhow!("No data in coin"))?
            .content
            .ok_or(anyhow!("No coin.data.content"))?
        else {
            return Err(anyhow!("Coin should be a MoveObject"));
        };
        let SuiMoveValue::String(balance) = object
            .fields
            .field_value("balance")
            .ok_or(anyhow!("No field coin.balance"))?
        else {
            return Err(anyhow!("Expected coin.balance to be a string"));
        };
        let b = balance.parse::<u64>()?;
        match b {
            SUI_12 => {
                twelve_count += 1;
            }
            4_000_000_000 => {
                four_count += 1;
            }
            b => {
                return Err(anyhow!(
                    "Did not expect anything else other than 12 or 4 SUI. Found {b}"
                ));
            }
        }
    }

    assert!(twelve_count == 8, "Expected 8 coins with 12 SUI");
    assert!(four_count == 1, "Expected 1 coin with 4 SUI");

    Ok(())
}
