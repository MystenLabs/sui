// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer::{new_pg_connection_pool, types_v2::IndexerResult, utils::reset_database};

pub fn reset_db(db_url: &str, drop_all: bool, use_v2: bool) -> IndexerResult<()> {
    let blocking_cp = new_pg_connection_pool(db_url)?;
    reset_database(&mut blocking_cp.get().unwrap(), drop_all, use_v2).unwrap();
    Ok(())
}
