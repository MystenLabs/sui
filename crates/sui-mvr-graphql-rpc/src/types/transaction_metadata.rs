// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::object::ObjectRef;
use super::sui_address::SuiAddress;
use super::uint53::UInt53;
use async_graphql::*;

/// The optional extra data a user can provide to a transaction dry run.
/// `sender` defaults to `0x0`. If gasObjects` is not present, or is an empty list,
/// it is substituted with a mock Coin object, `gasPrice` defaults to the reference
/// gas price, `gasBudget` defaults to the max gas budget and `gasSponsor` defaults
/// to the sender.
#[derive(Clone, Debug, PartialEq, Eq, InputObject)]
pub(crate) struct TransactionMetadata {
    pub sender: Option<SuiAddress>,
    pub gas_price: Option<UInt53>,
    pub gas_objects: Option<Vec<ObjectRef>>,
    pub gas_budget: Option<UInt53>,
    pub gas_sponsor: Option<SuiAddress>,
}
