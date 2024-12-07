// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::SYSTEM_PACKAGE_ADDRESSES;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::data::package_resolver::PackageResolver;

/// Background task responsible for evicting system packages from the package resolver's cache on
/// epoch boundaries.
pub(crate) struct SystemPackageTask {
    resolver: PackageResolver,
    epoch_rx: watch::Receiver<u64>,
    cancel: CancellationToken,
}

impl SystemPackageTask {
    pub(crate) fn new(
        resolver: PackageResolver,
        epoch_rx: watch::Receiver<u64>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            resolver,
            epoch_rx,
            cancel,
        }
    }

    pub(crate) async fn run(&mut self) {
        loop {
            tokio::select! {
                _ = self.cancel.cancelled() => {
                    info!("Shutdown signal received, terminating system package eviction task");
                    return;
                }

                _ = self.epoch_rx.changed() => {
                    info!("Detected epoch boundary, evicting system packages from cache");
                    self.resolver
                        .package_store()
                        .evict(SYSTEM_PACKAGE_ADDRESSES.iter().copied());
                }
            }
        }
    }
}
