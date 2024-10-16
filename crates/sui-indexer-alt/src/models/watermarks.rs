// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::watermarks;
use diesel::prelude::*;

/// Represents a row in the `watermarks` table.
#[derive(Queryable, Insertable, Default, QueryableByName)]
#[diesel(table_name = watermarks, primary_key(entity))]
pub struct StoredWatermark {
    /// The table governed by this watermark, i.e `epochs`, `checkpoints`, `transactions`.
    pub entity: String,
    /// Inclusive upper epoch bound for this entity's data. Committer updates this field. Pruner uses
    /// this to determine if pruning is necessary based on the retention policy.
    pub epoch_hi_inclusive: i64,
    /// Inclusive upper checkpoint bound for this entity's data. Committer updates this field. All
    /// data of this entity in the checkpoint must be persisted before advancing this watermark. The
    /// committer refers to this on disaster recovery to resume writing.
    pub checkpoint_hi_inclusive: i64,
    /// Inclusive upper transaction sequence number bound for this entity's data. Committer updates
    /// this field.
    pub tx_hi_inclusive: i64,
}
