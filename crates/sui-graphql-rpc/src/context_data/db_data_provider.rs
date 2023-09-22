// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use crate::{
    context_data::db_data_provider::diesel_macro::read_only_blocking,
    error::Error,
    types::{
        checkpoint::CheckpointId, digest::Digest, object::ObjectFilter,
        transaction_block::TransactionBlockFilter,
    },
};
use diesel::{
    r2d2::{self, ConnectionManager},
    ExpressionMethods, JoinOnDsl, PgArrayExpressionMethods, PgConnection, QueryDsl, RunQueryDsl,
};
use move_bytecode_utils::module_cache::SyncModuleCache;
use std::{env, str::FromStr, sync::Arc};
use sui_indexer::{
    models_v2::{
        checkpoints::StoredCheckpoint, epoch::StoredEpochInfo, objects::StoredObject,
        transactions::StoredTransaction,
    },
    schema_v2::{checkpoints, epochs, objects, transactions, tx_indices},
};
use sui_json_rpc_types::SuiTransactionBlockResponse;
pub type PgConnectionPool = diesel::r2d2::Pool<ConnectionManager<PgConnection>>;
pub type PgPoolConnection = diesel::r2d2::PooledConnection<ConnectionManager<PgConnection>>;
use async_graphql::Result;

use super::module_resolver::PgModuleResolver;

