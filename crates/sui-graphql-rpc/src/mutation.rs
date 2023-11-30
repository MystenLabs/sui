// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::code;

use crate::{error::graphql_error, types::transaction_exec::ExecutionResult};
use async_graphql::*;
use fastcrypto::encoding::Encoding;
use fastcrypto::{encoding::Base64, traits::ToFromBytes};
use shared_crypto::intent::Intent;
use sui_json_rpc_types::SuiTransactionBlockResponseOptions;
use sui_sdk::SuiClient;
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::{signature::GenericSignature, transaction::Transaction};

pub struct Mutation;

#[Object]
impl Mutation {
    /// Execute a transaction, committing its effects on chain.
    ///
    /// `txBytes` is a `TransactionData` struct that has been BCS-encoded
    ///     and then Base64-encoded.
    /// `signatures` are a list of `flag || signature || pubkey` bytes,
    ///     Base64-encoded.
    ///
    /// Waits until the transaction has been finalised on chain to return
    /// its transaction digest.  If the transaction could not be
    /// finalised, returns the errors that prevented it, instead.
    async fn execute_transaction_block(
        &self,
        ctx: &Context<'_>,
        tx_bytes: String,
        signatures: Vec<String>,
    ) -> Result<ExecutionResult> {
        let sui_sdk_client: &SuiClient = ctx.data().map_err(|_| {
            graphql_error(
                code::INTERNAL_SERVER_ERROR,
                "Unable to fetch Sui SDK client",
            )
        })?;

        let tx_data = bcs::from_bytes(&Base64::decode(&tx_bytes).map_err(|e| {
            graphql_error(
                code::INTERNAL_SERVER_ERROR,
                format!("Unable to deserialize transaction bytes from Base64: {e}"),
            )
        })?)?;

        let mut sigs = Vec::new();
        for sig in signatures {
            sigs.push(
                GenericSignature::from_bytes(&Base64::decode(&sig).map_err(|e| {
                    graphql_error(
                        code::INTERNAL_SERVER_ERROR,
                        format!("Unable to deserialize signature bytes {sig} from Base64: {e}"),
                    )
                })?)
                .map_err(|e| {
                    graphql_error(
                        code::INTERNAL_SERVER_ERROR,
                        format!("Unable to create signature from bytes: {e}"),
                    )
                })?,
            );
        }
        let transaction =
            Transaction::from_generic_sig_data(tx_data, Intent::sui_transaction(), sigs);

        let result = sui_sdk_client
            .quorum_driver_api()
            .execute_transaction_block(
                transaction,
                SuiTransactionBlockResponseOptions::default(),
                Some(ExecuteTransactionRequestType::WaitForEffectsCert),
            )
            .await
            .map_err(|e| {
                graphql_error(
                    code::INTERNAL_SERVER_ERROR,
                    format!("Unable to execute transaction: {:?}", e),
                )
            })?;

        Ok(ExecutionResult {
            errors: if result.errors.is_empty() {
                None
            } else {
                Some(result.errors)
            },
            digest: result.digest.to_string(),
        })
    }
}
