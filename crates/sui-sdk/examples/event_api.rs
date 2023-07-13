// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;
// use futures::stream::StreamExt;
use sui_sdk::rpc_types::EventFilter;
use sui_sdk::types::digests::TransactionDigest;
use sui_sdk::SuiClientBuilder;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClientBuilder::default().build_testnet().await?; // testnet Sui network
    println!("Sui testnet version{:?}", sui.api_version());

    // TODO - make this work
    // Subscribe event
    // let mut subscribe_all = sui
    //     .event_api()
    //     .subscribe_event(EventFilter::All(vec![]))
    //     .await?;
    // println!(" *** Subscribe event *** ");
    // loop {
    //     println!("{:?}", subscribe_all.next().await);
    // }
    // println!(" *** Subscribe event ***\n ");

    println!(" *** Get events *** ");
    // for demonstration purposes, we set to make a transaction
    let digest = TransactionDigest::from_str("FQyf6npjF5m9kg7o52zjLxnFMNQdFX2adMAnY4T7QTzp")?;
    let events = sui.event_api().get_events(digest).await?;
    println!("{:?}", events);
    println!(" *** Get events ***\n ");

    let query_events = sui
        .event_api()
        .query_events(EventFilter::All(vec![]), None, Some(5), true) // query first 5 events in descending order
        .await?;
    println!(" *** Query events *** ");
    println!("{:?}", query_events);
    println!(" *** Query events ***\n ");
    Ok(())
}
