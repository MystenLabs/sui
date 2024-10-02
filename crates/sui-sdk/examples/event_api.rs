// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod utils;
use futures::stream::StreamExt;
use sui_sdk::rpc_types::EventFilter;
use sui_sdk::SuiClientBuilder;
use utils::{setup_for_write, split_coin_digest};

// This example showcases how to use the Event API.
// At the end of the program it subscribes to the events
// on the Sui testnet and prints every incoming event to
// the console. The program will loop until it is force
// stopped.

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let (sui, active_address, _second_address) = setup_for_write().await?;

    println!(" *** Get events *** ");
    // for demonstration purposes, we set to make a transaction
    let digest = split_coin_digest(&sui, &active_address).await?;
    let events = sui.event_api().get_events(digest).await?;
    println!("{:?}", events);
    println!(" *** Get events ***\n ");

    let descending = true;
    let query_events = sui
        .event_api()
        .query_events(EventFilter::All([]), None, Some(5), descending) // query first 5 events in descending order
        .await?;
    println!(" *** Query events *** ");
    println!("{:?}", query_events);
    println!(" *** Query events ***\n ");

    let ws = SuiClientBuilder::default()
        .ws_url("wss://rpc.testnet.sui.io:443")
        .build("https://fullnode.testnet.sui.io:443")
        .await?;
    println!("WS version {:?}", ws.api_version());

    let mut subscribe = ws.event_api().subscribe_event(EventFilter::All([])).await?;

    loop {
        println!("{:?}", subscribe.next().await);
    }
}
