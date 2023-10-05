// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Ok;

use diesel::{QueryDsl, RunQueryDsl};

use crate::types_v2::IndexerResult;
use crate::PgConnectionPool;

use crate::schema_v2::{
    active_addresses, address_metrics, addresses, checkpoints, move_call_metrics, network_metrics,
    transactions, tx_count_metrics, tx_indices,
};

use crate::models_v2::address_metrics::{StoredActiveAddress, StoredAddress, StoredAddressMetrics};
use crate::models_v2::checkpoints::StoredCheckpoint;
use crate::models_v2::move_call_metrics::{
    build_move_call_metric_query, DerivedMoveCallInfo, QueriedMoveMetrics, StoredMoveCall,
    StoredMoveCallMetrics,
};
use crate::models_v2::network_metrics::StoredNetworkMetrics;
use crate::models_v2::transactions::StoredTransaction;
use crate::models_v2::tx_count_metrics::{StoredTxCountMetrics, TxCountMetricsDelta};
use crate::types_v2::IndexerResult;

use crate::store::diesel_macro::{read_only_blocking, transactional_blocking_with_retry};

use super::AnalyticsStore;

#[derive(Clone)]
pub struct PgAnalyticalStore {
    blocking_cp: PgConnectionPool,
}

impl PgAnalyticalStore {
    pub fn new(blocking_cp: PgConnectionPool) -> Self {
        Self { blocking_cp }
    }

    async fn get_latest_stored_checkpoint(&self) -> IndexerResult<StoredCheckpoint> {
        let latest_cp = read_only_blocking!(&self.blocking_cp, |conn| {
            checkpoints::dsl::checkpoints
                .order(checkpoints::dsl::sequence_number.desc())
                .first::<StoredCheckpoint>(conn)
        })
        .context("Failed reading latest checkpoint from PostgresDB")?;
        Ok(latest_cp)
    }

    async fn get_checkpoints_in_range(
        &self,
        start_checkpoint: i64,
        end_checkpoint: i64,
    ) -> IndexerResult<Vec<StoredCheckpoint>> {
        let cps = read_only_blocking!(&self.blocking_cp, |conn| {
            checkpoints::dsl::checkpoints
                .filter(checkpoints::dsl::sequence_number.ge(start_checkpoint))
                .filter(checkpoints::dsl::sequence_number.lt(end_checkpoint))
                .order(checkpoints::dsl::sequence_number.asc())
                .load::<StoredCheckpoint>(conn)
        })
        .context("Failed reading checkpoints from PostgresDB")?;
        Ok(cps)
    }

    async fn get_transactions_in_checkpoint_range(
        &self,
        start_checkpoint: i64,
        end_checkpoint: i64,
    ) -> IndexerResult<Vec<StoredTransaction>> {
        let tx_batch = read_only_blocking!(&self.blocking_cp, |conn| {
            transactions::dsl::transactions
                .filter(transactions::dsl::checkpoint_sequence_number.ge(start_checkpoint))
                .filter(transactions::dsl::checkpoint_sequence_number.lt(end_checkpoint))
                .order(transactions::dsl::tx_sequence_number.asc())
                .load::<StoredTransaction>(conn)
        })
        .context("Failed reading transactions from PostgresDB")?;
        Ok(tx_batch)
    }

    async fn get_tx_indices_in_checkpoint_range(
        &self,
        start_checkpoint: i64,
        end_checkpoint: i64,
    ) -> IndexerResult<Vec<StoredTxIndex>> {
        let tx_indices = read_only_blocking!(&self.blocking_cp, |conn| {
            tx_indices::dsl::tx_indices
                .filter(tx_indices::dsl::checkpoint_sequence_number.ge(start_checkpoint))
                .filter(tx_indices::dsl::checkpoint_sequence_number.lt(end_checkpoint))
                .order(tx_indices::dsl::tx_sequence_number.asc())
                .load::<StoredTxIndex>(conn)
        })
        .context("Failed reading tx indices from PostgresDB")?;
        Ok(tx_indices)
    }

