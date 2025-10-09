// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use sui_indexer_alt_framework::utils::{
    event_json::deserialize_event_to_json, package_store::RpcPackageStore,
};
use sui_package_resolver::Resolver;
use sui_rpc_api::Client;
use sui_types::digests::CheckpointDigest;
use sui_types::{
    coin::Coin, crypto::EmptySignInfo, effects::TransactionEffects, event::Event,
    full_checkpoint_content::{CheckpointData, CheckpointTransaction}, gas_coin::GAS,
    message_envelope::Envelope, object::Owner, transaction::SenderSignedData,
};

/// Balance change for a single owner and coin type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceChange {
    /// Owner whose balance changed
    pub owner: Owner,
    /// Type of the Coin (canonical string with "0x" prefix)
    pub coin_type: String,
    /// The amount the balance changed by. Negative = outflow, Positive = inflow
    pub amount: i128,
}

/// Wrapped event that includes both original BCS data and resolved JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedEvent {
    /// Original event with BCS data
    #[serde(flatten)]
    pub event: Event,
    /// Resolved JSON representation of the event data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parsed_json: Option<serde_json::Value>,
}

/// Wrapped transaction with resolved events and balance changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedTransaction {
    /// Original transaction data (envelope)
    pub transaction: Envelope<SenderSignedData, EmptySignInfo>,
    /// Transaction effects (contains the digest)
    pub effects: TransactionEffects,
    /// Events with resolved JSON data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<WrappedEvent>>,
    /// Balance changes for this transaction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance_changes: Option<Vec<BalanceChange>>,
}

/// Simplified checkpoint representation with resolved data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedCheckpoint {
    /// Checkpoint sequence number
    pub sequence_number: u64,
    /// Checkpoint content digest
    pub content_digest: CheckpointDigest,
    /// Epoch number
    pub epoch: u64,
    /// Timestamp in milliseconds
    pub timestamp_ms: u64,
    /// Transactions with resolved events
    pub transactions: Vec<WrappedTransaction>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Setup package resolver with our RpcPackageStore (which includes LRU cache)
    let store = RpcPackageStore::new("https://fullnode.testnet.sui.io:443")?;
    let cached_store = store.with_cache();
    let resolver = Resolver::new(cached_store);

    // Create RPC client to fetch checkpoints
    let mut client = Client::new("https://fullnode.testnet.sui.io:443")?;

    println!("Fetching checkpoint 249860250...");

    // Fetch specific checkpoint using RPC
    let checkpoint_data = client
        .get_full_checkpoint(249860250)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to fetch checkpoint: {}", e))?;

    // Convert to resolved checkpoint
    let resolved = convert_to_resolved_checkpoint(checkpoint_data, &resolver).await?;

    // Debug: Print the raw GraphQL response to check if Commands 0 & 1 work without json field
    println!(
        "Raw GraphQL response: {}",
        serde_json::to_string_pretty(&resolved).unwrap()
    );
    // Count events with successfully resolved JSON

    Ok(())
}

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

async fn convert_to_resolved_checkpoint<S>(
    checkpoint: CheckpointData,
    resolver: &Resolver<S>,
) -> Result<ResolvedCheckpoint>
where
    S: sui_package_resolver::PackageStore,
{
    let mut resolved_transactions = Vec::new();

    // Process each transaction
    for transaction in checkpoint.transactions {
        // Calculate balance changes first (before moving anything)
        let balance_changes = match calculate_balance_changes(&transaction) {
            Ok(changes) if !changes.is_empty() => Some(changes),
            Ok(_) => None, // Empty changes
            Err(e) => {
                eprintln!("Failed to calculate balance changes: {}", e);
                None
            }
        };

        let mut resolved_events = Vec::new();
        // Resolve events if present
        if let Some(events) = transaction.events {
            for event in events.data {
                // Try to resolve the event's BCS data to JSON using the helper
                let parsed_json = deserialize_event_to_json(&event, resolver).await.ok();
                resolved_events.push(WrappedEvent { event, parsed_json });
            }
        }

        resolved_transactions.push(WrappedTransaction {
            transaction: transaction.transaction,
            effects: transaction.effects,
            events: if resolved_events.is_empty() { None } else { Some(resolved_events) },
            balance_changes,
        });
    }

    checkpoint.checkpoint_contents.digest();

    Ok(ResolvedCheckpoint {
        sequence_number: checkpoint.checkpoint_summary.sequence_number,
        content_digest: checkpoint.checkpoint_summary.digest().clone(),
        epoch: checkpoint.checkpoint_summary.epoch,
        timestamp_ms: checkpoint.checkpoint_summary.timestamp_ms,
        transactions: resolved_transactions,
    })
}
