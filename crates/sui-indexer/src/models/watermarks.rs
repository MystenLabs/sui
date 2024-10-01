// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::watermarks::{self};
use diesel::prelude::*;

/// Represents a row in the `watermarks` table.
#[derive(Queryable, Insertable, Default, QueryableByName)]
#[diesel(table_name = watermarks, primary_key(entity))]
pub struct StoredWatermark {
    /// The table governed by this watermark, i.e `epochs`, `checkpoints`, `transactions`.
    pub entity: String,
    /// Inclusive upper bound epoch this entity has data for. Committer updates this field. Pruner
    /// uses this field for per-entity epoch-level retention, and is mostly useful for pruning
    /// unpartitioned tables.
    pub epoch_hi: i64,
    /// Inclusive lower bound epoch this entity has data for. Pruner updates this field, and uses
    /// this field in tandem with `epoch_hi` for per-entity epoch-level retention. This is mostly
    /// useful for pruning unpartitioned tables.
    pub epoch_lo: i64,
    /// Inclusive upper bound checkpoint this entity has data for. Committer updates this field. All
    /// data of this entity in the checkpoint must be persisted before advancing this watermark. The
    /// committer or ingestion task refers to this on disaster recovery.
    pub checkpoint_hi: i64,
    /// Inclusive high watermark that the committer advances. For `checkpoints`, this represents the
    /// checkpoint sequence number, for `transactions`, the transaction sequence number, etc.
    pub reader_hi: i64,
    /// Inclusive low watermark that the pruner advances. Data before this watermark is considered
    /// pruned by a reader. The underlying data may still exist in the db instance.
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
    pub fn from_upper_bound_update(
        entity: &str,
        epoch_hi: u64,
        checkpoint_hi: u64,
        reader_hi: u64,
    ) -> Self {
        StoredWatermark {
            entity: entity.to_string(),
            epoch_hi: epoch_hi as i64,
            checkpoint_hi: checkpoint_hi as i64,
            reader_hi: reader_hi as i64,
            ..StoredWatermark::default()
        }
    }
}
