// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use std::str::FromStr;
use std::sync::Arc;
use sui_storage::http_key_value_store::*;
use sui_storage::key_value_store::TransactionKeyValueStore;
use sui_storage::key_value_store_metrics::KeyValueStoreMetrics;
use sui_types::base_types::ObjectID;
use sui_types::digests::{
    CheckpointContentsDigest, CheckpointDigest, TransactionDigest, TransactionEventsDigest,
};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

// Command line options are:
// --base-url <url> - the base URL of the HTTP server
// --digest <digest> - the digest of the key being fetched
// --type <fx|tx|ev> - the type of key being fetched
#[derive(Parser)]
#[command(rename_all = "kebab-case")]
struct Options {
    // default value of 'https://transactions.sui.io/'
    #[arg(short, long, default_value = "https://transactions.sui.io/mainnet")]
    base_url: String,

    #[arg(short, long)]
    digest: Vec<String>,

    #[arg(short, long)]
    seq: Vec<String>,

    // must be either 'tx', 'fx','ob','events', or 'ckpt_contents'
    // default value of 'tx'
    #[arg(short, long, default_value = "tx")]
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

    let seqs: Vec<_> = options
        .seq
        .into_iter()
        .map(|s| {
            CheckpointSequenceNumber::from_str(&s).expect("invalid checkpoint sequence number")
        })
        .collect();

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
            let digests: Vec<_> = options
                .digest
                .into_iter()
                .map(|s| CheckpointContentsDigest::from_str(&s).expect("invalid checkpoint digest"))
                .collect();

            let ckpts = kv
                .multi_get_checkpoints(&[], &seqs, &[], &digests)
                .await
                .unwrap();

            for (seq, ckpt) in seqs.iter().zip(ckpts.1.iter()) {
                // populate digest before printing
                ckpt.as_ref().map(|c| c.digest());
                println!("fetched ckpt contents: {:?} {:?}", seq, ckpt);
            }
            for (digest, ckpt) in digests.iter().zip(ckpts.3.iter()) {
                // populate digest before printing
                ckpt.as_ref().map(|c| c.digest());
                println!("fetched ckpt contents: {:?} {:?}", digest, ckpt);
            }
        }

        "ckpt_summary" => {
            let digests: Vec<_> = options
                .digest
                .into_iter()
                .map(|s| CheckpointDigest::from_str(&s).expect("invalid checkpoint digest"))
                .collect();

            let ckpts = kv
                .multi_get_checkpoints(&seqs, &[], &digests, &[])
                .await
                .unwrap();

            for (seq, ckpt) in seqs.iter().zip(ckpts.0.iter()) {
                // populate digest before printing
                ckpt.as_ref().map(|c| c.digest());
                println!("fetched ckpt summary: {:?} {:?}", seq, ckpt);
            }
            for (digest, ckpt) in digests.iter().zip(ckpts.2.iter()) {
                // populate digest before printing
                ckpt.as_ref().map(|c| c.digest());
                println!("fetched ckpt summary: {:?} {:?}", digest, ckpt);
            }
        }

        "ob" => {
            let object_id = ObjectID::from_str(&options.digest[0]).expect("invalid object id");
            let object = kv.get_object(object_id, seqs[0].into()).await.unwrap();
            println!("fetched object {:?}", object);
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
