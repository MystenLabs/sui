// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::{Insertable, Queryable};

use crate::schema_v2::watermarks;

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = watermarks)]
pub struct StoredWatermark {
    pub latest_main_checkpoint_sequence_number: i64,
}
