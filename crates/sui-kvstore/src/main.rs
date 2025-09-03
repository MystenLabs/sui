// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::Result;
use clap::{Parser, Subcommand};
use prometheus::Registry;
use std::io::{self, Write};
use std::str::FromStr;
use sui_data_ingestion_core::{DataIngestionMetrics, IndexerExecutor, ReaderOptions, WorkerPool};
use sui_kvstore::{BigTableClient, BigTableProgressStore, KeyValueStoreReader, KvWorker};
use sui_types::base_types::ObjectID;
use sui_types::digests::TransactionDigest;
use sui_types::storage::ObjectKey;
use telemetry_subscribers::TelemetryConfig;
use tokio::sync::oneshot;

#[derive(Parser)]
struct App {
    instance_id: String,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    Ingestion {
        network: String,
    },
    Fetch {
        #[command(subcommand)]
        entry: Entry,
    },
}

#[derive(Subcommand)]
pub enum Entry {
    Object { id: String, version: u64 },
    Epoch { id: u64 },
    Checkpoint { id: u64 },
    Transaction { id: String },
    Watermark,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = TelemetryConfig::new().with_env().init();
    let app = App::parse();
    match app.command {
        Some(Command::Ingestion { network }) => {
            let client = BigTableClient::new_remote(
                app.instance_id,
                false,
                None,
                "ingestion".to_string(),
                None,
                None,
            )
            .await?;
            let (_exit_sender, exit_receiver) = oneshot::channel();
            let mut executor = IndexerExecutor::new(
                BigTableProgressStore::new(client.clone()),
                1,
                DataIngestionMetrics::new(&Registry::new()),
            );
            let worker_pool = WorkerPool::new(KvWorker { client }, "bigtable".to_string(), 50);
            executor.register(worker_pool).await?;
            executor
                .run(
                    tempfile::tempdir()?.keep(),
                    Some(format!("https://checkpoints.{}.sui.io", network)),
                    vec![],
                    ReaderOptions::default(),
                    exit_receiver,
                )
                .await?;
        }
        Some(Command::Fetch { entry }) => {
            let mut client = BigTableClient::new_remote(
                app.instance_id,
                true,
                None,
                "cli".to_string(),
                None,
                None,
            )
            .await?;
            let result = match entry {
                Entry::Epoch { id } => client.get_epoch(id).await?.map(|e| bcs::to_bytes(&e)),
                Entry::Object { id, version } => {
                    let objects = client
                        .get_objects(&[ObjectKey(ObjectID::from_str(&id)?, version.into())])
                        .await?;
                    objects.first().map(bcs::to_bytes)
                }
                Entry::Checkpoint { id } => {
                    let checkpoints = client.get_checkpoints(&[id]).await?;
                    checkpoints.first().map(bcs::to_bytes)
                }
                Entry::Transaction { id } => {
                    let transactions = client
                        .get_transactions(&[TransactionDigest::from_str(&id)?])
                        .await?;
                    transactions.first().map(bcs::to_bytes)
                }
                Entry::Watermark => {
                    let watermark = client.get_latest_checkpoint().await?;
                    println!("watermark is {}", watermark);
                    return Ok(());
                }
            };
            match result {
                Some(bytes) => io::stdout().write_all(&bytes?)?,
                None => println!("not found"),
            }
        }
        None => println!("no command provided"),
    }
    Ok(())
}
