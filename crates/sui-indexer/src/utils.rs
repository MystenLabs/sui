// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_json_rpc_types::SuiTransactionResponseOptions;
use sui_sdk::apis::ReadApi as SuiReadApi;
use sui_types::base_types::TransactionDigest;

use crate::errors::IndexerError;
use crate::types::SuiTransactionFullResponse;

pub async fn multi_get_full_transactions(
    read_api: &SuiReadApi,
    digests: Vec<TransactionDigest>,
) -> Result<Vec<SuiTransactionFullResponse>, IndexerError> {
    let sui_transactions = read_api
        .multi_get_transactions_with_options(
            digests.clone(),
            // MUSTFIX(gegaowp): avoid double fetching both input and raw_input
            SuiTransactionResponseOptions::new()
                .with_input()
                .with_effects()
                .with_events()
                .with_raw_input(),
        )
        .await
        .map_err(|e| {
            IndexerError::FullNodeReadingError(format!(
                "Failed to get transactions {:?} with error: {:?}",
                digests.clone(),
                e
            ))
        })?;
    let sui_full_transactions: Vec<SuiTransactionFullResponse> = sui_transactions
        .into_iter()
        .map(SuiTransactionFullResponse::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            IndexerError::FullNodeReadingError(format!(
                "Unexpected None value in SuiTransactionFullResponse of digests {:?} with error {:?}",
                digests, e
            ))
        })?;
    Ok(sui_full_transactions)
}
