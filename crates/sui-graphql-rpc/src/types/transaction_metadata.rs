// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::big_int::BigInt;
use super::object::ObjectRef;
use super::sui_address::SuiAddress;
use async_graphql::*;

/// The optional extra data a user can provide to a transaction dry run.
#[derive(Clone, Debug, PartialEq, Eq, InputObject)]
pub(crate) struct TransactionMetadata {
    pub sender: Option<SuiAddress>,
    pub gas_price: Option<BigInt>,
    pub gas_objects: Option<Vec<ObjectRef>>,
    pub gas_budget: Option<BigInt>,
    pub gas_sponsor: Option<SuiAddress>,
}
