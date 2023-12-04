// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::base64::Base64;
use super::big_int::BigInt;
use super::event::Event;
use super::move_type::MoveType;
use super::sui_address::SuiAddress;
use super::transaction_block::TransactionBlock;

use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
#[graphql(complex)]
pub(crate) struct DryRunResult {
    pub errors: Option<Vec<String>>,

    pub results: Option<Vec<DryRunEffect>>,
}

#[ComplexObject]
impl DryRunResult {
    pub async fn transaction(&self) -> Option<TransactionBlock> {
        // TODO: implement
        unimplemented!()
    }

    pub async fn events(&self) -> Option<Vec<Event>> {
        // TODO: implement
        unimplemented!()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct DryRunEffect {
    /// Changes made to arguments that were mutably borrowed by this transaction
    pub mutated_references: Option<Vec<DryRunMutation>>,

    /// Results of this transaction
    pub return_values: Option<Vec<DryRunReturn>>,
}

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct DryRunMutation {
    // TODO: not yet impl
    // pub input: TransactionInput,

    // TODO: rename to `type`
    pub type_: MoveType,

    pub bcs: Base64,
}

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct DryRunReturn {
    // TODO: rename to `type`
    pub type_: MoveType,

    pub bcs: Base64,
}
