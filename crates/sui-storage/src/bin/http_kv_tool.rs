// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use std::str::FromStr;
use std::sync::Arc;
use sui_storage::http_key_value_store::*;
use sui_storage::key_value_store::TransactionKeyValueStore;
use sui_storage::key_value_store_metrics::KeyValueStoreMetrics;
use sui_types::digests::{TransactionDigest, TransactionEventsDigest};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

// Command line options are:
// --base-url <url> - the base URL of the HTTP server
// --digest <digest> - the digest of the key being fetched
// --type <fx|tx|ev> - the type of key being fetched
#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
struct Options {
    // default value of 'https://transactions.sui.io/'
    #[clap(short, long, default_value = "https://transactions.sui.io/mainnet")]
    base_url: String,

    #[clap(short, long)]
    digest: Vec<String>,

    // must be either 'tx', 'fx', 'events', or 'ckpt_contents'
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

    let http_kv = Arc::new(HttpKVStore::new(&options.base_url).unwrap());
    let kv =
        TransactionKeyValueStore::new("http_kv", KeyValueStoreMetrics::new_for_tests(), http_kv);

    // verify that type is valid
    match options.type_.as_str() {
        "tx" | "fx" => {
            let digests: Vec<_> = options
                .digest
                .into_iter()
                .map(|digest| {
                    TransactionDigest::from_str(&digest).expect("invalid transaction digest")
                })
                .collect();

            if options.type_ == "tx" {
                let tx = kv.multi_get_tx(&digests).await.unwrap();
                for (digest, tx) in digests.iter().zip(tx.iter()) {
                    println!("fetched tx: {:?} {:?}", digest, tx);
                }
            } else {
                let fx = kv.multi_get_fx_by_tx_digest(&digests).await.unwrap();
                for (digest, fx) in digests.iter().zip(fx.iter()) {
                    println!("fetched fx: {:?} {:?}", digest, fx);
                }
            }
        }

        "events" => {
            let digests: Vec<_> = options
                .digest
                .into_iter()
                .map(|digest| {
                    TransactionEventsDigest::from_str(&digest).expect("invalid events digest")
                })
                .collect();

            let tx = kv.multi_get_events(&digests).await.unwrap();
            for (digest, ev) in digests.iter().zip(tx.iter()) {
                println!("fetched events: {:?} {:?}", digest, ev);
            }
        }

        "ckpt_contents" => {
            let seqs: Vec<_> = options
                .digest
                .into_iter()
                .map(|s| {
                    CheckpointSequenceNumber::from_str(&s)
                        .expect("invalid checkpoint sequence number")
                })
                .collect();

            let tx = kv.multi_get_checkpoints_contents(&seqs).await.unwrap();
            for (digest, ckpt) in seqs.iter().zip(tx.iter()) {
                println!("fetched ckpt: {:?} {:?}", digest, ckpt);
            }
        }

        _ => {
            println!(
                "Invalid key type: {}. Must be one of 'tx', 'fx', or 'ev'.",
                options.type_
            );
            std::process::exit(1);
        }
    }
}
