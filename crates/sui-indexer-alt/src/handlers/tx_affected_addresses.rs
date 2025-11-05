// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;
use std::sync::Arc;

use anyhow::Result;
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::RunQueryDsl;
use itertools::Itertools;
use sui_indexer_alt_framework::{
    pipeline::Processor,
    postgres::{Connection, handler::Handler},
    types::{full_checkpoint_content::Checkpoint, object::Owner},
};
use sui_indexer_alt_schema::{
    schema::tx_affected_addresses, transactions::StoredTxAffectedAddress,
};
use sui_types::transaction::TransactionDataAPI;

use crate::handlers::cp_sequence_numbers::tx_interval;
use async_trait::async_trait;

pub(crate) struct TxAffectedAddresses;

#[async_trait]
impl Processor for TxAffectedAddresses {
    const NAME: &'static str = "tx_affected_addresses";

    type Value = StoredTxAffectedAddress;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let Checkpoint {
            transactions,
            summary,
            ..
        } = checkpoint.as_ref();

        let mut values = Vec::new();
        let first_tx = summary.network_total_transactions as usize - transactions.len();

        for (i, tx) in transactions.iter().enumerate() {
            let tx_sequence_number = (first_tx + i) as i64;
            let sender = tx.transaction.sender();
            let payer = tx.transaction.gas_data().owner;
            let recipients = tx.effects.all_changed_objects().into_iter().filter_map(
                |(_object_ref, owner, _write_kind)| match owner {
                    Owner::AddressOwner(address) => Some(address),
                    _ => None,
                },
            );

            let affected_addresses: Vec<StoredTxAffectedAddress> = recipients
                .chain(vec![sender, payer])
                .unique()
                .map(|a| StoredTxAffectedAddress {
                    tx_sequence_number,
                    affected: a.to_vec(),
                    sender: sender.to_vec(),
                })
                .collect();
            values.extend(affected_addresses);
        }

        Ok(values)
    }
}

#[async_trait]
impl Handler for TxAffectedAddresses {
    const MIN_EAGER_ROWS: usize = 100;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit<'a>(values: &[Self::Value], conn: &mut Connection<'a>) -> Result<usize> {
        Ok(diesel::insert_into(tx_affected_addresses::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }

    async fn prune<'a>(
        &self,
        from: u64,
        to_exclusive: u64,
        conn: &mut Connection<'a>,
    ) -> Result<usize> {
        let Range {
            start: from_tx,
            end: to_tx,
        } = tx_interval(conn, from..to_exclusive).await?;
        let filter = tx_affected_addresses::table.filter(
            tx_affected_addresses::tx_sequence_number.between(from_tx as i64, to_tx as i64 - 1),
        );

        Ok(diesel::delete(filter).execute(conn).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel_async::RunQueryDsl;
    use sui_indexer_alt_framework::{
        Indexer, types::test_checkpoint_data_builder::TestCheckpointBuilder,
    };
    use sui_indexer_alt_schema::MIGRATIONS;

    use crate::handlers::cp_sequence_numbers::CpSequenceNumbers;

    async fn get_all_tx_affected_addresses(conn: &mut Connection<'_>) -> Result<Vec<i64>> {
        Ok(tx_affected_addresses::table
            .select(tx_affected_addresses::tx_sequence_number)
            .order_by(tx_affected_addresses::tx_sequence_number)
            .load(conn)
            .await?)
    }

    #[tokio::test]
    async fn test_tx_affected_addresses_pruning_complains_if_no_mapping() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        let result = TxAffectedAddresses.prune(0, 2, &mut conn).await;

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "No checkpoint mapping found for checkpoint 0"
        );
    }

    #[tokio::test]
    async fn test_tx_affected_addresses_pruning() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        // 0th checkpoint has 1 transaction
        let mut builder = TestCheckpointBuilder::new(0);
        builder = builder.start_transaction(0).finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = TxAffectedAddresses.process(&checkpoint).await.unwrap();
        TxAffectedAddresses::commit(&values, &mut conn)
            .await
            .unwrap();
        let values = CpSequenceNumbers.process(&checkpoint).await.unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        // 1st checkpoint has 2 transactions
        builder = builder.start_transaction(0).finish_transaction();
        builder = builder.start_transaction(1).finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = TxAffectedAddresses.process(&checkpoint).await.unwrap();
        TxAffectedAddresses::commit(&values, &mut conn)
            .await
            .unwrap();
        let values = CpSequenceNumbers.process(&checkpoint).await.unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        // 2nd checkpoint has 4 transactions
        builder = builder.start_transaction(0).finish_transaction();
        builder = builder.start_transaction(1).finish_transaction();
        builder = builder.start_transaction(2).finish_transaction();
        builder = builder.start_transaction(3).finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = TxAffectedAddresses.process(&checkpoint).await.unwrap();
        TxAffectedAddresses::commit(&values, &mut conn)
            .await
            .unwrap();
        let values = CpSequenceNumbers.process(&checkpoint).await.unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        let fetched_results = get_all_tx_affected_addresses(&mut conn).await.unwrap();
        assert_eq!(fetched_results.len(), 7);

        // Prune checkpoints from `[0, 2)`, expect 4 transactions remaining
        let rows_pruned = TxAffectedAddresses.prune(0, 2, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 3);
        let remaining_tx_affected_addresses =
            get_all_tx_affected_addresses(&mut conn).await.unwrap();
        assert_eq!(remaining_tx_affected_addresses.len(), 4);
        assert_eq!(remaining_tx_affected_addresses, vec![3, 4, 5, 6]);
    }
}
