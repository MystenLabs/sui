// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_trace_format::format::TypeTagWithRefs;

use serde::Serialize;

use crate::base_types::ObjectID;

/// Information about the PTB commands to be stored in the PTB start event.
/// It contains only the information needed to provide a summary of the command.
#[derive(Clone, Debug, Serialize)]
pub enum PTBCommandInfo {
    MoveCall {
        pkg: String,
        module: String,
        function: String,
    },
    TransferObjects,
    SplitCoins,
    Publish,
    MergeCoins,
    MakeMoveVec,
    Upgrade,
}

/// External events to be stored in the trace. The first one is a summary
/// of the whole PTB, and the following ones represent individual PTB commands.
#[derive(Clone, Debug, Serialize)]
pub enum PTBExternalEvent {
    Summary(Vec<PTBCommandInfo>),
    MoveCallStart,   // just a marker, all required info is in OpenFrame event
    MoveCallEnd,     // just a marker to make identifying the end of a MoveCall easier
    TransferObjects, // TODD
    SplitCoins(SplitCoinsEvent),
    Publish,     // TODD
    MergeCoins,  // TODO
    MakeMoveVec, // TODO
    Upgrade,     // TODO
}

/// Information about the coin store in external trace events.
#[derive(Clone, Debug, Serialize)]
pub struct CoinInfo {
    /// Identifier of the coin object.
    pub object_id: ObjectID,
    /// Coin balance.
    pub balance: u64,
}

/// Information about the SplitCoins external event.
#[derive(Clone, Debug, Serialize)]
pub struct SplitCoinsEvent {
    /// Type of both input and output coins.
    pub type_: TypeTagWithRefs,
    /// Input coin.
    pub input: CoinInfo,
    /// Output coins.
    pub result: Vec<CoinInfo>,
}
