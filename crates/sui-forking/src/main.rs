mod commands;
mod server;
mod types;

use anyhow::Result;
use clap::Parser;
use tracing::info;

use crate::commands::{Args, Commands};
use crate::server::start_server;
use crate::types::{AdvanceClockRequest, ApiResponse, ExecuteTxRequest, ForkingStatus};

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
        } => {
            info!(
                "Starting forking server for {} at checkpoint {:?} at address {}:{}",
                network, checkpoint, host, port
            );
            let info = if let Some(c) = checkpoint {
                format!(
                    "Starting forking server for {} at checkpoint {c} at address {}:{}",
                    network, host, port
                )
            } else {
                format!(
                    "Starting forking server for {} at latest checkpoint at address {}:{}",
                    network, host, port
                )
            };
            println!("{info}");
            start_server(host, port).await?
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
    }

    Ok(())
}
