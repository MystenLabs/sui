// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(dead_code)]
mod rosetta_client;
#[path = "custom_coins/test_coin_utils.rs"]
mod test_coin_utils;

use std::num::NonZeroUsize;
use std::path::Path;
use std::str::FromStr;

use serde_json::json;

use shared_crypto::intent::Intent;
use sui_json_rpc_types::{
    ObjectChange, SuiExecutionStatus, SuiTransactionBlockEffectsAPI,
    SuiTransactionBlockResponseOptions,
};
use sui_keys::keystore::AccountKeystore;
use sui_rosetta::operations::Operations;
use sui_rosetta::types::{
    AccountBalanceRequest, AccountBalanceResponse, AccountIdentifier, Amount, Currency,
    CurrencyMetadata, NetworkIdentifier, SuiEnv,
};
use sui_rosetta::types::{Currencies, OperationType};
use sui_rosetta::CoinMetadataCache;
use sui_rosetta::SUI;
use sui_types::coin::COIN_MODULE_NAME;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::transaction::{
    Argument, Command, ObjectArg, Transaction, TransactionData, TransactionDataAPI,
};
use sui_types::{Identifier, SUI_FRAMEWORK_PACKAGE_ID};
use test_cluster::TestClusterBuilder;
use test_coin_utils::{init_package, mint, TEST_COIN_DECIMALS};

use crate::rosetta_client::{start_rosetta_test_server, RosettaEndpoint};

