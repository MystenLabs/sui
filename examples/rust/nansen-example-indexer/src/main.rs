// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod models;

use anyhow::Result;
use models::{BalanceChange, WrappedEvent, WrappedTransaction};
use std::collections::BTreeMap;
use sui_indexer_alt_framework::ingestion::remote_client::RemoteIngestionClient;
use sui_package_resolver::Resolver;
use sui_rpc_resolver::{json_visitor::JsonVisitor, package_store::RpcPackageStore};
use sui_storage::blob::Blob;
use sui_types::{
    coin::Coin, full_checkpoint_content::{CheckpointData, CheckpointTransaction}, gas_coin::GAS,
};
use url::Url;

/// Calculate balance changes for a transaction.
///
/// This analyzes the input and output objects to determine how balances changed.
/// For failed transactions, only gas charges are considered.
fn calculate_balance_changes(transaction: &CheckpointTransaction) -> Result<Vec<BalanceChange>> {
    use sui_types::effects::TransactionEffectsAPI;

    // If transaction failed, only gas was charged
    if transaction.effects.status().is_err() {
        return Ok(vec![BalanceChange {
            owner: transaction.effects.gas_object().1,
            coin_type: GAS::type_tag().to_canonical_string(/* with_prefix */ true),
            amount: -(transaction.effects.gas_cost_summary().net_gas_usage() as i128),
        }]);
    }

    let mut changes = BTreeMap::new();

    // Process input objects (outflows)
    for object in &transaction.input_objects {
        if let Some((type_, balance)) = Coin::extract_balance_if_coin(object)? {
            *changes.entry((object.owner(), type_)).or_insert(0i128) -= balance as i128;
        }
    }

    // Process output objects (inflows)
    for object in &transaction.output_objects {
        if let Some((type_, balance)) = Coin::extract_balance_if_coin(object)? {
            *changes.entry((object.owner(), type_)).or_insert(0i128) += balance as i128;
        }
    }

    // Convert to vector of BalanceChange
    Ok(changes
        .into_iter()
        .filter(|(_, amount)| *amount != 0) // Filter out zero changes
        .map(|((owner, coin_type), amount)| BalanceChange {
            owner: owner.clone(),
            coin_type: coin_type.to_canonical_string(/* with_prefix */ true),
            amount,
        })
        .collect())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Configuration - modify these values as needed
    let remote_store_url = "https://checkpoints.testnet.sui.io"; // Use testnet checkpoint store
    let rpc_url = "https://fullnode.testnet.sui.io:443"; // For package resolution
    let checkpoint_number = 245424622;

    println!("Fetching checkpoint {} from remote store...", checkpoint_number);
    println!("Remote store URL: {}", remote_store_url);
    println!("RPC URL (for package resolution): {}", rpc_url);

    // Create remote ingestion client for fetching checkpoints
    let client = RemoteIngestionClient::new(Url::parse(remote_store_url)?)?;

    // Fetch checkpoint bytes from remote store
    let response = client.checkpoint(checkpoint_number).await?;
    let bytes = response.bytes().await?;

    // Deserialize checkpoint data from bytes
    let checkpoint_data: CheckpointData = Blob::from_bytes(&bytes)?;

    // Setup package resolver for event deserialization (using RPC)
    let store = RpcPackageStore::new(rpc_url);
    let cached_store = store.with_cache();
    let resolver = Resolver::new(cached_store);


    // Process each transaction
    for (tx_idx, transaction) in checkpoint_data.transactions.iter().enumerate() {
        // Resolve events if present
        let resolved_events_vec = if let Some(events) = &transaction.events {
            let mut wrapped_events = Vec::new();
            for event in &events.data {
                let parsed_json = JsonVisitor::deserialize_event(event, &resolver).await.ok();
                wrapped_events.push(WrappedEvent {
                    event: event.clone(),
                    parsed_json,
                });
            }
            Some(wrapped_events)
        } else {
            None
        };

        // Create wrapped transaction
        let wrapped = WrappedTransaction {
            transaction: transaction.transaction.clone(),
            effects: transaction.effects.clone(),
            events: resolved_events_vec,
            balance_changes: calculate_balance_changes(transaction).ok(),
        };

        // Print as JSON
        match serde_json::to_string_pretty(&wrapped) {
            Ok(json) => println!("{}", json),
            Err(e) => println!("Failed to serialize transaction: {}", e),
        }
    }

    Ok(())
}