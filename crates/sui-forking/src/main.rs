// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod cli;

use std::net::IpAddr;

use anyhow::{Context, Result};
use clap::Parser;
use sui_forking::{ForkingClient, ForkingNetwork, ForkingNode, ForkingNodeConfig, StartupSeeding};
use tracing::info;
use url::Url;

use crate::cli::{Args, Commands};

fn client_from_server_url(server_url: &str) -> Result<ForkingClient> {
    let base_url =
        Url::parse(server_url).with_context(|| format!("invalid server URL '{}'", server_url))?;
    Ok(ForkingClient::new(base_url))
}

fn startup_seeding(args: cli::StartupSeedArgs) -> StartupSeeding {
    if !args.accounts.is_empty() {
        return StartupSeeding::Accounts(args.accounts);
    }
    if !args.objects.is_empty() {
        return StartupSeeding::Objects(args.objects);
    }
    StartupSeeding::None
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    match args.command {
        Commands::Start {
            host,
            checkpoint,
            rpc_port,
            server_port,
            network,
            fullnode_url,
            data_dir,
            seeds,
        } => {
            let fork_network = ForkingNetwork::parse(&network)?;
            let host: IpAddr = host
                .parse()
                .with_context(|| format!("invalid host IP '{}'", host))?;

            info!(
                "Starting forking server with {} as the starting point...",
                fork_network
            );

            let mut builder = ForkingNodeConfig::builder()
                .network(fork_network)
                .host(host)
                .server_port(server_port)
                .rpc_port(rpc_port)
                .startup_seeding(startup_seeding(seeds));

            if let Some(checkpoint) = checkpoint {
                builder = builder.checkpoint(checkpoint);
            }
            if let Some(fullnode_url) = fullnode_url {
                let url = Url::parse(&fullnode_url)
                    .with_context(|| format!("invalid fullnode URL '{}'", fullnode_url))?;
                builder = builder.fullnode_url(url);
            }
            if let Some(data_dir) = data_dir {
                builder = builder.data_dir(data_dir);
            }

            let node = ForkingNode::start(builder.build()?).await?;
            node.wait().await?;
        }
        Commands::AdvanceCheckpoint { server_url } => {
            client_from_server_url(&server_url)?
                .advance_checkpoint()
                .await?
        }
        Commands::AdvanceClock { server_url, ms } => {
            client_from_server_url(&server_url)?
                .advance_clock(ms)
                .await?
        }
        Commands::AdvanceEpoch { server_url } => {
            client_from_server_url(&server_url)?.advance_epoch().await?
        }
        Commands::Status { server_url } => {
            let status = client_from_server_url(&server_url)?.status().await?;
            println!("Checkpoint: {}", status.checkpoint);
            println!("Epoch: {}", status.epoch);
            println!("Clock timestamp (ms): {}", status.clock_timestamp_ms);
        }
        Commands::Faucet {
            server_url,
            address,
            amount,
        } => {
            let client = client_from_server_url(&server_url)?;
            client.faucet(address, amount).await?;
            client.advance_checkpoint().await?;
            println!("Faucet request completed");
        }
    }

    Ok(())
}