    async fn get_estimated_count(&self, table: &str) -> IndexerResult<i64> {
        let count = read_only_blocking!(&self.blocking_cp, |conn| {
            diesel::sql_query(format!(
                "SELECT reltuples::bigint AS estimate FROM pg_class WHERE relname='{}';",
                table
            ))
            .get_result::<i64>(conn)
        })
        .context("Failed reading estimated count from PostgresDB")?;
        Ok(count)
    }

    async fn get_latest_tx_count_metrics(&self) -> IndexerResult<StoredTxCountMetrics> {
        let latest_tx_count = read_only_blocking!(&self.blocking_cp, |conn| {
            tx_count_metrics::dsl::tx_count_metrics
                .order(tx_count_metrics::dsl::checkpoint_sequence_number.desc())
                .first::<StoredTxCountMetrics>(conn)
        })
        .context("Failed reading latest tx count metrics from PostgresDB")?;
        Ok(latest_tx_count)
    }

    async fn persist_tx_count_metrics(
        &self,
        tx_count_metrics: StoredTxCountMetrics,
    ) -> IndexerResult<()> {
        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                diesel::insert_into(tx_count_metrics::dsl::tx_count_metrics)
                    .values(tx_count_metrics)
                    .execute(conn)
            },
            Duration::from_secs(60)
        )
        .context("Failed persisting tx count metrics to PostgresDB")?;
        Ok(())
    }

    async fn persist_network_metrics(
        &self,
        network_metrics: StoredNetworkMetrics,
    ) -> IndexerResult<()> {
        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                diesel::insert_into(network_metrics::dsl::network_metrics)
                    .values(network_metrics)
                    .execute(conn)
            },
            Duration::from_secs(60)
        )
        .context("Failed persisting network metrics to PostgresDB")?;
        Ok(())
    }

    async fn get_latest_address_metrics(&self) -> IndexerResult<StoredAddressMetrics> {
        let latest_address_metrics = read_only_blocking!(&self.blocking_cp, |conn| {
            address_metrics::dsl::address_metrics
                .order(address_metrics::dsl::checkpoint.desc())
                .first::<StoredAddressMetrics>(conn)
        })
        .context("Failed reading latest address metrics from PostgresDB")?;
        Ok(latest_address_metrics)
    }

    async fn persist_addresses(&self, addresses: Vec<StoredAddress>) -> IndexerResult<()> {
        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                diesel::insert_into(addresses::dsl::addresses)
                    .values(addresses)
                    .execute(conn)
            },
            Duration::from_secs(60)
        )
        .context("Failed persisting addresses to PostgresDB")?;
        Ok(())
    }

    async fn persist_active_addresses(
        &self,
        active_addresses: Vec<StoredActiveAddress>,
    ) -> IndexerResult<()> {
        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                diesel::insert_into(active_addresses::dsl::active_addresses)
                    .values(active_addresses)
                    .execute(conn)
            },
            Duration::from_secs(60)
        )
        .context("Failed persisting active addresses to PostgresDB")?;
        Ok(())
    }

    async fn calculate_address_metrics(
        &self,
        checkpoint: &StoredCheckpoint,
    ) -> IndexerResult<StoredAddressMetrics> {
        let cp_timestamp_ms = checkpoint.timestamp_ms;
        let addr_count = read_only_blocking!(&self.blocking_cp, |conn| {
            addresses::dsl::addresses
                .filter(addresses::first_appearance_time.le(cp_timestamp_ms))
                .count()
                .get_result::<i64>(conn)
        })?;
        let active_addr_count = read_only_blocking!(&self.blocking_cp, |conn| {
            active_addresses::dsl::active_addresses
                .filter(active_addresses::first_appearance_time.le(cp_timestamp_ms))
                .count()
                .get_result::<i64>(conn)
        })?;
        let time_one_day_ago = cp_timestamp_ms - 1000 * 60 * 60 * 24;
        let daily_active_addresses = active_addresses::dsl::active_addresses
            .filter(active_addresses::first_appearance_time.le(cp_timestamp_ms))
            .filter(active_addresses::last_appearance_time.gt(time_one_day_ago))
            .select(count(active_addresses::account_address))
            .first(conn)?;
        Ok(StoredAddressMetrics {
            checkpoint: checkpoint.sequence_number,
            epoch: checkpoint.epoch,
            timestamp_ms: checkpoint.timestamp_ms,
            cumulative_addresses: addr_count,
            cumulative_active_addresses: active_addr_count,
            daily_active_addresses,
        })
    }

    async fn persist_address_metrics(
        &self,
        address_metrics: StoredAddressMetrics,
    ) -> IndexerResult<()> {
        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                diesel::insert_into(address_metrics::dsl::address_metrics)
                    .values(address_metrics)
                    .execute(conn)
            },
            Duration::from_secs(60)
        )
        .context("Failed persisting address metrics to PostgresDB")?;
        Ok(())
    }

    async fn get_latest_move_call_metrics(&self) -> IndexerResult<StoredMoveCallMetrics> {
        let latest_move_call_metrics = read_only_blocking!(&self.blocking_cp, |conn| {
            move_call_metrics::dsl::move_call_metrics
                .order(move_call_metrics::dsl::checkpoint.desc())
                .first::<StoredMoveCallMetrics>(conn)
        })
        .context("Failed reading latest move call metrics from PostgresDB")?;
        Ok(latest_move_call_metrics)
    }

    async fn persist_move_calls(&self, move_calls: Vec<StoredMoveCall>) -> IndexerResult<()> {
        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                diesel::insert_into(move_calls::dsl::move_calls)
                    .values(move_calls)
                    .execute(conn)
            },
            Duration::from_secs(60)
        )
        .context("Failed persisting move calls to PostgresDB")?;
        Ok(())
    }

    async fn calculate_move_call_metrics(
        &self,
        checkpoint: &StoredCheckpoint,
    ) -> IndexerResult<Vec<StoredMoveCallMetrics>> {
        let epoch = checkpoint.epoch;
        let move_call_query_3d = build_move_call_metric_query(epoch, 3);
        let move_call_query_7d = build_move_call_metric_query(epoch, 7);
        let move_call_query_30d = build_move_call_metric_query(epoch, 30);

        let move_call_metrics_3d = read_only_blocking!(&self.blocking_cp, |conn| {
            diesel::sql_query(move_call_query_3d).get_results::<QueriedMoveMetrics>(conn)
        })?;
        let move_call_metrics_7d = read_only_blocking!(&self.blocking_cp, |conn| {
            diesel::sql_query(move_call_query_7d).get_results::<QueriedMoveMetrics>(conn)
        })?;
        let move_call_metrics_30d = read_only_blocking!(&self.blocking_cp, |conn| {
            diesel::sql_query(move_call_query_30d).get_results::<QueriedMoveMetrics>(conn)
        })?;

        let chained = move_call_metrics_3d
            .into_iter()
            .chain(move_call_metrics_7d.into_iter())
            .chain(move_call_metrics_30d.into_iter())
            .collect::<Vec<_>>();
        let move_call_metrics: Vec<StoredMoveCallMetrics> =
            chained
                .into_iter()
                .map(|queried_move_metrics| StoredMoveCallMetrics {
                    id: None,
                    checkpoint: checkpoint.sequence_number,
                    epoch: checkpoint.epoch,
                    day: queried_move_metrics.day,
                    move_package: queried_move_metrics.move_package,
                    move_module: queried_move_metrics.move_module,
                    move_function: queried_move_metrics.move_function,
                    count: queried_move_metrics.count,
                });
        Ok(move_call_metrics)
    }

    fn persist_move_call_metrics(
        &self,
        move_call_metrics: Vec<StoredMoveCallMetrics>,
    ) -> IndexerResult<()> {
        transactional_blocking_with_retry!(
            &self.blocking_cp,
            |conn| {
                diesel::insert_into(move_call_metrics::dsl::move_call_metrics)
                    .values(move_call_metrics)
                    .execute(conn)
            },
            Duration::from_secs(60)
        )
        .context("Failed persisting move call metrics to PostgresDB")?;
        Ok(())
    }
}
