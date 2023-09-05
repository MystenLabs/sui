// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    address::Address,
    base64::Base64,
    epoch::Epoch,
    gas::{GasEffects, GasInput},
    sui_address::SuiAddress,
};
use async_graphql::*;

#[derive(SimpleObject, Clone, Eq, PartialEq)]
pub(crate) struct TransactionBlock {
    pub digest: String,
    pub effects: Option<TransactionBlockEffects>,
    pub sender: Option<Address>,
    pub bcs: Option<Base64>,
    pub gas_input: Option<GasInput>,
    pub expiration: Option<Epoch>,
}

#[derive(SimpleObject, Clone, Eq, PartialEq)]
pub(crate) struct TransactionBlockEffects {
    pub digest: String,
    pub gas_effects: Option<GasEffects>,
    pub epoch: Option<Epoch>,
    pub status: Option<ExecutionStatus>,
    pub errors: Option<String>,
    // pub transaction_block: TransactionBlock,
    // pub dependencies: Vec<TransactionBlock>,
    // pub lamport_version: Option<u64>,
    // pub object_reads: Vec<Object>,
    // pub object_changes: Vec<ObjectChange>,
    // pub balance_changes: Vec<BalanceChange>,
    // pub epoch: Epoch
    // pub checkpoint: Checkpoint
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub(crate) enum TransactionBlockKindInput {
    ProgrammableTx,
    SystemTx,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum ExecutionStatus {
    Success,
    Failure,
}

#[derive(InputObject)]
pub(crate) struct TransactionBlockFilter {
    package: Option<SuiAddress>,
    module: Option<String>,
    function: Option<String>,

    kind: Option<TransactionBlockKindInput>,
    checkpoint: Option<u64>,

    sign_address: Option<SuiAddress>,
    sent_address: Option<SuiAddress>,
    recv_address: Option<SuiAddress>,
    paid_address: Option<SuiAddress>,

    input_object: Option<SuiAddress>,
    changed_object: Option<SuiAddress>,
}
