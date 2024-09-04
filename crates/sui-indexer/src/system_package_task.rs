// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use crate::store::diesel_macro::*;
use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
use sui_types::SYSTEM_PACKAGE_ADDRESSES;
use tokio_util::sync::CancellationToken;

use crate::{indexer_reader::IndexerReader, schema::epochs};

/// Background task responsible for evicting system packages from the package resolver's cache after
/// detecting an epoch boundary.
pub(crate) struct SystemPackageTask {
    /// Holds the DB connection and also the package resolver to evict packages from.
    reader: IndexerReader,
    /// Signal to cancel the task.
    cancel: CancellationToken,
    /// Interval to sleep for between checks.
    interval: Duration,
}

impl SystemPackageTask {
    pub(crate) fn new(
        reader: IndexerReader,
        cancel: CancellationToken,
        interval: Duration,
    ) -> Self {
        Self {
            reader,
            cancel,
            interval,
        }
    }

    pub(crate) async fn run(&self) {
        let mut last_epoch: i64 = 0;
        loop {
            tokio::select! {
                _ = self.cancel.cancelled() => {
                    tracing::info!(
                        "Shutdown signal received, terminating system package eviction task"
                    );
                    return;
                }
                _ = tokio::time::sleep(self.interval) => {
                    let pool = self.reader.get_pool();
                    let next_epoch = match run_query_async!(&pool, move |conn| {
                            epochs::dsl::epochs
                                .select(epochs::dsl::epoch)
                                .order_by(epochs::epoch.desc())
                                .first::<i64>(conn)
                        }) {
                        Ok(epoch) => epoch,
                        Err(e) => {
                            tracing::error!("Failed to fetch latest epoch: {:?}", e);
                            continue;
                        }
                    };

                    if next_epoch > last_epoch {
                        last_epoch = next_epoch;
                        tracing::info!(
                            "Detected epoch boundary, evicting system packages from cache"
                        );
                        self.reader
                            .package_resolver()
                            .package_store()
                            .evict(SYSTEM_PACKAGE_ADDRESSES.iter().copied());
                    }
                }
            }
        }
    }
}
