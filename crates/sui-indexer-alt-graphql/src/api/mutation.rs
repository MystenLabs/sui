// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Context, Object, Result};

use crate::error::RpcError;

pub struct Mutation;

/// Mutations are used to write to the Sui network.
#[Object]
impl Mutation {
    /// Execute a transaction, committing its effects on chain.
    ///
    /// - `txBytes` is a `TransactionData` struct that has been BCS-encoded and then Base64-encoded.
    /// - `signatures` are a list of `flag || signature || pubkey` bytes, Base64-encoded.
    ///
    /// Waits until the transaction has reached finality on chain to return its transaction digest, or returns the error that prevented finality if that was not possible. A transaction is final when its effects are guaranteed on chain (it cannot be revoked).
    ///
    /// There may be a delay between transaction finality and when GraphQL requests (including the request that issued the transaction) reflect its effects. As a result, queries that depend on indexing the state of the chain (e.g. contents of output objects, address-level balance information at the time of the transaction), must wait for indexing to catch up by polling for the transaction digest using `Query.transaction`.
    async fn execute_transaction(
        &self,
        _ctx: &Context<'_>,
        _tx_bytes: String,
        _signatures: Vec<String>,
    ) -> Result<String, RpcError> {
        todo!("execute_transaction implementation")
    }
}
