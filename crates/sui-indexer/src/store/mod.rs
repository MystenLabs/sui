// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use indexer_store::*;
pub use pg_indexer_store::PgIndexerStore;

mod indexer_store;
mod module_resolver;
mod pg_indexer_store;
mod query;

mod diesel_marco {
    macro_rules! read_only_blocking {
        ($pool:expr, $query:expr) => {{
            let mut pg_pool_conn = crate::get_pg_pool_connection($pool)?;
            pg_pool_conn
                .build_transaction()
                .read_only()
                .run($query)
                .map_err(|e| IndexerError::PostgresReadError(e.to_string()))
        }};
    }

    macro_rules! transactional_blocking {
        ($pool:expr, $query:expr) => {{
            let mut pg_pool_conn = crate::get_pg_pool_connection($pool)?;
            pg_pool_conn
                .build_transaction()
                .serializable()
                .read_write()
                .run($query)
                .map_err(|e| IndexerError::PostgresWriteError(e.to_string()))
        }};
    }
    pub(crate) use read_only_blocking;
    pub(crate) use transactional_blocking;
}
