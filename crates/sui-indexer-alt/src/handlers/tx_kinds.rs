// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use diesel_async::RunQueryDsl;
use sui_types::full_checkpoint_content::CheckpointData;

use crate::{
    db,
    models::transactions::{StoredKind, StoredTxKind},
    pipeline::{concurrent::Handler, Processor},
    schema::tx_kinds,
};

pub struct TxKinds;

impl Processor for TxKinds {
    const NAME: &'static str = "tx_kinds";

    type Value = StoredTxKind;

    fn process(checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let CheckpointData {
            transactions,
            checkpoint_summary,
            ..
        } = checkpoint.as_ref();

        let mut values = Vec::new();
        let first_tx = checkpoint_summary.network_total_transactions as usize - transactions.len();

        for (i, tx) in transactions.iter().enumerate() {
            let tx_sequence_number = (first_tx + i) as i64;
            let tx_kind = if tx.transaction.is_system_tx() {
                StoredKind::SystemTransaction
            } else {
                StoredKind::ProgrammableTransaction
            };

            values.push(StoredTxKind {
                tx_sequence_number,
                tx_kind,
            });
        }

        Ok(values)
    }
}

#[async_trait::async_trait]
impl Handler for TxKinds {
    const MIN_EAGER_ROWS: usize = 100;
    const MAX_CHUNK_ROWS: usize = 1000;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        Ok(diesel::insert_into(tx_kinds::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
