// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::SimpleObject;

use super::programmable::ProgrammableTransaction;

/// ProgrammableSystemTransaction is identical to ProgrammableTransaction, but GraphQL does not allow multiple variants with the same type.
#[derive(SimpleObject, Clone)]
pub struct ProgrammableSystemTransaction {
    #[graphql(flatten)]
    pub(crate) inner: ProgrammableTransaction,
}
