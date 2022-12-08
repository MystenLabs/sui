// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

use crate::base_types::{SuiAddress, TransactionDigest};
use crate::event::EventType;
use crate::object::Owner;
use crate::ObjectID;

#[derive(Clone, Debug, JsonSchema, Serialize, Deserialize)]
pub enum TransactionQuery {
    /// All transaction hashes.
    All,
    /// Query by move function.
    MoveFunction {
        package: ObjectID,
        module: Option<String>,
        function: Option<String>,
    },
    /// Query by input object.
    InputObject(ObjectID),
    /// Query by mutated object.
    MutatedObject(ObjectID),
    /// Query by sender address.
    FromAddress(SuiAddress),
    /// Query by recipient address.
    ToAddress(SuiAddress),
}

#[derive(Clone, Debug, JsonSchema, Serialize, Deserialize)]
pub enum EventQuery {
    /// Return all events.
    All,
    /// Return events emitted by the given transaction.
    Transaction(
        ///digest of the transaction, as base-64 encoded string
        TransactionDigest,
    ),
    /// Return events emitted in a specified Move module
    MoveModule {
        /// the Move package ID
        package: ObjectID,
        /// the module name
        module: String,
    },
    /// Return events with the given move event struct name
    MoveEvent(
        /// the event struct name type, e.g. `0x2::devnet_nft::MintNFTEvent` or `0x2::SUI::test_foo<address, vector<u8>>` with type params
        String,
    ),
    EventType(EventType),
    /// Query by sender address.
    Sender(SuiAddress),
    /// Query by recipient address.
    Recipient(Owner),
    /// Return events associated with the given object
    Object(ObjectID),
    /// Return events emitted in [start_time, end_time] interval
    #[serde(rename_all = "camelCase")]
    TimeRange {
        /// left endpoint of time interval, milliseconds since epoch, inclusive
        start_time: u64,
        /// right endpoint of time interval, milliseconds since epoch, exclusive
        end_time: u64,
    },
}
