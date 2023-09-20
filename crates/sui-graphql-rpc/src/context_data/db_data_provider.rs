// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    context_data::db_data_provider::diesel_macro::read_only_blocking, error::Error,
    types::digest::Digest,
};
use diesel::{
    r2d2::{self, ConnectionManager},
    ExpressionMethods, OptionalExtension, PgConnection, QueryDsl, RunQueryDsl,
};
use move_bytecode_utils::module_cache::SyncModuleCache;
use std::{env, str::FromStr, sync::Arc};
use sui_indexer::{
    models_v2::{
        checkpoints::StoredCheckpoint, epoch::StoredEpochInfo, transactions::StoredTransaction,
    },
    schema_v2::{checkpoints, epochs, transactions},
};
pub type PgConnectionPool = diesel::r2d2::Pool<ConnectionManager<PgConnection>>;
pub type PgPoolConnection = diesel::r2d2::PooledConnection<ConnectionManager<PgConnection>>;

use super::module_resolver::PgModuleResolver;

pub(crate) mod diesel_macro {
    macro_rules! read_only_blocking {
        ($pool:expr, $query:expr) => {{
            let mut pg_pool_conn =
                crate::context_data::db_data_provider::get_pg_pool_connection($pool)?;
            pg_pool_conn
                .build_transaction()
                .read_only()
                .run($query)
                .map_err(|e| Error::Internal(e.to_string()))
        }};
    }
    pub(crate) use read_only_blocking;
}

pub fn get_pg_pool_connection(pool: &PgConnectionPool) -> Result<PgPoolConnection, Error> {
    pool.get().map_err(|e| {
        Error::Internal(format!(
            "Failed to get connection from PG connection pool with error: {:?}",
            e
        ))
    })
}

pub fn establish_connection_pool() -> PgConnectionPool {
    let database_url = env::var("PG_DB_URL").expect("PG_DB_URL must be set");
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    r2d2::Pool::builder()
        .build(manager)
        .expect("Failed to create pool.")
}

#[allow(dead_code)]
pub(crate) struct PgManager {
    pub pool: PgConnectionPool,
    pub module_cache: Arc<SyncModuleCache<PgModuleResolver>>,
}

impl PgManager {
    pub(crate) fn new() -> Self {
        let pool = establish_connection_pool();
        Self {
            pool: pool.clone(),
            module_cache: Arc::new(SyncModuleCache::new(PgModuleResolver::new(pool))),
        }
    }

    pub(crate) async fn fetch_tx(&self, digest: &str) -> Result<Option<StoredTransaction>, Error> {
        let digest = Digest::from_str(digest)?;
        read_only_blocking!(&self.pool, |conn| {
            transactions::dsl::transactions
                .filter(transactions::dsl::transaction_digest.eq(digest.into_vec()))
                .get_result::<StoredTransaction>(conn) // Expect exactly 0 to 1 result
                .optional()
        })
    }

    pub(crate) async fn fetch_latest_epoch(&self) -> Result<StoredEpochInfo, Error> {
        read_only_blocking!(&self.pool, |conn| {
            epochs::dsl::epochs
                .order_by(epochs::dsl::epoch.desc())
                .limit(1)
                .first::<StoredEpochInfo>(conn)
        })
    }

    pub(crate) async fn fetch_epoch(
        &self,
        epoch_id: u64,
    ) -> Result<Option<StoredEpochInfo>, Error> {
        let bigint_e = i64::try_from(epoch_id)
            .map_err(|_| Error::Internal("Failed to convert epoch id to i64".to_string()))?;

        read_only_blocking!(&self.pool, |conn| {
            epochs::dsl::epochs
                .filter(epochs::dsl::epoch.eq(bigint_e))
                .get_result::<StoredEpochInfo>(conn) // Expect exactly 0 to 1 result
                .optional()
        })
    }

    pub(crate) async fn fetch_epoch_strict(&self, epoch_id: u64) -> Result<StoredEpochInfo, Error> {
        let result = self.fetch_epoch(epoch_id).await?;
        match result {
            Some(epoch) => Ok(epoch),
            None => Err(Error::Internal(format!("Epoch {} not found", epoch_id))),
        }
    }

    pub(crate) async fn fetch_latest_checkpoint(&self) -> Result<StoredCheckpoint, Error> {
        read_only_blocking!(&self.pool, |conn| {
            checkpoints::dsl::checkpoints
                .order_by(checkpoints::dsl::sequence_number.desc())
                .limit(1)
                .first::<StoredCheckpoint>(conn)
        })
    }

    pub(crate) async fn fetch_checkpoint(
        &self,
        digest: Option<&str>,
        sequence_number: Option<u64>,
    ) -> Result<Option<StoredCheckpoint>, Error> {
        let mut query = checkpoints::dsl::checkpoints.into_boxed();

        match (digest, sequence_number) {
            (Some(digest), None) => {
                let digest = Digest::from_str(digest)?;
                query = query.filter(checkpoints::dsl::checkpoint_digest.eq(digest.into_vec()));
            }
            (None, Some(sequence_number)) => {
                query = query.filter(checkpoints::dsl::sequence_number.eq(sequence_number as i64));
            }
            _ => (), // No-op if invalid input
        }

        read_only_blocking!(&self.pool, |conn| {
            query.get_result::<StoredCheckpoint>(conn).optional()
        })
    }
}
