// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use diesel::prelude::*;

use crate::{
    handlers::{pruner::PrunableTable, CommitterWatermark},
    schema::watermarks::{self},
};

/// Represents a row in the `watermarks` table.
#[derive(Queryable, Insertable, Default, QueryableByName, Clone)]
#[diesel(table_name = watermarks, primary_key(entity))]
pub struct StoredWatermark {
    /// The table governed by this watermark, i.e `epochs`, `checkpoints`, `transactions`.
    pub entity: String,
    /// Inclusive upper epoch bound for this entity's data. Committer updates this field. Pruner uses
    /// this to determine if pruning is necessary based on the retention policy.
    pub epoch_hi_inclusive: i64,
    /// Inclusive lower epoch bound for this entity's data. Pruner updates this field when the epoch range exceeds the retention policy.
    pub epoch_lo: i64,
    /// Inclusive upper checkpoint bound for this entity's data. Committer updates this field. All
    /// data of this entity in the checkpoint must be persisted before advancing this watermark. The
    /// committer refers to this on disaster recovery to resume writing.
    pub checkpoint_hi_inclusive: i64,
    /// Inclusive upper transaction sequence number bound for this entity's data. Committer updates
    /// this field.
    pub tx_hi_inclusive: i64,
    /// Inclusive low watermark that the pruner advances. Corresponds to the epoch id, checkpoint
    /// sequence number, or tx sequence number depending on the entity. Data before this watermark is
    /// considered pruned by a reader. The underlying data may still exist in the db instance.
    pub reader_lo: i64,
    /// Updated using the database's current timestamp when the pruner sees that some data needs to
    /// be dropped. The pruner uses this column to determine whether to prune or wait long enough
    /// that all in-flight reads complete or timeout before it acts on an updated watermark.
    pub timestamp_ms: i64,
    /// Updated and used by the pruner. Data up to and excluding this watermark can be immediately
    /// dropped. Data between this and `reader_lo` can be pruned after a delay.
    pub pruner_lo: Option<i64>,
}

#[derive(Debug)]
pub struct PrunableWatermark {
    pub entity: PrunableTable,
    pub epoch_hi_inclusive: u64,
    pub epoch_lo: u64,
    pub checkpoint_hi_inclusive: u64,
    pub tx_hi_inclusive: u64,
    pub reader_lo: u64,
    /// Timestamp when the watermark's lower bound was last updated.
    pub timestamp_ms: i64,
    /// Latest timestamp read from db.
    pub current_timestamp_ms: i64,
    /// Data at and below `pruned_lo` is considered pruned by the pruner.
    pub pruner_lo: Option<u64>,
}

impl PrunableWatermark {
    pub fn new(stored: StoredWatermark, latest_db_timestamp: i64) -> Option<Self> {
        let entity = PrunableTable::from_str(&stored.entity).ok()?;

        Some(PrunableWatermark {
            entity,
            epoch_hi_inclusive: stored.epoch_hi_inclusive as u64,
            epoch_lo: stored.epoch_lo as u64,
            checkpoint_hi_inclusive: stored.checkpoint_hi_inclusive as u64,
            tx_hi_inclusive: stored.tx_hi_inclusive as u64,
            reader_lo: stored.reader_lo as u64,
            timestamp_ms: stored.timestamp_ms,
            current_timestamp_ms: latest_db_timestamp,
            pruner_lo: stored.pruner_lo.map(|lo| lo as u64),
        })
    }

    pub fn update(&mut self, new_epoch_lo: u64, new_reader_lo: u64) {
        self.pruner_lo = Some(match self.entity {
            PrunableTable::ObjectsHistory => self.epoch_lo,
            PrunableTable::Transactions => self.epoch_lo,
            PrunableTable::Events => self.epoch_lo,
            _ => self.reader_lo,
        });

        self.epoch_lo = new_epoch_lo;
        self.reader_lo = new_reader_lo;
    }

    /// Represents the exclusive upper bound of data that can be pruned immediately.
    pub fn immediately_prunable_hi(&self) -> Option<u64> {
        self.pruner_lo
    }

    /// Represents the lower bound of data that can be pruned after a delay.
    pub fn delayed_prunable_lo(&self) -> Option<u64> {
        self.pruner_lo
    }

    /// The new `pruner_lo` is the current reader_lo, or epoch_lo for epoch-partitioned tables.
    pub fn new_pruner_lo(&self) -> u64 {
        self.entity.select_pruner_lo(self.epoch_lo, self.reader_lo)
    }
}

impl StoredWatermark {
    pub fn from_upper_bound_update(entity: &str, watermark: CommitterWatermark) -> Self {
        StoredWatermark {
            entity: entity.to_string(),
            epoch_hi_inclusive: watermark.epoch as i64,
            checkpoint_hi_inclusive: watermark.cp as i64,
            tx_hi_inclusive: watermark.tx as i64,
            ..StoredWatermark::default()
        }
    }

    pub fn from_lower_bound_update(
        entity: &str,
        epoch_lo: u64,
        reader_lo: u64,
        pruner_lo: u64,
    ) -> Self {
        StoredWatermark {
            entity: entity.to_string(),
            epoch_lo: epoch_lo as i64,
            reader_lo: reader_lo as i64,
            pruner_lo: Some(pruner_lo as i64),
            ..StoredWatermark::default()
        }
    }
}
