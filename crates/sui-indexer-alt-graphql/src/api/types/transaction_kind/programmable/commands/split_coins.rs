// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

use super::TransactionArgument;

/// Splits off coins with denominations in `amounts` from `coin`, returning multiple results (as many as there are amounts.)
#[derive(SimpleObject, Clone)]
pub struct SplitCoinsCommand {
    /// The coin to split.
    pub coin: Option<TransactionArgument>,
    /// The denominations to split off from the coin.
    pub amounts: Vec<TransactionArgument>,
}
