// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use async_graphql::{Context, Object, Result};
use fastcrypto::encoding::{Base64, Encoding};
use sui_indexer_alt_reader::full_node_client::{Error::GrpcExecutionError, FullNodeClient};
use sui_indexer_alt_reader::kv_loader::TransactionContents as NativeTransactionContents;

use sui_types::crypto::EncodeDecodeBase64;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::signature::GenericSignature;
use sui_types::transaction::TransactionData;

use crate::{
    api::types::{
        execution_result::ExecutionResult, 
        transaction_execution_input::TransactionExecutionInput,
        transaction_effects::{EffectsContents, TransactionEffects},
    },
    error::RpcError,
    scope::Scope,
};

pub struct Mutation;

/// Mutations are used to write to the Sui network.
#[Object]
impl Mutation {
    /// Execute a transaction, committing its effects on chain.
    ///
    /// - `transaction` contains the transaction data in the desired format.
    /// - `signatures` are a list of `flag || signature || pubkey` bytes, Base64-encoded.
    ///
    /// Waits until the transaction has reached finality on chain to return its transaction digest, or returns the error that prevented finality if that was not possible. A transaction is final when its effects are guaranteed on chain (it cannot be revoked).
    ///
    /// There may be a delay between transaction finality and when GraphQL requests (including the request that issued the transaction) reflect its effects. As a result, queries that depend on indexing the state of the chain (e.g. contents of output objects, address-level balance information at the time of the transaction), must wait for indexing to catch up by polling for the transaction digest using `Query.transaction`.
    async fn execute_transaction(
        &self,
        ctx: &Context<'_>,
        transaction: TransactionExecutionInput,
        signatures: Vec<String>,
    ) -> Result<ExecutionResult, RpcError> {
        // Get the gRPC client from context
        let full_node_client: &FullNodeClient = ctx.data()?;

        // Parse transaction data from Base64 BCS
        let tx_data: TransactionData = {
            let bytes: Vec<u8> = Base64::decode(&transaction.transaction_data_bcs)
                .context("Invalid Base64 encoding in transaction data")?;

            bcs::from_bytes(&bytes).context("Invalid BCS encoding in transaction data")?
        };

        // Parse signatures from Base64 encoded raw signature bytes
        let mut parsed_signatures = Vec::new();
        for (i, sig_str) in signatures.iter().enumerate() {
            let signature = GenericSignature::decode_base64(sig_str)
                .with_context(|| format!("Invalid signature bytes for signature {i}"))?;

            parsed_signatures.push(signature);
        }

        // Execute transaction - capture gRPC errors for ExecutionResult.errors
        match full_node_client
            .execute_transaction(tx_data.clone(), parsed_signatures.clone())
            .await
        {
            Ok(response) => {
                let scope = Scope::new(ctx)?;
                let transaction_digest = response.effects.transaction_digest();
                
                // Create TransactionEffects with fresh ExecutedTransaction data
                let effects = TransactionEffects {
                    digest: *transaction_digest,
                    contents: EffectsContents {
                        scope,
                        contents: Some(std::sync::Arc::new(
                            NativeTransactionContents::ExecutedTransaction {
                                effects: response.effects,
                                events: response.events.map(|events| events.data),
                                transaction_data: tx_data,
                                signatures: parsed_signatures,
                            }
                        )),
                    },
                };
                
                Ok(ExecutionResult {
                    effects: Some(effects),
                    errors: None,
                })
            }
            Err(GrpcExecutionError(status)) => {
                Ok(ExecutionResult {
                    effects: None,
                    errors: Some(vec![status.to_string()]),
                })
            }
            Err(other_error) => {
                Err(anyhow::anyhow!("Failed to execute transaction: {}", other_error).into())
            }
        }
    }
}
