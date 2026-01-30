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

use crate::write_legacy_data;
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
/// Writes 8 columns by default (or 9 when `write_legacy_data()` is enabled, which adds
/// the deprecated TX column).
pub fn encode(
    transaction: &Transaction,
    effects: &TransactionEffects,
    events: &Option<TransactionEvents>,
    checkpoint_number: CheckpointSequenceNumber,
    timestamp_ms: u64,
    balance_changes: &[BalanceChange],
    unchanged_loaded_runtime_objects: &[ObjectKey],
) -> Result<Vec<(&'static str, Bytes)>> {
    let mut cols = Vec::with_capacity(if write_legacy_data() { 9 } else { 8 });

    if write_legacy_data() {
        cols.push((col::TX, Bytes::from(bcs::to_bytes(transaction)?)));
    }

    cols.extend([
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
    ]);

    Ok(cols)
}

pub fn decode(row: &[(Bytes, Bytes)]) -> Result<TransactionData> {
    let mut transaction = None;
    let mut effects = None;
    let mut events = None;
    let mut timestamp = 0;
    let mut checkpoint_number = 0;

    for (column, value) in row {
        match column.as_ref() {
            b"tx" => transaction = Some(bcs::from_bytes(value)?),
            b"ef" => effects = Some(bcs::from_bytes(value)?),
            b"ev" => events = Some(bcs::from_bytes(value)?),
            b"ts" => timestamp = bcs::from_bytes(value)?,
            b"cn" => checkpoint_number = bcs::from_bytes(value)?,
            _ => {}
        }
    }

    Ok(TransactionData {
        transaction: transaction.context("transaction field is missing")?,
        effects: effects.context("effects field is missing")?,
        events: events.context("events field is missing")?,
        timestamp,
        checkpoint_number,
        balance_changes: Vec::new(),
        unchanged_loaded_runtime_objects: Vec::new(),
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
