// Copyright (c) 2022, Mysten Labs, Inc.
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

pub async fn block(
    Json(payload): Json<BlockRequest>,
    Extension(state): Extension<Arc<OnlineServerContext>>,
    Extension(env): Extension<SuiEnv>,
) -> Result<BlockResponse, Error> {
    debug!("Called /block endpoint: {:?}", payload.block_identifier);
    env.check_network_identifier(&payload.network_identifier)?;
    let blocks = state.blocks();
    if let Some(index) = payload.block_identifier.index {
        blocks.get_block_by_index(index).await
    } else if let Some(hash) = payload.block_identifier.hash {
        blocks.get_block_by_hash(hash).await
    } else {
        blocks.current_block().await
    }
}

pub async fn transaction(
    Json(payload): Json<BlockTransactionRequest>,
    Extension(context): Extension<Arc<OnlineServerContext>>,
    Extension(env): Extension<SuiEnv>,
) -> Result<BlockTransactionResponse, Error> {
    env.check_network_identifier(&payload.network_identifier)?;
    let digest = payload.transaction_identifier.hash;
    let (cert, effects) = context.state.get_transaction(digest).await?;
    let hash = *cert.digest();
    let data = cert.signed_data.data;
    let operations = Operation::from_data_and_effect(&data, &effects)?;

    let transaction = Transaction {
        transaction_identifier: TransactionIdentifier { hash },
        operations,
        related_transactions: vec![],
        metadata: None,
    };

    Ok(BlockTransactionResponse { transaction })
}
