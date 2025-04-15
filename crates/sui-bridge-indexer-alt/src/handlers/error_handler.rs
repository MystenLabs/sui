use crate::handlers::is_bridge_txn;
use async_trait::async_trait;
use diesel_async::RunQueryDsl;
use std::sync::Arc;
use sui_bridge_schema::models::SuiErrorTransactions;
use sui_bridge_schema::schema::sui_error_transactions;
use sui_indexer_alt_framework::db::Db;
use sui_indexer_alt_framework::pipeline::concurrent::Handler;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_framework::types::effects::TransactionEffectsAPI;
use sui_indexer_alt_framework::types::execution_status::ExecutionStatus;
use sui_indexer_alt_framework::types::full_checkpoint_content::CheckpointData;

pub struct ErrorTransactionHandler;

impl Processor for ErrorTransactionHandler {
    const NAME: &'static str = "ErrorTransactions";
    type Value = SuiErrorTransactions;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> anyhow::Result<Vec<Self::Value>> {
        let timestamp_ms = checkpoint.checkpoint_summary.timestamp_ms as i64;
        checkpoint
            .transactions
            .iter()
            .try_fold(vec![], |mut results, tx| {
                if let ExecutionStatus::Failure { error, command } = tx.effects.status() {
                    if is_bridge_txn(tx) {
                        results.push(SuiErrorTransactions {
                            txn_digest: tx.transaction.digest().inner().to_vec(),
                            timestamp_ms,
                            failure_status: error.to_string(),
                            cmd_idx: command.map(|idx| idx as i64),
                            sender_address: tx.transaction.sender_address().to_vec(),
                        })
                    }
                }
                Ok(results)
            })
    }
}

#[async_trait]
impl Handler for ErrorTransactionHandler {
    type Store = Db;

    async fn commit<'a>(
        values: &[Self::Value],
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> anyhow::Result<usize> {
        Ok(diesel::insert_into(sui_error_transactions::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
