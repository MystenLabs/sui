// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use diesel::{sql_query, sql_types::BigInt, QueryableByName};
use sui_types::SYSTEM_PACKAGE_ADDRESSES;
use tokio::{task::JoinHandle, time};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::context::Context;

#[derive(clap::Args, Debug, Clone)]
pub struct SystemPackageTaskArgs {
    /// How long to wait between checking for epoch changes.
    #[clap(long, default_value_t = Self::default().epoch_polling_interval_ms)]
    epoch_polling_interval_ms: u64,
}

/// Background task responsible for evicting system package from the package resolver's cache after
/// detecting an epoch boundary.
pub(crate) struct SystemPackageTask {
    /// Access to the database and package resolver.
    context: Context,
    /// How long to wait between checks.
    interval: Duration,
    /// Signal to cancel the task.
    cancel: CancellationToken,
}

impl SystemPackageTaskArgs {
    pub fn epoch_polling_interval(&self) -> Duration {
        Duration::from_millis(self.epoch_polling_interval_ms)
    }
}

impl SystemPackageTask {
    pub(crate) fn new(
        context: Context,
        args: SystemPackageTaskArgs,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            context,
            interval: args.epoch_polling_interval(),
            cancel,
        }
    }

    /// Start a new task that regularly polls the database for the latest epoch and evicts system
    /// packages if it detects that the epoch has changed (which means that a framework upgrade
    /// could have happened).
    ///
    /// This operation consumes the `self` and returns a handle to the spawned tokio task. The task
    /// will continue to run until its cancellation token is triggered.
    pub(crate) fn run(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            let Self {
                context,
                interval,
                cancel,
            } = self;

            let mut last_epoch: i64 = 0;
            let mut interval = time::interval(interval);

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        info!("Shutdown signal received, terminating system package eviction task");
                        break;
                    }

                    _ = interval.tick() => {
                        let mut conn = match context.reader().connect().await {
                            Ok(conn) => conn,
                            Err(e) => {
                                error!("Failed to connect to database: {:?}", e);
                                continue;
                            }
                        };

                        #[derive(QueryableByName, Copy, Clone)]
                        struct Watermark {
                            #[diesel(sql_type = BigInt)]
                            epoch_hi_inclusive: i64,
                        }

                        let query = sql_query(r#"
                            SELECT epoch_hi_inclusive FROM watermarks WHERE pipeline = 'sum_packages'
                        "#);

                        let Watermark { epoch_hi_inclusive: next_epoch } = match conn
                            .results(query)
                            .await
                            .as_deref()
                        {
                            Ok([epoch]) => *epoch,

                            Ok([]) => {
                                info!("Package index isn't populated yet, no epoch information");
                                continue;
                            }

                            Ok(_) => {
                                error!("Expected exactly one row from the watermarks table");
                                continue;
                            },

                            Err(e) => {
                                error!("Failed to fetch latest epoch: {e}");
                                continue;
                            }
                        };

                        if next_epoch > last_epoch {
                            info!(last_epoch, next_epoch, "Detected epoch boundary, evicting system packages from cache");
                            last_epoch = next_epoch;
                            context.package_resolver().package_store().evict(SYSTEM_PACKAGE_ADDRESSES.iter().copied())
                        }
                    }
                }
            }
        })
    }
}

impl Default for SystemPackageTaskArgs {
    fn default() -> Self {
        Self {
            epoch_polling_interval_ms: 10_000,
        }
    }
}
