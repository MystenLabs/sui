// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::extract::State;
use axum::{Extension, Json};
use axum_extra::extract::WithRejection;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetTransactionRequest;
use tracing::debug;

use crate::operations::Operations;
use crate::types::{
    BlockRequest, BlockResponse, BlockTransactionRequest, BlockTransactionResponse, Transaction,
    TransactionIdentifier,
};
use crate::{Error, OnlineServerContext, SuiEnv};

// This module implements the [Mesh Block API](https://docs.cdp.coinbase.com/mesh/mesh-api-spec/api-reference#block)

/// Get a block by its Block Identifier.
/// [Mesh API Spec](https://docs.cdp.coinbase.com/api-reference/mesh/block/get-a-block)
pub async fn block(
    State(state): State<OnlineServerContext>,
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<BlockRequest>, Error>,
) -> Result<BlockResponse, Error> {
    debug!("Called /block endpoint: {:?}", request.block_identifier);
    env.check_network_identifier(&request.network_identifier)?;
    let blocks = state.blocks();
    if let Some(index) = request.block_identifier.index {
        blocks.get_block_by_index(index).await
    } else if let Some(hash) = request.block_identifier.hash {
        blocks.get_block_by_hash(hash).await
    } else {
        blocks.current_block().await
    }
}

/// Get a transaction in a block by its Transaction Identifier.
/// [Mesh API Spec](https://docs.cdp.coinbase.com/api-reference/mesh/block/get-a-block-transaction)
pub async fn transaction(
    State(context): State<OnlineServerContext>,
    Extension(env): Extension<SuiEnv>,
    WithRejection(Json(request), _): WithRejection<Json<BlockTransactionRequest>, Error>,
) -> Result<BlockTransactionResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;
    let digest = request.transaction_identifier.hash;

    let request = GetTransactionRequest::default()
        .with_digest(digest.to_string())
        .with_read_mask(FieldMask::from_paths([
            "digest",
            "transaction.sender",
            "transaction.gas_payment",
            "transaction.kind",
            "effects.gas_object",
            "effects.gas_used",
            "effects.status",
            "balance_changes",
            "events.events.event_type",
            "events.events.json",
            "events.events.contents",
        ]));

    let mut client = context.client.clone();
    let response = client
        .ledger_client()
        .get_transaction(request)
        .await?
        .into_inner();

    let operations = Operations::try_from_executed_transaction(
        response
            .transaction
            .ok_or_else(|| Error::DataError("Response missing transaction".to_string()))?,
        &context.coin_metadata_cache,
    )
    .await?;

    let transaction = Transaction {
        transaction_identifier: TransactionIdentifier { hash: digest },
        operations,
        related_transactions: vec![],
        metadata: None,
    };

    Ok(BlockTransactionResponse { transaction })
}
