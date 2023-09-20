// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use crate::{error::Error, types::digest::Digest};
use diesel::{ExpressionMethods, OptionalExtension, PgConnection, QueryDsl, RunQueryDsl};
use std::str::FromStr;
use sui_indexer::{
    indexer_reader::IndexerReader,
    models_v2::{
        checkpoints::StoredCheckpoint, epoch::StoredEpochInfo, transactions::StoredTransaction, objects::StoredObject,        
    },
    schema_v2::{checkpoints, epochs, transactions, objects, tx_indices},
    PgConnectionPoolConfig,
};

pub(crate) struct PgManager {
    pub inner: IndexerReader,
}

impl PgManager {
    pub(crate) fn new<T: Into<String>>(
        db_url: T,
        config: Option<PgConnectionPoolConfig>,
    ) -> Result<Self, Error> {
        // TODO (wlmyng): support config
        let mut config = config.unwrap_or(PgConnectionPoolConfig::default());
        config.set_pool_size(30);
        let inner = IndexerReader::new_with_config(db_url, config)
            .map_err(|e| Error::Internal(e.to_string()))?;

        Ok(Self { inner })
    }

    pub async fn run_query_async<T, E, F>(&self, query: F) -> Result<T, Error>
    where
        F: FnOnce(&mut PgConnection) -> Result<T, E> + Send + 'static,
        E: From<diesel::result::Error> + std::error::Error + Send + 'static,
        T: Send + 'static,
    {
        self.inner
            .run_query_async(query)
            .await
            .map_err(|e| Error::Internal(e.to_string()))
    }

    pub(crate) async fn fetch_tx(&self, digest: &str) -> Result<Option<StoredTransaction>, Error> {
        let digest = Digest::from_str(digest)?.into_vec();

        self.run_query_async(|conn| {
            transactions::dsl::transactions
                .filter(transactions::dsl::transaction_digest.eq(digest))
                .get_result::<StoredTransaction>(conn) // Expect exactly 0 to 1 result
                .optional()
        })
        .await
    }

    pub(crate) async fn fetch_latest_epoch(&self) -> Result<StoredEpochInfo, Error> {
        self.run_query_async(|conn| {
            epochs::dsl::epochs
                .order_by(epochs::dsl::epoch.desc())
                .limit(1)
                .first::<StoredEpochInfo>(conn)
        })
        .await
    }

    pub(crate) async fn fetch_epoch(
        &self,
        epoch_id: u64,
    ) -> Result<Option<StoredEpochInfo>, Error> {
        let epoch_id = i64::try_from(epoch_id)
            .map_err(|_| Error::Internal("Failed to convert epoch id to i64".to_string()))?;
        self.run_query_async(move |conn| {
            epochs::dsl::epochs
                .filter(epochs::dsl::epoch.eq(epoch_id))
                .get_result::<StoredEpochInfo>(conn) // Expect exactly 0 to 1 result
                .optional()
        })
        .await
    }

    pub(crate) async fn fetch_epoch_strict(&self, epoch_id: u64) -> Result<StoredEpochInfo, Error> {
        let result = self.fetch_epoch(epoch_id).await?;
        match result {
            Some(epoch) => Ok(epoch),
            None => Err(Error::Internal(format!("Epoch {} not found", epoch_id))),
        }
    }

    pub(crate) async fn fetch_latest_checkpoint(&self) -> Result<StoredCheckpoint, Error> {
        self.run_query_async(|conn| {
            checkpoints::dsl::checkpoints
                .order_by(checkpoints::dsl::sequence_number.desc())
                .limit(1)
                .first::<StoredCheckpoint>(conn)
        })
        .await
    }

    pub(crate) async fn fetch_checkpoint(
        &self,
        digest: Option<&str>,
        sequence_number: Option<u64>,
    ) -> Result<Option<StoredCheckpoint>, Error> {
        let mut query = checkpoints::dsl::checkpoints.into_boxed();

        match (digest, sequence_number) {
            (Some(digest), None) => {
                let digest = Digest::from_str(digest)?.into_vec();
                query = query.filter(checkpoints::dsl::checkpoint_digest.eq(digest));
            }
            (None, Some(sequence_number)) => {
                query = query.filter(checkpoints::dsl::sequence_number.eq(sequence_number as i64));
            }
            _ => (), // No-op if invalid input
        }

        self.run_query_async(|conn| query.get_result::<StoredCheckpoint>(conn).optional())
            .await
    }

    pub(crate) async fn fetch_txs(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<TransactionBlockFilter>,
    ) -> Result<Option<(Vec<(String, StoredTransaction)>, bool)>, Error> {
        let mut query =
            transactions::dsl::transactions
                .inner_join(tx_indices::dsl::tx_indices.on(
                    transactions::dsl::tx_sequence_number.eq(tx_indices::dsl::tx_sequence_number),
                ))
                .into_boxed();
        if let Some(after) = after {
            let after: i64 = after.parse().expect("Failed to parse string to i64");
            query = query
                .filter(transactions::dsl::tx_sequence_number.gt(after))
                .order(transactions::dsl::tx_sequence_number.asc());
        } else if let Some(before) = before {
            let before: i64 = before.parse().expect("Failed to parse string to i64");
            query = query
                .filter(transactions::dsl::tx_sequence_number.lt(before))
                .order(transactions::dsl::tx_sequence_number.desc());
        }

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

        let limit = first.or(last).unwrap_or(10) as i64;
        query = query.limit(limit + 1);

        let result: Option<Vec<StoredTransaction>> = read_only_blocking!(&self.pool, |conn| {
            query
                .select(transactions::all_columns)
                .load(conn)
                .optional()
        })?;

        result
            .map(|mut stored_txs| {
                let has_next_page = stored_txs.len() as i64 > limit;
                if has_next_page {
                    stored_txs.pop();
                }

                let transformed = stored_txs
                    .into_iter()
                    .map(|stored_tx| {
                        let cursor = stored_tx.tx_sequence_number.to_string();
                        (cursor, stored_tx)
                    })
                    .collect();

                Ok((transformed, has_next_page))
            })
            .transpose()
    }

    pub(crate) async fn fetch_objs(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<ObjectFilter>,
    ) -> Result<Option<(Vec<(String, StoredObject)>, bool)>, Error> {
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
            query = query
                .filter(objects::dsl::checkpoint_sequence_number.gt(after))
                .order(objects::dsl::checkpoint_sequence_number.asc());
        } else if let Some(before) = before {
            let before: i64 = before.parse().expect("Failed to parse string to i64");
            query = query
                .filter(objects::dsl::checkpoint_sequence_number.lt(before))
                .order(objects::dsl::checkpoint_sequence_number.desc());
        }

        let limit = first.or(last).unwrap_or(10) as i64;
        query = query.limit(limit + 1);

        let result: Option<Vec<StoredObject>> =
            read_only_blocking!(&self.pool, |conn| { query.load(conn).optional() })?;

        result
            .map(|mut stored_objs| {
                let has_next_page = stored_objs.len() as i64 > limit;
                if has_next_page {
                    stored_objs.pop();
                }

                let transformed = stored_objs
                    .into_iter()
                    .map(|stored_obj| {
                        let cursor = stored_obj.checkpoint_sequence_number.to_string();
                        (cursor, stored_obj)
                    })
                    .collect();
                Ok((transformed, has_next_page))
            })
            .transpose()
    }
}
