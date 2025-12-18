// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod cli;
mod context;
mod graphql;
mod indexers;
mod rpc;
// mod seeds;
mod server;
mod store;

use anyhow::Result;
use clap::Parser;
use tracing::info;

use sui_types::supported_protocol_versions::Chain;

use crate::cli::{Args, Commands};
// use crate::seeds::Network;
use crate::server::server::start_server;
use crate::server::{AdvanceClockRequest, ApiResponse, ExecuteTxRequest, ForkingStatus};
use std::path::PathBuf;

// Define the `GIT_REVISION` const
bin_version::git_revision!();

static VERSION: &str = const_str::concat!(
    env!("CARGO_PKG_VERSION_MAJOR"),
    ".",
    env!("CARGO_PKG_VERSION_MINOR"),
    ".",
    env!("CARGO_PKG_VERSION_PATCH"),
    "-",
    GIT_REVISION
);

async fn send_command(url: &str, endpoint: &str, body: Option<serde_json::Value>) -> Result<()> {
    let client = reqwest::Client::new();
    let full_url = format!("{}/{}", url, endpoint);

    let response = if let Some(body) = body {
        client.post(&full_url).json(&body).send().await?
    } else {
        client.post(&full_url).send().await?
    };

    if response.status().is_success() {
        let result: ApiResponse<serde_json::Value> = response.json().await?;
        if result.success {
            println!("Success: {:?}", result.data);
        } else {
            println!("Error: {:?}", result.error);
        }
    } else {
        println!("Server error: {}", response.status());
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    match args.command {
        Commands::Start {
            host,
            checkpoint,
            port,
            network,
            data_dir,
            // accounts,
        } => {
            let chain = match network.as_str() {
                "mainnet" => Chain::Mainnet,
                "testnet" => Chain::Testnet,
                _ => Chain::Unknown,
            };

            // let mut seeds = vec![];
            // let graphql_endpoint = Network::from_str(&network)?;
            // for addr in accounts.accounts.iter() {
            //     let owned_objects = fetch_owned_objects(&graphql_endpoint, *addr).await?;
            //     seeds.extend(owned_objects);
            // }
            // info!("Downloaded seeds for {} accounts", accounts.accounts.len());
            info!("Starting forking server...");

            let data_ingestion_path = if let Some(data_dir) = data_dir {
                let path = PathBuf::from(data_dir);
                if !path.exists() {
                    std::fs::create_dir_all(&path).expect("Failed to create data directory");
                }
                path
            } else {
                mysten_common::tempdir().unwrap().keep()
            };
            start_server(chain, checkpoint, host, port, data_ingestion_path, VERSION).await?
        }
        Commands::AdvanceCheckpoint { server_url } => {
            send_command(&server_url, "advance-checkpoint", None).await?
        }
        Commands::AdvanceClock {
            server_url,
            seconds,
        } => {
            let body = serde_json::json!(AdvanceClockRequest { seconds });
            send_command(&server_url, "advance-clock", Some(body)).await?
        }
        Commands::AdvanceEpoch { server_url } => {
            send_command(&server_url, "advance-epoch", None).await?
        }
        Commands::Status { server_url } => {
            let client = reqwest::Client::new();
            let response = client.get(format!("{}/status", server_url)).send().await?;

            if response.status().is_success() {
                let result: ApiResponse<ForkingStatus> = response.json().await?;
                if let Some(status) = result.data {
                    println!("Checkpoint: {}", status.checkpoint);
                    println!("Epoch: {}", status.epoch);
                    println!("Transactions: {}", status.transaction_count);
                } else {
                    println!("Error: {:?}", result.error);
                }
            } else {
                println!("Server error: {}", response.status());
            }
        }
        Commands::ExecuteTx {
            server_url,
            tx_bytes,
        } => {
            let body = serde_json::json!(ExecuteTxRequest { tx_bytes });
            send_command(&server_url, "execute-tx", Some(body)).await?
        }

        Commands::Faucet {
            server_url,
            address,
            amount,
        } => {
            send_command(
                &server_url,
                "faucet",
                Some(serde_json::json!({
                    "address": address,
                    "amount": amount,
                })),
            )
            .await?
        }
    }

    Ok(())
}
