// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use std::str::FromStr;
use std::sync::Arc;
use sui_storage::http_key_value_store::*;
use sui_storage::key_value_store::TransactionKeyValueStore;
use sui_storage::key_value_store_metrics::KeyValueStoreMetrics;
use sui_types::digests::TransactionDigest;

// Command line options are:
// --base-url <url> - the base URL of the HTTP server
// --digest <digest> - the digest of the key being fetched
// --type <fx|tx|ev> - the type of key being fetched
#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
struct Options {
    // default value of 'https://transactions.sui.io/'
    #[clap(short, long, default_value = "https://transactions.sui.io/")]
    base_url: String,

    #[clap(short, long)]
    digest: Vec<String>,

    // must be either 'tx', 'fx', or 'ev'
    // default value of 'tx'
    #[clap(short, long, default_value = "tx")]
    type_: String,
}

#[tokio::main]
async fn main() {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let options = Options::parse();

    let digests: Vec<_> = options
        .digest
        .into_iter()
        .map(|digest| TransactionDigest::from_str(&digest).expect("invalid transaction digest"))
        .collect();

    // verify that type is valid
    match options.type_.as_str() {
        "tx" | "fx" | "ev" => (),
        _ => {
            println!(
                "Invalid key type: {}. Must be one of 'tx', 'fx', or 'ev'.",
                options.type_
            );
            std::process::exit(1);
        }
    }

    let http_kv = Arc::new(HttpKVStore::new(&options.base_url).unwrap());
    let kv =
        TransactionKeyValueStore::new("http_kv", KeyValueStoreMetrics::new_for_tests(), http_kv);
    let tx = kv.multi_get_tx(&digests).await.unwrap();

    for (digest, tx) in digests.iter().zip(tx.iter()) {
        println!("fetched tx: {:?} {:?}", digest, tx);
    }
}
