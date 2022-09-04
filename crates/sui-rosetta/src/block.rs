use std::sync::Arc;

use axum::{Extension, Json};

use crate::actions::SuiAction;
use crate::types::{
    BlockRequest, BlockResponse, BlockTransactionRequest, BlockTransactionResponse, Operation,
    OperationStatus, Transaction, TransactionIdentifier,
};
use crate::{Error, ServerContext};

pub async fn block(
    Json(payload): Json<BlockRequest>,
    Extension(state): Extension<Arc<ServerContext>>,
) -> Result<BlockResponse, Error> {
    state.checks_network_identifier(&payload.network_identifier)?;
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
    Extension(context): Extension<Arc<ServerContext>>,
) -> Result<BlockTransactionResponse, Error> {
    context.checks_network_identifier(&payload.network_identifier)?;
    let digest = payload.transaction_identifier.hash;
    let (cert, effect) = context.state.get_transaction(digest).await?;
    let hash = *cert.digest();
    let data = cert.signed_data.data;
    let actions = SuiAction::try_from_data(&data)?;
    let mut operations = Operation::from_actions(actions);

    let status = OperationStatus::from(effect.status).to_string();

    for mut operation in &mut operations {
        operation.status = Some(status.clone())
    }

    let transaction = Transaction {
        transaction_identifier: TransactionIdentifier { hash },
        operations,
        related_transactions: vec![],
        metadata: None,
    };

    Ok(BlockTransactionResponse { transaction })
}