pub(crate) mod diesel_macro {
    macro_rules! read_only_blocking {
        ($pool:expr, $query:expr) => {{
            let mut pg_pool_conn =
                crate::context_data::db_data_provider::get_pg_pool_connection($pool)?;
            let result = pg_pool_conn.build_transaction().read_only().run($query);

            match result {
                Ok(value) => Ok(Some(value)),
                Err(e) => match e {
                    diesel::result::Error::NotFound => Ok(None),
                    _ => Err(Error::Internal(e.to_string())),
                },
            }
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

    // Lifted directly from https://github.com/MystenLabs/sui/blob/4e847ee6cbef7e667199d15e67af28e54322273c/crates/sui-indexer/src/store/pg_indexer_store_v2.rs#L747
    pub(crate) async fn fetch_tx(
        &self,
        digest: String,
    ) -> Result<Option<SuiTransactionBlockResponse>> {
        let digest = Digest::from_str(&digest)?;
        let result: Option<StoredTransaction> = read_only_blocking!(&self.pool, |conn| {
            transactions::dsl::transactions
                .filter(transactions::dsl::transaction_digest.eq(digest.into_vec()))
                .first::<StoredTransaction>(conn)
        })?;

        match result {
            Some(stored_tx) => {
                let transformed = stored_tx
                    .try_into_sui_transaction_block_response(&self.module_cache)
                    .map_err(Error::from)?;
                Ok(Some(transformed))
            }
            None => Ok(None),
        }
    }

    pub(crate) async fn fetch_txs(
        &self,
        _first: Option<u64>,
        _after: Option<String>,
        _last: Option<u64>,
        _before: Option<String>,
        filter: Option<TransactionBlockFilter>,
    ) -> Result<Option<Vec<StoredTransaction>>> {
        let mut query =
            transactions::dsl::transactions
                .inner_join(tx_indices::dsl::tx_indices.on(
                    transactions::dsl::transaction_digest.eq(tx_indices::dsl::transaction_digest),
                ))
                .into_boxed();

        if let Some(filter) = filter {
            // Filters for transaction table
            if let Some(kind) = filter.kind {
                query = query.filter(transactions::dsl::transaction_kind.eq(kind as i16));
            }
            if let Some(checkpoint) = filter.checkpoint {
                query = query
                    .filter(transactions::dsl::checkpoint_sequence_number.eq(checkpoint as i64));
            }

            // Filters for tx_indices table
            match (filter.package, filter.module, filter.function) {
                (Some(p), None, None) => {
                    query = query.filter(
                        tx_indices::dsl::packages.contains(vec![Some(p.into_array().to_vec())]),
                    );
                }
                (Some(p), Some(m), None) => {
                    query = query.filter(
                        tx_indices::dsl::package_modules
                            .contains(vec![Some(format!("{}::{}", p, m))]),
                    );
                }
                (Some(p), Some(m), Some(f)) => {
                    query = query.filter(
                        tx_indices::dsl::package_module_functions
                            .contains(vec![Some(format!("{}::{}::{}", p, m, f))]),
                    );
                }
                _ => {}
            }
            if let Some(sender) = filter.sent_address {
                query = query.filter(tx_indices::dsl::senders.contains(vec![sender.into_vec()]));
            }
            if let Some(receiver) = filter.recv_address {
                query =
                    query.filter(tx_indices::dsl::recipients.contains(vec![receiver.into_vec()]));
            }
            // TODO: sign_, paid_address, input_, changed_object
        };

        let result: Option<Vec<StoredTransaction>> = read_only_blocking!(&self.pool, |conn| {
            query.select(transactions::all_columns).load(conn)
        })?;

        Ok(result)
    }

    pub(crate) async fn fetch_objs(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<ObjectFilter>,
    ) -> Result<Option<Vec<StoredObject>>> {
        let mut query = objects::dsl::objects.into_boxed();

        if let Some(filter) = filter {
            if let Some(object_ids) = filter.object_ids {
                query = query.filter(
                    objects::dsl::object_id.eq_any(
                        object_ids
                            .into_iter()
                            .map(|id| id.into_vec())
                            .collect::<Vec<_>>(),
                    ),
                );
            }

            if let Some(_object_keys) = filter.object_keys {
                // TODO: Temporary table? Probably better than a long list of ORs
            }

            if let Some(owner) = filter.owner {
                query = query.filter(objects::dsl::owner_id.eq(owner.into_vec()));
            }
        }

        // TODO: for demonstration purposes only, not finalized and assumes checkpoint sequence number for now.
        if let Some(after) = after {
            let after: i64 = after.parse().expect("Failed to parse string to i64");
            query = query.filter(objects::dsl::checkpoint_sequence_number.gt(after));
        } else if let Some(before) = before {
            let before: i64 = before.parse().expect("Failed to parse string to i64");
            query = query.filter(objects::dsl::checkpoint_sequence_number.lt(before));
        }

        if let Some(first) = first {
            query = query
                .order_by(objects::dsl::checkpoint_sequence_number.asc())
                .limit(first as i64);
        } else if let Some(last) = last {
            query = query
                .order_by(objects::dsl::checkpoint_sequence_number.desc())
                .limit(last as i64);
        }

        let result: Option<Vec<StoredObject>> =
            read_only_blocking!(&self.pool, |conn| { query.load(conn) }).map_err(Error::from)?;
        Ok(result)
    }

    pub(crate) async fn fetch_epoch(
        &self,
        epoch_id: Option<u64>,
    ) -> Result<Option<StoredEpochInfo>> {
        let mut query = epochs::dsl::epochs.into_boxed();

        if let Some(e) = epoch_id {
            let bigint_e = i64::try_from(e)
                .map_err(|_| Error::Internal("Failed to convert epoch to i64".to_string()))?;
            query = query.filter(epochs::dsl::epoch.eq(bigint_e));
        } else {
            query = query.order(epochs::dsl::epoch.desc()).limit(1);
        }

        Ok(
            read_only_blocking!(&self.pool, |conn| { query.first::<StoredEpochInfo>(conn) })
                .map_err(Error::from)?,
        )
    }

    pub(crate) async fn fetch_checkpoint(
        &self,
        id: CheckpointId,
    ) -> Result<Option<StoredCheckpoint>> {
        let mut query = checkpoints::dsl::checkpoints.into_boxed();

        match (id.digest, id.sequence_number) {
            (Some(digest), None) => {
                let digest = Digest::from_str(&digest)?;
                query = query.filter(checkpoints::dsl::checkpoint_digest.eq(digest.into_vec()));
            }
            (None, Some(sequence_number)) => {
                query = query.filter(checkpoints::dsl::sequence_number.eq(sequence_number as i64));
            }
            (None, None) => {
                query = query
                    .order(checkpoints::dsl::sequence_number.desc())
                    .limit(1);
            }
            _ => (), // No-op if invalid input
        }

        Ok(
            read_only_blocking!(&self.pool, |conn| { query.first::<StoredCheckpoint>(conn) })
                .map_err(Error::from)?,
        )
    }

    pub(crate) async fn fetch_events(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Vec<StoredTransaction>>> {
        let mut query = transactions::dsl::transactions.into_boxed();

        if let Some(after) = after {
            let digest = Digest::from_str(&after)?;
            query = query.filter(transactions::dsl::transaction_digest.gt(digest.into_vec()));
        }

        if let Some(before) = before {
            let digest = Digest::from_str(&before)?;
            query = query.filter(transactions::dsl::transaction_digest.lt(digest.into_vec()));
        }

        if let Some(first) = first {
            query = query.limit(first as i64);
        }

        if let Some(last) = last {
            query = query.limit(last as i64);
        }

        Ok(
            read_only_blocking!(&self.pool, |conn| { query.load::<StoredTransaction>(conn) })
                .map_err(Error::from)?,
        )
    }
}
