// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::event::Event;
use super::transaction_block_effects::TransactionBlockEffects;
use async_graphql::*;
use sui_json_rpc_types::BalanceChange as NativeBalanceChange;
use sui_types::{
    effects::TransactionEffects as NativeTransactionEffects,
    transaction::SenderSignedData as NativeSenderSignedData,
};

/// The result of an execution, including errors that occurred during said execution.
#[derive(SimpleObject, Clone)]
pub(crate) struct ExecutionResult {
    /// The errors field captures any errors that occurred during execution
    pub errors: Option<Vec<String>>,

    /// The effects of the executed transaction.
    pub effects: TransactionBlockEffects,
}

#[derive(Clone, Debug)]
pub(crate) struct ExecutedTransaction {
    pub sender_signed_data: NativeSenderSignedData,
    pub raw_effects: NativeTransactionEffects,
    pub balance_changes: Vec<NativeBalanceChange>,
    pub events: Vec<Event>,
}
