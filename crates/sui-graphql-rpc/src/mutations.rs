// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::error::code;
use crate::types::date_time::DateTime;
use crate::types::digest::Digest;
use crate::types::transaction_block_effects::TransactionBlockEffects;
use crate::{error::graphql_error, types::transaction_exec::ExecutionResult};
use async_graphql::*;
use fastcrypto::{encoding::Base64, traits::ToFromBytes};
use shared_crypto::intent::Intent;
use sui_json_rpc_types::{SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponseOptions};
use sui_sdk::SuiClient;
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::{signature::GenericSignature, transaction::Transaction};

pub struct Mutation;

#[Object]
impl Mutation {
    async fn execute_transaction_block(
        &self,
        ctx: &Context<'_>,
        tx_bytes: String,
        signatures: Vec<String>,
    ) -> Result<ExecutionResult> {
        // Get the list of fullnode urls from config
        let sui_sdk_client: &SuiClient = ctx.data().map_err(|_| {
            graphql_error(
                code::INTERNAL_SERVER_ERROR,
                "Unable to fetch Sui SDK client",
            )
        })?;

        let tx_data = bcs::from_bytes(
            &Base64::try_from(tx_bytes)
                .map_err(|e| {
                    graphql_error(
                        code::INTERNAL_SERVER_ERROR,
                        format!("Unable to deserialize transaction to Base64: {:?}", e),
                    )
                })?
                .to_vec()
                .map_err(|e| {
                    graphql_error(
                        code::INTERNAL_SERVER_ERROR,
                        format!("Unable to decode Base64 to vec: {:?}", e),
                    )
                })?,
        )?;

        let mut sigs = Vec::new();
        for sig in signatures {
            sigs.push(
                GenericSignature::from_bytes(
                    &Base64::try_from(sig)
                        .map_err(|e| {
                            graphql_error(
                                code::INTERNAL_SERVER_ERROR,
                                format!("Unable to deserialize signature to Base64: {:?}", e),
                            )
                        })?
                        .to_vec()
                        .map_err(|e| {
                            graphql_error(
                                code::INTERNAL_SERVER_ERROR,
                                format!("Unable to decode Base64 to vec: {:?}", e),
                            )
                        })?,
                )
                .map_err(|e| {
                    graphql_error(
                        code::INTERNAL_SERVER_ERROR,
                        format!("Unable to create generic signature from bytes: {:?}", e),
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
                SuiTransactionBlockResponseOptions::full_content(),
                Some(ExecuteTransactionRequestType::WaitForEffectsCert),
            )
            .await
            .map_err(|e| {
                graphql_error(
                    code::INTERNAL_SERVER_ERROR,
                    format!("Unable to execute transaction: {:?}", e),
                )
            })?;

        let timestamp = result
            .timestamp_ms
            .and_then(|t| DateTime::from_ms(t as i64));
        let balance_changes = result
            .balance_changes
            .ok_or(graphql_error(
                code::INTERNAL_SERVER_ERROR,
                "Balance changes not in transaction result",
            ))?
            .iter()
            .map(|b| {
                Some(bcs::to_bytes(b).map_err(|e| {
                    graphql_error(
                        code::INTERNAL_SERVER_ERROR,
                        format!("Unable to serialize balance change: {:?}", e),
                    )
                }))
                .transpose()
            })
            .collect::<Result<Vec<Option<Vec<u8>>>, ServerError>>()?;
        let object_changes = result
            .object_changes
            .ok_or(graphql_error(
                code::INTERNAL_SERVER_ERROR,
                "Object changes not in transaction result",
            ))?
            .iter()
            .map(|b| {
                Some(bcs::to_bytes(b).map_err(|e| {
                    graphql_error(
                        code::INTERNAL_SERVER_ERROR,
                        format!("Unable to serialize object change: {:?}", e),
                    )
                }))
                .transpose()
            })
            .collect::<Result<Vec<Option<Vec<u8>>>, ServerError>>()?;
        let tx_effects = result.effects.ok_or(graphql_error(
            code::INTERNAL_SERVER_ERROR,
            "Effects not in transaction result",
        ))?;
        let tx_block_digest = Digest::try_from(tx_effects.transaction_digest().inner().as_slice())?;
        let errors = result.errors;

        let effects = TransactionBlockEffects::from_stored_transaction(
            balance_changes,
            None,
            object_changes,
            &tx_effects,
            tx_block_digest,
            timestamp,
        )?;
        Ok(ExecutionResult {
            effects: Some(effects),
            errors,
        })
    }
}
