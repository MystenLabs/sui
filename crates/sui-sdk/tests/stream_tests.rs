// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::StreamExt;
use std::future;
use sui::client_commands::SuiClientCommands;
use sui_sdk::{SuiClientBuilder, SUI_COIN_TYPE};
use sui_types::event::EventType;
use sui_types::query::{EventQuery, TransactionQuery};
use test_utils::network::TestClusterBuilder;

#[tokio::test]
async fn test_transactions_stream() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await?;
    let rpc_url = test_cluster.rpc_url();

    let client = SuiClientBuilder::default().build(rpc_url).await?;
    let txs = client
        .read_api()
        .get_transactions_stream(TransactionQuery::All, None, true)
        .collect::<Vec<_>>()
        .await;

    assert_eq!(1, txs.len());

    // execute some transactions
    SuiClientCommands::CreateExampleNFT {
        name: None,
        description: None,
        url: None,
        gas: None,
        gas_budget: None,
    }
    .execute(&mut test_cluster.wallet)
    .await?;

    let txs = client
        .read_api()
        .get_transactions_stream(TransactionQuery::All, None, true)
        .collect::<Vec<_>>()
        .await;

    assert_eq!(2, txs.len());
    Ok(())
}

#[tokio::test]
async fn test_events_stream() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new()
        .enable_fullnode_events()
        .build()
        .await?;
    let rpc_url = test_cluster.rpc_url();

    let client = SuiClientBuilder::default().build(rpc_url).await?;
    let events = client
        .event_api()
        .get_events_stream(EventQuery::All, None, true)
        .collect::<Vec<_>>()
        .await;

    let starting_event_count = events.len();

    let events = client
        .event_api()
        .get_events_stream(
            EventQuery::EventType(EventType::CoinBalanceChange),
            None,
            true,
        )
        .collect::<Vec<_>>()
        .await;

    let starting_coin_balance_change_event_count = events.len();

    let events = client
        .event_api()
        .get_events_stream(EventQuery::EventType(EventType::NewObject), None, true)
        .collect::<Vec<_>>()
        .await;

    let starting_new_object_event_count = events.len();

    // execute some transactions
    SuiClientCommands::CreateExampleNFT {
        name: None,
        description: None,
        url: None,
        gas: None,
        gas_budget: None,
    }
    .execute(&mut test_cluster.wallet)
    .await?;

    let events = client
        .event_api()
        .get_events_stream(EventQuery::All, None, true)
        .collect::<Vec<_>>()
        .await;
    assert_eq!(starting_event_count + 3, events.len());

    let events = client
        .event_api()
        .get_events_stream(
            EventQuery::EventType(EventType::CoinBalanceChange),
            None,
            true,
        )
        .collect::<Vec<_>>()
        .await;
    assert_eq!(starting_coin_balance_change_event_count + 1, events.len());

    let events = client
        .event_api()
        .get_events_stream(EventQuery::EventType(EventType::NewObject), None, true)
        .collect::<Vec<_>>()
        .await;
    assert_eq!(starting_new_object_event_count + 1, events.len());
    Ok(())
}

#[tokio::test]
async fn test_coins_stream() -> Result<(), anyhow::Error> {
    let test_cluster = TestClusterBuilder::new().build().await?;
    let address = test_cluster.get_address_0();
    let rpc_url = test_cluster.rpc_url();

    let client = SuiClientBuilder::default().build(rpc_url).await?;
    let coins = client
        .coin_read_api()
        .get_coins_stream(address, Some(SUI_COIN_TYPE.to_string()))
        .collect::<Vec<_>>()
        .await;

    assert_eq!(5, coins.len());

    let page = client
        .coin_read_api()
        .get_coins(address, Some(SUI_COIN_TYPE.to_string()), None, None)
        .await?;

    for (coin1, coin2) in coins.into_iter().zip(page.data) {
        assert_eq!(coin1.coin_object_id, coin2.coin_object_id);
    }

    let amount = client
        .coin_read_api()
        .get_coins_stream(address, Some(SUI_COIN_TYPE.to_string()))
        .fold(0u128, |acc, coin| async move { acc + coin.balance as u128 })
        .await;

    assert_eq!(500000000000000, amount);

    let mut total = 0u128;

    let coins = client
        .coin_read_api()
        .get_coins_stream(address, Some(SUI_COIN_TYPE.to_string()))
        .take_while(|coin| {
            let ready = future::ready(total < 300000000000000);
            total += coin.balance as u128;
            ready
        })
        .map(|coin| coin.object_ref())
        .collect::<Vec<_>>()
        .await;

    assert_eq!(3, coins.len());

    Ok(())
}
