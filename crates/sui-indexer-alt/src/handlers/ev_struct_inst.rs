// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, ops::Range, sync::Arc};

use anyhow::{Context, Result};
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    models::cp_sequence_numbers::tx_interval,
    pipeline::{concurrent::Handler, Processor},
};
use sui_indexer_alt_schema::{events::StoredEvStructInst, schema::ev_struct_inst};
use sui_pg_db as db;
use sui_types::full_checkpoint_content::CheckpointData;

pub(crate) struct EvStructInst;

impl Processor for EvStructInst {
    const NAME: &'static str = "ev_struct_inst";

    type Value = StoredEvStructInst;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let CheckpointData {
            transactions,
            checkpoint_summary,
            ..
        } = checkpoint.as_ref();

        let mut values = BTreeSet::new();
        let first_tx = checkpoint_summary.network_total_transactions as usize - transactions.len();

        for (i, tx) in transactions.iter().enumerate() {
            let tx_sequence_number = (first_tx + i) as i64;
            for (j, ev) in tx.events.iter().flat_map(|evs| evs.data.iter().enumerate()) {
                values.insert(StoredEvStructInst {
                    package: ev.type_.address.to_vec(),
                    module: ev.type_.module.to_string(),
                    name: ev.type_.name.to_string(),
                    instantiation: bcs::to_bytes(&ev.type_.type_params)
                        .with_context(|| format!(
                            "Failed to serialize type parameters for event ({tx_sequence_number}, {j})"
                        ))?,
                    tx_sequence_number: (first_tx + i) as i64,
                    sender: ev.sender.to_vec(),
                });
            }
        }

        Ok(values.into_iter().collect())
    }
}

#[async_trait::async_trait]
impl Handler for EvStructInst {
    const MIN_EAGER_ROWS: usize = 100;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        Ok(diesel::insert_into(ev_struct_inst::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }

    async fn prune(
        &self,
        from: u64,
        to_exclusive: u64,
        conn: &mut db::Connection<'_>,
    ) -> Result<usize> {
        let Range {
            start: from_tx,
            end: to_tx,
        } = tx_interval(conn, from..to_exclusive).await?;

        let filter = ev_struct_inst::table
            .filter(ev_struct_inst::tx_sequence_number.between(from_tx as i64, to_tx as i64 - 1));

        Ok(diesel::delete(filter).execute(conn).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel_async::RunQueryDsl;
    use sui_indexer_alt_framework::handlers::cp_sequence_numbers::CpSequenceNumbers;
    use sui_indexer_alt_framework::Indexer;
    use sui_indexer_alt_schema::MIGRATIONS;
    use sui_types::event::Event;
    use sui_types::test_checkpoint_data_builder::TestCheckpointDataBuilder;

    async fn get_all_ev_struct_inst(
        conn: &mut db::Connection<'_>,
    ) -> Result<Vec<StoredEvStructInst>> {
        let query = ev_struct_inst::table
            .order_by((
                ev_struct_inst::tx_sequence_number,
                ev_struct_inst::sender,
                ev_struct_inst::package,
                ev_struct_inst::module,
                ev_struct_inst::name,
                ev_struct_inst::instantiation,
            ))
            .load(conn)
            .await?;
        Ok(query)
    }

    #[tokio::test]
    async fn test_ev_struct_inst_pruning_complains_if_no_mapping() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.db().connect().await.unwrap();

        let result = EvStructInst.prune(0, 2, &mut conn).await;

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "No checkpoint mapping found for checkpoint 0"
        );
    }

    #[tokio::test]
    async fn test_ev_struct_inst_process_no_events() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.db().connect().await.unwrap();

        let checkpoint = Arc::new(
            TestCheckpointDataBuilder::new(0)
                .start_transaction(0)
                .finish_transaction()
                .build_checkpoint(),
        );

        let values = EvStructInst.process(&checkpoint).unwrap();
        EvStructInst::commit(&values, &mut conn).await.unwrap();

        assert_eq!(values.len(), 0);
    }

    #[tokio::test]
    async fn test_ev_struct_inst_process_single_event() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.db().connect().await.unwrap();

        let checkpoint = Arc::new(
            TestCheckpointDataBuilder::new(0)
                .start_transaction(0)
                .with_events(vec![Event::random_for_testing()])
                .finish_transaction()
                .build_checkpoint(),
        );

        // Process checkpoint with one event
        let values = EvStructInst.process(&checkpoint).unwrap();
        EvStructInst::commit(&values, &mut conn).await.unwrap();

        let events = get_all_ev_struct_inst(&mut conn).await.unwrap();
        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn test_ev_struct_inst_prune_events() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.db().connect().await.unwrap();

        // 0th checkpoint has no events
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder.start_transaction(0).finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = EvStructInst.process(&checkpoint).unwrap();
        EvStructInst::commit(&values, &mut conn).await.unwrap();
        let values = CpSequenceNumbers.process(&checkpoint).unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        // 1st checkpoint has 1 event
        builder = builder
            .start_transaction(0)
            .with_events(vec![Event::random_for_testing()])
            .finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = EvStructInst.process(&checkpoint).unwrap();
        EvStructInst::commit(&values, &mut conn).await.unwrap();
        let values = CpSequenceNumbers.process(&checkpoint).unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        // 2nd checkpoint has 2 events
        builder = builder
            .start_transaction(0)
            .with_events(vec![
                Event::random_for_testing(),
                Event::random_for_testing(),
            ])
            .finish_transaction();
        let checkpoint = Arc::new(builder.build_checkpoint());
        let values = EvStructInst.process(&checkpoint).unwrap();
        EvStructInst::commit(&values, &mut conn).await.unwrap();
        let values = CpSequenceNumbers.process(&checkpoint).unwrap();
        CpSequenceNumbers::commit(&values, &mut conn).await.unwrap();

        // Prune checkpoints from `[0, 2)`, expect 2 events remaining
        let rows_pruned = EvStructInst.prune(0, 2, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 1);

        let remaining_events = get_all_ev_struct_inst(&mut conn).await.unwrap();
        assert_eq!(remaining_events.len(), 2);
    }
}
