// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    handlers::CommitterWatermark,
    schema::watermarks::{self},
};
use diesel::prelude::*;

/// Represents a row in the `watermarks` table.
#[derive(Queryable, Insertable, Default, QueryableByName)]
#[diesel(table_name = watermarks, primary_key(entity))]
pub struct StoredWatermark {
    /// The table governed by this watermark, i.e `epochs`, `checkpoints`, `transactions`.
    pub entity: String,
    /// Inclusive upper epoch bound for this entity's data. Committer updates this field. Pruner uses
    /// this to determine if pruning is necessary based on the retention policy.
    pub epoch_hi: i64,
    /// Inclusive lower epoch bound for this entity's data. Pruner updates this field when the epoch range exceeds the retention policy.
    pub epoch_lo: i64,
    /// Inclusive upper checkpoint bound for this entity's data. Committer updates this field. All
    /// data of this entity in the checkpoint must be persisted before advancing this watermark. The
    /// committer refers to this on disaster recovery to resume writing.
    pub checkpoint_hi: i64,
    /// Inclusive upper transaction sequence number bound for this entity's data. Committer updates
    /// this field.
    pub tx_hi: i64,
    /// Inclusive low watermark that the pruner advances. Corresponds to the epoch id, checkpoint
    /// sequence number, or tx sequence number depending on the entity. Data before this watermark is
    /// considered pruned by a reader. The underlying data may still exist in the db instance.
    pub reader_lo: i64,
    /// Updated using the database's current timestamp when the pruner sees that some data needs to
    /// be dropped. The pruner uses this column to determine whether to prune or wait long enough
    /// that all in-flight reads complete or timeout before it acts on an updated watermark.
    pub timestamp_ms: i64,
    /// Column used by the pruner to track its true progress. Data at and below this watermark has
    /// been truly pruned from the db, and should no longer exist. When recovering from a crash, the
    /// pruner will consult this column to determine where to continue.
    pub pruned_lo: Option<i64>,
}

impl StoredWatermark {
    pub fn from_upper_bound_update(entity: &str, watermark: CommitterWatermark) -> Self {
        StoredWatermark {
            entity: entity.to_string(),
            epoch_hi: watermark.epoch as i64,
            checkpoint_hi: watermark.cp as i64,
            tx_hi: watermark.tx as i64,
            ..StoredWatermark::default()
        }
    }
}
