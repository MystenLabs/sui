// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::*;
use std::str::FromStr;
use std::sync::Arc;
use sui_storage::http_key_value_store::*;
use sui_storage::key_value_store::TransactionKeyValueStore;
use sui_storage::key_value_store_metrics::KeyValueStoreMetrics;
use sui_types::base_types::ObjectID;
use sui_types::digests::{CheckpointDigest, TransactionDigest};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

#[derive(Parser)]
#[command(rename_all = "kebab-case")]
enum Command {
    Fetch {
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
    },

    DecodeKey {
        #[arg(short, long)]
        url: String,
    },
}

impl Command {
    async fn execute(self) -> anyhow::Result<(), anyhow::Error> {
        match self {
            Command::Fetch {
                base_url,
                digest,
                seq,
                type_,
            } => {
                let metrics = KeyValueStoreMetrics::new_for_tests();
                let http_kv = Arc::new(HttpKVStore::new(&base_url, 100, metrics).unwrap());
                let kv = TransactionKeyValueStore::new(
                    "http_kv",
                    KeyValueStoreMetrics::new_for_tests(),
                    http_kv,
                );

                let seqs: Vec<_> = seq
                    .into_iter()
                    .map(|s| {
                        CheckpointSequenceNumber::from_str(&s)
                            .expect("invalid checkpoint sequence number")
                    })
                    .collect();

                // verify that type is valid
                match type_.as_str() {
                    "tx" | "fx" => {
                        let digests: Vec<_> = digest
                            .into_iter()
                            .map(|digest| {
                                TransactionDigest::from_str(&digest)
                                    .expect("invalid transaction digest")
                            })
                            .collect();

                        if type_ == "tx" {
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

                    "ckpt_contents" => {
                        let ckpts = kv.multi_get_checkpoints(&[], &seqs, &[]).await.unwrap();

                        for (seq, ckpt) in seqs.iter().zip(ckpts.1.iter()) {
                            // populate digest before printing
                            ckpt.as_ref().map(|c| c.digest());
                            println!("fetched ckpt contents: {:?} {:?}", seq, ckpt);
                        }
                    }

                    "ckpt_summary" => {
                        let digests: Vec<_> = digest
                            .into_iter()
                            .map(|s| {
                                CheckpointDigest::from_str(&s).expect("invalid checkpoint digest")
                            })
                            .collect();

                        let ckpts = kv
                            .multi_get_checkpoints(&seqs, &[], &digests)
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
                        let object_id = ObjectID::from_str(&digest[0]).expect("invalid object id");
                        let object = kv.get_object(object_id, seqs[0].into()).await.unwrap();
                        println!("fetched object {:?}", object);
                    }

                    _ => {
                        println!(
                            "Invalid key type: {}. Must be one of 'tx', 'fx', or 'ev'.",
                            type_
                        );
                        std::process::exit(1);
                    }
                }

                Ok(())
            }
            Command::DecodeKey { url } => {
                // url may look like
                // https://transactions.sui.io/mainnet/jlkqmZbVuunngIyy2vjBOJSETrM56EH_kIc5wuLvDydN_x0GAAAAAA/ob
                // extract the digest and type
                let parts: Vec<_> = url.split('/').collect();

                // its allowed to supply either the whole URL, or the last two pieces
                if parts.len() < 2 {
                    println!("Invalid URL: {}", url);
                    std::process::exit(1);
                }

                let identifier = parts[parts.len() - 2];
                let type_ = parts[parts.len() - 1];

                let key = path_elements_to_key(identifier, type_)?;
                println!("decoded key: {:?}", key);
                Ok(())
            }
        }
    }
}

#[derive(Parser)]
#[command(
    name = "http_kv_tool",
    about = "Utilities for the HTTP key-value store",
    rename_all = "kebab-case",
    author,
    version = "1"
)]
struct App {
    #[command(subcommand)]
    command: Command,
}

#[tokio::main]
async fn main() {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let app = App::parse();
    app.command.execute().await.unwrap();
}
