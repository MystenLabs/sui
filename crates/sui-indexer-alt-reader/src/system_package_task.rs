// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use diesel::{
    sql_types::{BigInt, Bytea},
    QueryableByName,
};
use move_core_types::account_address::AccountAddress;
use sui_sql_macro::query;
use tokio::{task::JoinHandle, time};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::{package_resolver::PackageCache, pg_reader::PgReader};

#[derive(clap::Args, Debug, Clone)]
pub struct SystemPackageTaskArgs {
    /// How long to wait between checking for epoch changes.
    #[clap(long, default_value_t = Self::default().epoch_polling_interval_ms)]
    epoch_polling_interval_ms: u64,
}

/// Background task responsible for evicting system package from the package resolver's cache after
/// detecting an epoch boundary.
pub struct SystemPackageTask {
    /// Access to the database
    pg_reader: PgReader,

    /// The cached store underlying the package resolver.
    package_cache: Arc<PackageCache>,

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
    pub fn new(
        args: SystemPackageTaskArgs,
        pg_reader: PgReader,
        package_cache: Arc<PackageCache>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            pg_reader,
            package_cache,
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
    pub fn run(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            let Self {
                pg_reader,
                package_cache,
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
                        let mut conn = match pg_reader.connect().await {
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

                            #[diesel(sql_type = BigInt)]
                            checkpoint_hi_inclusive: i64,
                        }

                        let query = query!(
                            r#"
                            SELECT
                                epoch_hi_inclusive,
                                checkpoint_hi_inclusive
                            FROM
                                watermarks
                            WHERE
                                pipeline = 'kv_packages'
                            "#
                        );

                        let Watermark {
                            epoch_hi_inclusive: next_epoch,
                            checkpoint_hi_inclusive,
                        } = match conn
                            .results(query)
                            .await
                            .as_deref()
                        {
                            Ok([watermark]) => *watermark,

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

                        if next_epoch <= last_epoch {
                            continue;
                        }

                        info!(last_epoch, next_epoch, "Detected epoch boundary");
                        last_epoch = next_epoch;

                        #[derive(QueryableByName, Clone)]
                        struct SystemPackage {
                            #[diesel(sql_type = Bytea)]
                            original_id: Vec<u8>,
                        }

                        let query = query!(
                            r#"
                            SELECT DISTINCT
                                original_id
                            FROM
                                kv_packages
                            WHERE
                                is_system_package
                            AND cp_sequence_number <= {BigInt}
                            "#,
                            checkpoint_hi_inclusive
                        );

                        let system_packages: Vec<SystemPackage> = match conn.results(query).await {
                            Ok(system_packages) => system_packages,

                            Err(e) => {
                                error!("Failed to fetch system packages: {e}");
                                continue;
                            }
                        };

                        let Ok(system_packages) = system_packages
                            .into_iter()
                            .map(|pkg| AccountAddress::from_bytes(pkg.original_id))
                            .collect::<Result<Vec<_>, _>>()
                        else {
                            error!("Failed to deserialize system package addresses");
                            continue;
                        };

                        info!(system_packages = ?system_packages, "Evicting...");
                        package_cache.evict(system_packages)
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
