// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::StreamExt;
use sui_sdk::rpc_types::SuiEventFilter;
use sui_sdk::SuiClient;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClient::new("http://127.0.0.1:5001", Some("ws://127.0.0.1:9001"), None).await?;
    let mut subscribe_all = sui
        .event_api()
        .subscribe_event(SuiEventFilter::All(vec![]))
        .await?;
    loop {
        println!("{:?}", subscribe_all.next().await);
    }
}
