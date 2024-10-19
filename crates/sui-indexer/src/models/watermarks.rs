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
    pub pipeline: String,
    /// Inclusive upper epoch bound for this entity's data. Committer updates this field. Pruner uses
    /// this to determine if pruning is necessary based on the retention policy.
    pub epoch_hi_inclusive: i64,
    /// Inclusive upper checkpoint bound for this entity's data. Committer updates this field. All
    /// data of this entity in the checkpoint must be persisted before advancing this watermark. The
    /// committer refers to this on disaster recovery to resume writing.
    pub checkpoint_hi_inclusive: i64,
    /// Exclusive upper transaction sequence number bound for this entity's data. Committer updates
    /// this field.
    pub tx_hi: i64,
    /// Inclusive lower epoch bound for this entity's data. Pruner updates this field when the epoch range exceeds the retention policy.
    pub epoch_lo: i64,
    /// Inclusive low watermark that the pruner advances. Corresponds to the epoch id, checkpoint
    /// sequence number, or tx sequence number depending on the entity. Data before this watermark is
    /// considered pruned by a reader. The underlying data may still exist in the db instance.
    pub reader_lo: i64,
    /// Updated using the database's current timestamp when the pruner sees that some data needs to
    /// be dropped. The pruner uses this column to determine whether to prune or wait long enough
    /// that all in-flight reads complete or timeout before it acts on an updated watermark.
    pub timestamp_ms: i64,
    /// Column used by the pruner to track its true progress. Data below this watermark can be
    /// immediately pruned.
    pub pruner_hi: i64,
}

impl StoredWatermark {
    pub fn from_upper_bound_update(entity: &str, watermark: CommitterWatermark) -> Self {
        StoredWatermark {
            pipeline: entity.to_string(),
            epoch_hi_inclusive: watermark.epoch_hi_inclusive as i64,
            checkpoint_hi_inclusive: watermark.checkpoint_hi_inclusive as i64,
            tx_hi: watermark.tx_hi as i64,
            ..StoredWatermark::default()
        }
    }

    pub fn from_lower_bound_update(entity: &str, epoch_lo: u64, reader_lo: u64) -> Self {
        StoredWatermark {
            pipeline: entity.to_string(),
            epoch_lo: epoch_lo as i64,
            reader_lo: reader_lo as i64,
            ..StoredWatermark::default()
        }
    }

    pub fn entity(&self) -> Option<PrunableTable> {
        PrunableTable::from_str(&self.pipeline).ok()
    }

    /// Determine whether to set a new epoch lower bound based on the retention policy.
    pub fn new_epoch_lo(&self, retention: u64) -> Option<u64> {
        if self.epoch_lo as u64 + retention <= self.epoch_hi_inclusive as u64 {
            Some((self.epoch_hi_inclusive as u64).saturating_sub(retention - 1))
        } else {
            None
        }
    }
}
