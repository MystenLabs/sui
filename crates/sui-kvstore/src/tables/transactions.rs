// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Transactions table: stores transaction data indexed by digest.

use anyhow::{Context, Result};
use bytes::Bytes;
use sui_types::digests::TransactionDigest;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::transaction::Transaction;

use crate::{TransactionData, TransactionEventsData};

pub mod col {
    pub const TX: &str = "tx";
    pub const EFFECTS: &str = "ef";
    pub const EVENTS: &str = "ev";
    pub const TIMESTAMP: &str = "ts";
    pub const CHECKPOINT_NUMBER: &str = "cn";
}

pub const NAME: &str = "transactions";

pub fn encode_key(digest: &TransactionDigest) -> Vec<u8> {
    digest.inner().to_vec()
}

pub fn encode(
    transaction: &Transaction,
    effects: &TransactionEffects,
    events: &Option<TransactionEvents>,
    checkpoint_number: CheckpointSequenceNumber,
    timestamp_ms: u64,
) -> Result<[(&'static str, Bytes); 5]> {
    Ok([
        (col::TX, Bytes::from(bcs::to_bytes(transaction)?)),
        (col::EFFECTS, Bytes::from(bcs::to_bytes(effects)?)),
        (col::EVENTS, Bytes::from(bcs::to_bytes(events)?)),
        (col::TIMESTAMP, Bytes::from(bcs::to_bytes(&timestamp_ms)?)),
        (
            col::CHECKPOINT_NUMBER,
            Bytes::from(bcs::to_bytes(&checkpoint_number)?),
        ),
    ])
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