#[tokio::test]
async fn test_mint() {
    const COIN1_BALANCE: u64 = 100_000_000;
    const COIN2_BALANCE: u64 = 200_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let sender = test_cluster.get_address_0();
    let init_ret = init_package(
        &client,
        keystore,
        sender,
        Path::new("tests/custom_coins/test_coin"),
    )
    .await
    .unwrap();

    let address1 = test_cluster.get_address_1();
    let address2 = test_cluster.get_address_2();
    let balances_to = vec![(COIN1_BALANCE, address1), (COIN2_BALANCE, address2)];

    let mint_res = mint(&client, keystore, init_ret, balances_to)
        .await
        .unwrap();
    let coins = mint_res
        .object_changes
        .unwrap()
        .into_iter()
        .filter_map(|change| {
            if let ObjectChange::Created {
                object_type, owner, ..
            } = change
            {
                Some((object_type, owner))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let coin1 = coins
        .iter()
        .find(|coin| coin.1 == Owner::AddressOwner(address1))
        .unwrap();
    let coin2 = coins
        .iter()
        .find(|coin| coin.1 == Owner::AddressOwner(address2))
        .unwrap();
    assert!(coin1.0.to_string().contains("::test_coin::TEST_COIN"));
    assert!(coin2.0.to_string().contains("::test_coin::TEST_COIN"));
}

#[tokio::test]
async fn test_custom_coin_balance() {
    // mint coins to `test_culset.get_address_1()` and `test_culset.get_address_2()`
    const SUI_BALANCE: u64 = 150_000_000_000_000_000;
    const COIN1_BALANCE: u64 = 100_000_000;
    const COIN2_BALANCE: u64 = 200_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let sender = test_cluster.get_address_0();
    let init_ret = init_package(
        &client,
        keystore,
        sender,
        Path::new("tests/custom_coins/test_coin"),
    )
    .await
    .unwrap();

    let address1 = test_cluster.get_address_1();
    let address2 = test_cluster.get_address_2();
    let balances_to = vec![(COIN1_BALANCE, address1), (COIN2_BALANCE, address2)];
    let coin_type = init_ret.coin_tag.to_canonical_string(true);

    let _mint_res = mint(&client, keystore, init_ret, balances_to)
        .await
        .unwrap();

    // setup AccountBalanceRequest
    let network_identifier = NetworkIdentifier {
        blockchain: "sui".to_string(),
        network: SuiEnv::LocalNet,
    };

    let sui_currency = SUI.clone();
    let test_coin_currency = Currency {
        symbol: "TEST_COIN".to_string(),
        decimals: TEST_COIN_DECIMALS,
        metadata: CurrencyMetadata {
            coin_type: coin_type.clone(),
        },
    };

    // Verify initial balance and stake
    let request = AccountBalanceRequest {
        network_identifier: network_identifier.clone(),
        account_identifier: AccountIdentifier {
            address: address1,
            sub_account: None,
        },
        block_identifier: Default::default(),
        currencies: Currencies(vec![sui_currency, test_coin_currency]),
    };

    println!(
        "request: {}",
        serde_json::to_string_pretty(&request).unwrap()
    );
    let response: AccountBalanceResponse = rosetta_client
        .call(RosettaEndpoint::Balance, &request)
        .await
        .unwrap();
    println!(
        "response: {}",
        serde_json::to_string_pretty(&response).unwrap()
    );
    assert_eq!(response.balances.len(), 2);
    assert_eq!(response.balances[0].value, SUI_BALANCE as i128);
    assert_eq!(
        response.balances[0].currency.clone().metadata.coin_type,
        "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI"
    );
    assert_eq!(response.balances[1].value, COIN1_BALANCE as i128);
    assert_eq!(
        response.balances[1].currency.clone().metadata.coin_type,
        coin_type
    );
}

#[tokio::test]
async fn test_default_balance() {
    // mint coins to `test_culset.get_address_1()` and `test_culset.get_address_2()`
    const SUI_BALANCE: u64 = 150_000_000_000_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let client = test_cluster.wallet.get_client().await.unwrap();

    let (rosetta_client, _handles) = start_rosetta_test_server(client.clone()).await;

    let request: AccountBalanceRequest = serde_json::from_value(json!(
        {
            "network_identifier": {
                "blockchain": "sui",
                "network": "localnet"
            },
            "account_identifier": {
                "address": test_cluster.get_address_0()
            }
        }
    ))
    .unwrap();
    let response: AccountBalanceResponse = rosetta_client
        .call(RosettaEndpoint::Balance, &request)
        .await
        .unwrap();
    println!(
        "response: {}",
        serde_json::to_string_pretty(&response).unwrap()
    );
    assert_eq!(response.balances.len(), 1);
    assert_eq!(response.balances[0].value, SUI_BALANCE as i128);
}

#[tokio::test]
async fn test_custom_coin_transfer() {
    const COIN1_BALANCE: u64 = 100_000_000_000_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    // TEST_COIN setup and mint
    let init_ret = init_package(
        &client,
        keystore,
        sender,
        Path::new("tests/custom_coins/test_coin"),
    )
    .await
    .unwrap();
    let balances_to = vec![(COIN1_BALANCE, sender)];
    let coin_type = init_ret.coin_tag.to_canonical_string(true);
    let _mint_res = mint(&client, keystore, init_ret, balances_to)
        .await
        .unwrap();

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"PayCoin",
            "account": { "address" : recipient.to_string() },
            "amount" : {
                "value": "30000000",
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {
                        "coin_type": coin_type.clone(),
                    }
                }
            },
        },
        {
            "operation_identifier":{"index":1},
            "type":"PayCoin",
            "account": { "address" : sender.to_string() },
            "amount" : {
                "value": "-30000000",
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {
                        "coin_type": coin_type.clone(),
                    }
                }
            },
        }]
    ))
    .unwrap();

    let response = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();

    let tx = client
        .read_api()
        .get_transaction_with_options(
            response.transaction_identifier.hash,
            SuiTransactionBlockResponseOptions::new()
                .with_input()
                .with_effects()
                .with_balance_changes()
                .with_events(),
        )
        .await
        .unwrap();

    assert_eq!(
        &SuiExecutionStatus::Success,
        tx.effects.as_ref().unwrap().status()
    );
    println!("Sui TX: {tx:?}");
    let coin_cache = CoinMetadataCache::new(client, NonZeroUsize::new(2).unwrap());
    let ops2 = Operations::try_from_response(tx, &coin_cache)
        .await
        .unwrap();
    assert!(
        ops2.contains(&ops),
        "Operation mismatch. expecting:{}, got:{}",
        serde_json::to_string(&ops).unwrap(),
        serde_json::to_string(&ops2).unwrap()
    );
}

#[tokio::test]
async fn test_custom_coin_without_symbol() {
    const COIN1_BALANCE: u64 = 100_000_000_000_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    // TEST_COIN setup and mint
    let init_ret = init_package(
        &client,
        keystore,
        sender,
        Path::new("tests/custom_coins/test_coin_no_symbol"),
    )
    .await
    .unwrap();

    let balances_to = vec![(COIN1_BALANCE, sender)];
    let mint_res = mint(&client, keystore, init_ret, balances_to)
        .await
        .unwrap();

    let tx = client
        .read_api()
        .get_transaction_with_options(
            mint_res.digest,
            SuiTransactionBlockResponseOptions::new()
                .with_input()
                .with_effects()
                .with_balance_changes()
                .with_events(),
        )
        .await
        .unwrap();

    assert_eq!(
        &SuiExecutionStatus::Success,
        tx.effects.as_ref().unwrap().status()
    );
    let coin_cache = CoinMetadataCache::new(client, NonZeroUsize::new(2).unwrap());
    let ops = Operations::try_from_response(tx, &coin_cache)
        .await
        .unwrap();

    for op in ops {
        if op.type_ == OperationType::SuiBalanceChange {
            assert!(!op.amount.unwrap().currency.symbol.is_empty())
        }
    }
}

