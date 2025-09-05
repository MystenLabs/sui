// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_graphql::{Context, Object, Result};
use fastcrypto::error::FastCryptoError;

use sui_indexer_alt_reader::fullnode_client::{Error::GrpcExecutionError, FullnodeClient};
use sui_types::crypto::ToFromBytes;
use sui_types::signature::GenericSignature;
use sui_types::transaction::TransactionData;

use crate::api::scalars::base64::Base64;
use crate::{
    api::types::{execution_result::ExecutionResult, transaction_effects::TransactionEffects},
    error::{bad_user_input, RpcError},
    scope::Scope,
};

/// Error type for user input validation in executeTransaction
#[derive(thiserror::Error, Debug)]
enum TransactionInputError {
    #[error("Invalid BCS encoding in transaction data: {0}")]
    InvalidTransactionBcs(bcs::Error),

    #[error("Invalid signature format in signature {index}: {err}")]
    InvalidSignatureFormat { index: usize, err: FastCryptoError },
}

pub struct Mutation;

/// Mutations are used to write to the Sui network.
#[Object]
impl Mutation {
    /// Execute a transaction, committing its effects on chain.
    ///
    /// - `transactionDataBcs` contains the BCS-encoded transaction data (Base64-encoded).
    /// - `signatures` are a list of `flag || signature || pubkey` bytes, Base64-encoded.
    ///
    /// Waits until the transaction has reached finality on chain to return its transaction digest, or returns the error that prevented finality if that was not possible. A transaction is final when its effects are guaranteed on chain (it cannot be revoked).
    ///
    /// There may be a delay between transaction finality and when GraphQL requests (including the request that issued the transaction) reflect its effects. As a result, queries that depend on indexing the state of the chain (e.g. contents of output objects, address-level balance information at the time of the transaction), must wait for indexing to catch up by polling for the transaction digest using `Query.transaction`.
    async fn execute_transaction(
        &self,
        ctx: &Context<'_>,
        transaction_data_bcs: Base64,
        signatures: Vec<Base64>,
    ) -> Result<ExecutionResult, RpcError<TransactionInputError>> {
        // Get the gRPC client from context
        let fullnode_client: &FullnodeClient = ctx.data()?;

        // Parse transaction data from BCS
        let tx_data: TransactionData = {
            let bytes: &Vec<u8> = &transaction_data_bcs.0;
            bcs::from_bytes(bytes)
                .map_err(|err| bad_user_input(TransactionInputError::InvalidTransactionBcs(err)))?
        };

        // Parse signatures from raw bytes
        let mut parsed_signatures = Vec::new();
        for (index, sig_base64) in signatures.iter().enumerate() {
            let sig_bytes: &Vec<u8> = &sig_base64.0;
            let signature: GenericSignature =
                GenericSignature::from_bytes(sig_bytes).map_err(|err| {
                    bad_user_input(TransactionInputError::InvalidSignatureFormat { index, err })
                })?;
            parsed_signatures.push(signature);
        }

        // Execute transaction - capture gRPC errors for ExecutionResult.errors
        match fullnode_client
            .execute_transaction(tx_data.clone(), parsed_signatures.clone())
            .await
        {
            Ok(response) => {
                let scope = Scope::new(ctx)?.with_execution_output();
                let effects = TransactionEffects::from_execution_response(
                    scope,
                    response,
                    tx_data,
                    parsed_signatures,
                );

                Ok(ExecutionResult {
                    effects: Some(effects),
                    errors: None,
                })
            }
            Err(GrpcExecutionError(status)) => Ok(ExecutionResult {
                effects: None,
                errors: Some(vec![status.to_string()]),
            }),
            Err(other_error) => Err(anyhow!(other_error)
                .context("Failed to execute transaction")
                .into()),
        }
    }
}
