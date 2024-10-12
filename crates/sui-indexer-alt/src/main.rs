// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use sui_indexer_alt::{args::Args, ingestion::IngestionClient};

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    println!("Fetching from {}", args.remote_store_url);

    let client = IngestionClient::new(args.remote_store_url)?;

    let checkpoint = client.fetch(args.start).await?;
    println!(
        "Fetch checkpoint {cp}:\n\
         #transactions   = {txs}\n\
         #events         = {evs}\n\
         #input-objects  = {ins}\n\
         #output-objects = {outs}",
        cp = args.start,
        txs = checkpoint.transactions.len(),
        evs = checkpoint
            .transactions
            .iter()
            .map(|tx| tx.events.as_ref().map_or(0, |evs| evs.data.len()))
            .sum::<usize>(),
        ins = checkpoint
            .transactions
            .iter()
            .map(|tx| tx.input_objects.len())
            .sum::<usize>(),
        outs = checkpoint
            .transactions
            .iter()
            .map(|tx| tx.output_objects.len())
            .sum::<usize>(),
    );

    Ok(())
}
