// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use sui_types::{
    crypto::EmptySignInfo, effects::TransactionEffects, event::Event,
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