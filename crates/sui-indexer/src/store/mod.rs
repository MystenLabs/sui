// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use indexer_store::*;
pub use pg_indexer_store::PgIndexerStore;

mod indexer_store;
mod module_resolver;
mod pg_indexer_store;
mod query;

mod diesel_marco {
    // MUSTFIX(gegaowp): figure read-only
    macro_rules! read_only_blocking {
        ($pool:expr, $query:expr) => {{
            let mut mysql_pool_conn = crate::get_db_pool_connection($pool)?;
            $query(&mut mysql_pool_conn)
                .map_err(|e| IndexerError::PostgresWriteError(e.to_string()))
        }};
    }

    macro_rules! transactional_blocking {
        ($pool:expr, $query:expr) => {{
            let mut mysql_pool_conn = crate::get_db_pool_connection($pool)?;
            $query(mysql_pool_conn)
                .map_err(|e| IndexerError::PostgresWriteError(e.to_string()))
        }};
    }
    pub(crate) use read_only_blocking;
    pub(crate) use transactional_blocking;
}
