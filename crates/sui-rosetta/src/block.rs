// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use axum::{Extension, Json};
use tracing::debug;

use crate::operations::Operation;
use crate::types::{
    BlockRequest, BlockResponse, BlockTransactionRequest, BlockTransactionResponse, Transaction,
    TransactionIdentifier,
};
use crate::{Error, OnlineServerContext, SuiEnv};

/// This module implements the [Rosetta Block API](https://www.rosetta-api.org/docs/BlockApi.html)

/// Get a block by its Block Identifier.
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/BlockApi.html#block)
pub async fn block(
    Json(request): Json<BlockRequest>,
    Extension(state): Extension<Arc<OnlineServerContext>>,
    Extension(env): Extension<SuiEnv>,
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
/// [Rosetta API Spec](https://www.rosetta-api.org/docs/BlockApi.html#blocktransaction)
pub async fn transaction(
    Json(request): Json<BlockTransactionRequest>,
    Extension(context): Extension<Arc<OnlineServerContext>>,
    Extension(env): Extension<SuiEnv>,
) -> Result<BlockTransactionResponse, Error> {
    env.check_network_identifier(&request.network_identifier)?;
    let digest = request.transaction_identifier.hash;
    let (cert, effects) = context.state.get_transaction(digest).await?;
    let hash = *cert.digest();
    let data = &cert.data().data;

    let operations = Operation::from_data_and_events(data, &effects.status, &effects.events)?;

    let transaction = Transaction {
        transaction_identifier: TransactionIdentifier { hash },
        operations,
        related_transactions: vec![],
        metadata: None,
    };

    Ok(BlockTransactionResponse { transaction })
}
