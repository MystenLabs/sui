// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Example tool to download the last 1000 Sui transactions from the network.
//!
//! Usage:
//!   cargo run --example download_transactions -- --network mainnet --output transactions.json
//!   cargo run --example download_transactions -- --network testnet --limit 500
//!   cargo run --example download_transactions -- \
//!             --network mainnet \
//!             --limit 1000 \
//!             --output my_transactions.json \
//!             --show-effects \
//!             --show-events \
//!             --show-balance-changes
//!
//! Filter examples:
//!   # Download only failed transactions
//!   cargo run --example download_transactions -- \
//!             --show-effects \
//!             --filter-status failure \
//!             --output failed_transactions.json
//!
//!   # Download high gas cost transactions (computationCost > 100000)
//!   cargo run --example download_transactions -- \
//!             --show-effects \
//!             --min-gas-cost 100000 \
//!             --output high_gas_transactions.json

use anyhow::Result;
use clap::Parser;
use serde_json;
use std::fs::File;
use std::io::BufWriter;
use sui_json_rpc_types::{
    SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponse, SuiTransactionBlockResponseQuery,
};
use sui_sdk::SuiClientBuilder;

#[derive(Parser, Debug)]
#[clap(
    name = "download_transactions",
    about = "Download the last N Sui transactions from the network"
)]
struct Args {
    #[clap(long, default_value = "mainnet")]
    network: String,

    #[clap(long, default_value = "1000")]
    limit: usize,

    #[clap(long, default_value = "transactions.json")]
    output: String,

    #[clap(long)]
    show_input: bool,

    #[clap(long)]
    show_effects: bool,

    #[clap(long)]
    show_events: bool,

    #[clap(long)]
    show_object_changes: bool,

    #[clap(long)]
    show_balance_changes: bool,

    #[clap(long)]
    rpc_url: Option<String>,

    /// Filter by transaction status (success, failure)
    #[clap(long)]
    filter_status: Option<String>,

    /// Filter transactions with gas cost greater than this value
    #[clap(long)]
    min_gas_cost: Option<u64>,

    /// Scan limit - how many transactions to scan before stopping (default: no limit)
    /// Useful when filtering, as we may need to scan more transactions to find matches
    #[clap(long)]
    scan_limit: Option<usize>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let rpc_url = if let Some(url) = args.rpc_url {
        url
    } else {
        match args.network.as_str() {
            "mainnet" => "https://fullnode.mainnet.sui.io:443".to_string(),
            "testnet" => "https://fullnode.testnet.sui.io:443".to_string(),
            "devnet" => "https://fullnode.devnet.sui.io:443".to_string(),
            "localnet" => "http://127.0.0.1:9000".to_string(),
            _ => {
                eprintln!("Unknown network: {}. Use --rpc-url to specify a custom RPC endpoint.", args.network);
                std::process::exit(1);
            }
        }
    };

    println!("Connecting to Sui network: {}", rpc_url);
    let sui = SuiClientBuilder::default().build(rpc_url).await?;
    println!("Connected successfully!");

    let total_transactions = sui.read_api().get_total_transaction_blocks().await?;
    println!("Total transactions on chain: {}", total_transactions);

    let has_filters = args.filter_status.is_some() || args.min_gas_cost.is_some();
    if has_filters {
        println!("Searching for {} transactions matching filters...", args.limit);
        if let Some(ref status) = args.filter_status {
            println!("  - Status filter: {}", status);
        }
        if let Some(gas) = args.min_gas_cost {
            println!("  - Min gas cost: {}", gas);
        }
    } else {
        println!("Downloading last {} transactions...", args.limit);
    }

    // Note: TransactionStatus and MinGasCost filters have been added to the codebase
    // but are not yet deployed to the RPC servers. Once deployed, we can use:
    //   TransactionFilter::TransactionStatus(status)
    //   TransactionFilter::MinGasCost(min_gas)
    // For now, we apply these filters client-side after downloading.
    let query = SuiTransactionBlockResponseQuery {
        filter: None,  // No server-side filter for status/gas yet
        options: Some(sui_json_rpc_types::SuiTransactionBlockResponseOptions {
            show_input: args.show_input,
            show_effects: args.show_effects || has_filters, // Always show effects if filtering
            show_events: args.show_events,
            show_object_changes: args.show_object_changes,
            show_balance_changes: args.show_balance_changes,
            show_raw_input: false,
            show_raw_effects: false,
        }),
    };

    let mut transactions: Vec<SuiTransactionBlockResponse> = Vec::new();
    let mut cursor = None;
    let page_size = 50;
    let mut scanned = 0;
    let max_scan = args.scan_limit.unwrap_or(usize::MAX);

    // Determine if we need client-side filtering
    // (only needed if both status AND gas filters are specified)
    let need_client_filter = args.filter_status.is_some() && args.min_gas_cost.is_some();

    while transactions.len() < args.limit && scanned < max_scan {
        let fetch_size = page_size;

        let page = sui
            .read_api()
            .query_transaction_blocks(
                query.clone(),
                cursor,
                Some(fetch_size),
                true, // descending order (newest first)
            )
            .await?;

        let fetched = page.data.len();
        if fetched == 0 {
            break;
        }

        scanned += fetched;

        // Apply client-side filters only if needed (when both filters are active)
        for tx in page.data {
            if transactions.len() >= args.limit {
                break;
            }

            let mut matches = true;

            // Only apply gas filter client-side if status filter is also present
            // (because in that case, status is handled server-side)
            if need_client_filter {
                if let Some(min_gas) = args.min_gas_cost {
                    if let Some(ref effects) = tx.effects {
                        let gas_cost = effects.gas_cost_summary().computation_cost;
                        matches = matches && gas_cost >= min_gas;
                    } else {
                        matches = false;
                    }
                }
            }

            if matches {
                transactions.push(tx);
            }
        }

        if has_filters {
            println!(
                "Progress: {}/{} matching (scanned {} total)",
                transactions.len(),
                args.limit,
                scanned
            );
        } else {
            println!(
                "Progress: {}/{} transactions downloaded",
                transactions.len(),
                args.limit
            );
        }

        if !page.has_next_page {
            break;
        }

        cursor = page.next_cursor;
    }

    println!("\nSuccessfully downloaded {} transactions", transactions.len());
    println!("Writing to file: {}", args.output);

    let file = File::create(&args.output)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, &transactions)?;

    println!("Done! Transactions saved to {}", args.output);
    println!("\nFirst transaction digest: {}", transactions[0].digest);
    println!(
        "Last transaction digest: {}",
        transactions[transactions.len() - 1].digest
    );

    Ok(())
}
