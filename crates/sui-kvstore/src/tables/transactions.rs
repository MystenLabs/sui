// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Transactions table: stores transaction data indexed by digest.

use anyhow::{Context, Result};
use bytes::Bytes;
use sui_types::balance_change::BalanceChange;
use sui_types::digests::TransactionDigest;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::storage::ObjectKey;
use sui_types::transaction::Transaction;

use crate::{TransactionData, TransactionEventsData};

pub mod col {
    pub const EFFECTS: &str = "ef";
    pub const EVENTS: &str = "ev";
    pub const TIMESTAMP: &str = "ts";
    pub const CHECKPOINT_NUMBER: &str = "cn";
    pub const DATA: &str = "td";
    pub const SIGNATURES: &str = "sg";
    pub const BALANCE_CHANGES: &str = "bc";
    pub const UNCHANGED_LOADED: &str = "ul";
    /// Deprecated: Full Transaction (data+sigs combined). Use DATA+SIGNATURES instead.
    pub const TX: &str = "tx";
}

pub const NAME: &str = "transactions";

pub fn encode_key(digest: &TransactionDigest) -> Vec<u8> {
    digest.inner().to_vec()
}

/// Encode all transaction columns.
/// Writes 9 columns: TX (deprecated but kept for backwards compat), EFFECTS, EVENTS,
/// TIMESTAMP, CHECKPOINT_NUMBER, DATA, SIGNATURES, BALANCE_CHANGES, UNCHANGED_LOADED.
pub fn encode(
    transaction: &Transaction,
    effects: &TransactionEffects,
    events: &Option<TransactionEvents>,
    checkpoint_number: CheckpointSequenceNumber,
    timestamp_ms: u64,
    balance_changes: &[BalanceChange],
    unchanged_loaded_runtime_objects: &[ObjectKey],
) -> Result<[(&'static str, Bytes); 9]> {
    Ok([
        // Deprecated: full transaction (use DATA+SIGNATURES)
        (col::TX, Bytes::from(bcs::to_bytes(transaction)?)),
        (col::EFFECTS, Bytes::from(bcs::to_bytes(effects)?)),
        (col::EVENTS, Bytes::from(bcs::to_bytes(events)?)),
        (col::TIMESTAMP, Bytes::from(bcs::to_bytes(&timestamp_ms)?)),
        (
            col::CHECKPOINT_NUMBER,
            Bytes::from(bcs::to_bytes(&checkpoint_number)?),
        ),
        (
            col::DATA,
            Bytes::from(bcs::to_bytes(&transaction.data().intent_message().value)?),
        ),
        (
            col::SIGNATURES,
            Bytes::from(bcs::to_bytes(transaction.data().tx_signatures())?),
        ),
        (
            col::BALANCE_CHANGES,
            Bytes::from(bcs::to_bytes(balance_changes)?),
        ),
        (
            col::UNCHANGED_LOADED,
            Bytes::from(bcs::to_bytes(unchanged_loaded_runtime_objects)?),
        ),
    ])
}

/// Decode transaction data from row cells.
/// Prefers new format (DATA + SIGNATURES), falls back to legacy (TX).
pub fn decode(row: &[(Bytes, Bytes)]) -> Result<TransactionData> {
    // Legacy column (combined transaction data + signatures)
    let mut transaction_legacy: Option<Transaction> = None;
    // New columns (separate data and signatures)
    let mut tx_data = None;
    let mut tx_signatures = None;

    let mut effects = None;
    let mut events = None;
    let mut timestamp = 0;
    let mut checkpoint_number = 0;
    let mut balance_changes: Option<Vec<_>> = None;
    let mut unchanged_loaded: Option<Vec<_>> = None;

    for (column, value) in row {
        match column.as_ref() {
            b"tx" => transaction_legacy = Some(bcs::from_bytes(value)?),
            b"td" => tx_data = Some(bcs::from_bytes(value)?),
            b"sg" => tx_signatures = Some(bcs::from_bytes(value)?),
            b"ef" => effects = Some(bcs::from_bytes(value)?),
            b"ev" => events = Some(bcs::from_bytes(value)?),
            b"ts" => timestamp = bcs::from_bytes(value)?,
            b"cn" => checkpoint_number = bcs::from_bytes(value)?,
            b"bc" => balance_changes = Some(bcs::from_bytes(value)?),
            b"ul" => unchanged_loaded = Some(bcs::from_bytes(value)?),
            _ => {}
        }
    }

    // Prefer new columns (DATA + SIGNATURES), fallback to legacy (TX)
    let transaction = match (tx_data, tx_signatures) {
        (Some(data), Some(sigs)) => Transaction::from_generic_sig_data(data, sigs),
        _ => transaction_legacy.context("transaction field is missing")?,
    };

    Ok(TransactionData {
        transaction,
        effects: effects.context("effects field is missing")?,
        events: events.context("events field is missing")?,
        timestamp,
        checkpoint_number,
        balance_changes: balance_changes.unwrap_or_default(),
        unchanged_loaded_runtime_objects: unchanged_loaded.unwrap_or_default(),
    })
}

/// Decode only events and timestamp from a row (for partial reads).
pub fn decode_events(row: &[(Bytes, Bytes)]) -> Result<TransactionEventsData> {
    let mut transaction_events: Option<Option<TransactionEvents>> = None;
    let mut timestamp_ms = 0;

    for (column, value) in row {
        match column.as_ref() {
            b"ev" => transaction_events = Some(bcs::from_bytes(value)?),
            b"ts" => timestamp_ms = bcs::from_bytes(value)?,
            _ => {}
        }
    }

    let events = transaction_events
        .context("events field is missing")?
        .map(|e| e.data)
        .unwrap_or_default();

    Ok(TransactionEventsData {
        events,
        timestamp_ms,
    })
}
