// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

use super::TransactionArgument;

/// Transfers `inputs` to `address`. All inputs must have the `store` ability (allows public transfer) and must not be previously immutable or shared.
#[derive(SimpleObject, Clone)]
pub struct TransferObjectsCommand {
    /// The objects to transfer.
    pub inputs: Vec<TransactionArgument>,
    /// The address to transfer to.
    pub address: Option<TransactionArgument>,
}
