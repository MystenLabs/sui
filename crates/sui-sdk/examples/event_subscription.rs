// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::StreamExt;
use sui_sdk::rpc_types::SuiEventFilter;
use sui_sdk::SuiClientBuilder;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClientBuilder::default()
        .ws_url("ws://127.0.0.1:9001")
        .build("http://127.0.0.1:5001")
        .await?;
    let mut subscribe_all = sui
        .event_api()
        .subscribe_event(SuiEventFilter::All(vec![]))
        .await?;
    loop {
        println!("{:?}", subscribe_all.next().await);
    }
}
