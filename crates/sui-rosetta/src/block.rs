use std::sync::Arc;

use axum::{Extension, Json};

use crate::actions::SuiAction;
use crate::types::{
    BlockRequest, BlockResponse, BlockTransactionRequest, BlockTransactionResponse, Operation,
    Transaction, TransactionIdentifier,
};
use crate::{ApiState, Error};

pub async fn block(
    Json(payload): Json<BlockRequest>,
    Extension(state): Extension<Arc<ApiState>>,
) -> Result<BlockResponse, Error> {
    state.checks_network_identifier(&payload.network_identifier)?;
    let blocks = state.blocks(payload.network_identifier.network)?;

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
    Extension(state): Extension<Arc<ApiState>>,
) -> Result<BlockTransactionResponse, Error> {
    state.checks_network_identifier(&payload.network_identifier)?;
    let digest = payload.transaction_identifier.hash;
    let transaction = state
        .get_client(payload.network_identifier.network)
        .await?
        .read_api()
        .get_transaction(digest)
        .await?;

    let data = transaction.certificate.data;
    let actions = SuiAction::try_from_data(&data)?;
    let transaction = Transaction {
        transaction_identifier: TransactionIdentifier {
            hash: transaction.certificate.transaction_digest,
        },
        operations: Operation::from_actions(actions),
        related_transactions: vec![],
        metadata: None,
    };

    Ok(BlockTransactionResponse { transaction })
}