#[tokio::test]
async fn test_mint_with_gas_coin_transfer() -> anyhow::Result<()> {
    const COIN1_BALANCE: u64 = 100_000_000;
    const COIN2_BALANCE: u64 = 200_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let sender = test_cluster.get_address_0();
    let init_ret = init_package(
        &client,
        keystore,
        sender,
        Path::new("tests/custom_coins/test_coin"),
    )
    .await
    .unwrap();

    let address1 = test_cluster.get_address_1();
    let address2 = test_cluster.get_address_2();
    let balances_to = vec![(COIN1_BALANCE, address1), (COIN2_BALANCE, address2)];

    let treasury_cap_owner = init_ret.owner;

    let gas_price = client
        .governance_api()
        .get_reference_gas_price()
        .await
        .unwrap();
    const LARGE_GAS_BUDGET: u128 = 1_000_000_000;
    let mut gas_coins = client
        .coin_read_api()
        .select_coins(sender, None, LARGE_GAS_BUDGET, vec![])
        .await?;
    assert!(
        gas_coins.len() == 1,
        "Expected 1 large gas-coin to satisfy the budget"
    );
    let gas_coin = gas_coins.pop().unwrap();
    let mut ptb = ProgrammableTransactionBuilder::new();

    let treasury_cap = ptb.obj(ObjectArg::ImmOrOwnedObject(init_ret.treasury_cap))?;
    for (balance, to) in balances_to {
        let balance = ptb.pure(balance)?;
        let coin = ptb.command(Command::move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            Identifier::from(COIN_MODULE_NAME),
            Identifier::from_str("mint")?,
            vec![init_ret.coin_tag.clone()],
            vec![treasury_cap, balance],
        ));
        ptb.transfer_arg(to, coin);
    }
    ptb.transfer_arg(address1, Argument::GasCoin);
    let builder = ptb.finish();

    // Sign transaction
    let tx_data = TransactionData::new_programmable(
        treasury_cap_owner,
        vec![gas_coin.object_ref()],
        builder,
        LARGE_GAS_BUDGET as u64,
        gas_price,
    );

    let sig = keystore
        .sign_secure(&tx_data.sender(), &tx_data, Intent::sui_transaction())
        .await?;

    let mint_res = client
        .quorum_driver_api()
        .execute_transaction_block(
            Transaction::from_data(tx_data, vec![sig]),
            SuiTransactionBlockResponseOptions::new()
                .with_balance_changes()
                .with_effects()
                .with_input()
                .with_object_changes(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;
    let gas_used = mint_res.effects.as_ref().unwrap().gas_cost_summary();
    let mut gas_used = gas_used.net_gas_usage() as i128;
    println!("gas_used: {gas_used}");

    let coins = mint_res
        .object_changes
        .as_ref()
        .unwrap()
        .iter()
        .filter_map(|change| {
            if let ObjectChange::Created {
                object_type, owner, ..
            } = change
            {
                Some((object_type, owner))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let coin1 = coins
        .iter()
        .find(|coin| coin.1.get_address_owner_address().unwrap() == address1)
        .unwrap();
    let coin2 = coins
        .iter()
        .find(|coin| coin.1.get_address_owner_address().unwrap() == address2)
        .unwrap();
    assert!(coin1.0.to_string().contains("::test_coin::TEST_COIN"));
    assert!(coin2.0.to_string().contains("::test_coin::TEST_COIN"));

    let coin_cache = CoinMetadataCache::new(client, NonZeroUsize::new(2).unwrap());
    let ops = Operations::try_from_response(mint_res, &coin_cache)
        .await
        .unwrap();
    const COIN_BALANCE_CREATED: u64 = COIN1_BALANCE + COIN2_BALANCE;
    println!("ops: {}", serde_json::to_string_pretty(&ops).unwrap());
    let mut coin_created = 0;
    ops.into_iter().for_each(|op| {
        if let Some(Amount {
            value, currency, ..
        }) = op.amount
        {
            if currency == Currency::default() {
                gas_used += value
            } else {
                coin_created += value
            }
        }
    });
    assert!(COIN_BALANCE_CREATED as i128 == coin_created);
    assert!(gas_used == 0);

    Ok(())
}
