// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use sui_futures::service::Service;
use sui_types::SYSTEM_PACKAGE_ADDRESSES;
use tokio::time;
use tracing::info;
use tracing::warn;

use crate::ledger_grpc_reader::LedgerGrpcReader;
use crate::monotonic_read::CheckpointFloor;
use crate::package_resolver::PackageCache;

#[derive(clap::Args, Debug, Clone)]
pub struct SystemPackageTaskArgs {
    /// How long to wait between checking for epoch changes.
    #[clap(long, default_value_t = Self::default().epoch_polling_interval_ms)]
    epoch_polling_interval_ms: u64,
}

/// Background task responsible for evicting system package from the package resolver's cache after
/// detecting an epoch boundary.
///
/// System packages are the only packages upgraded in place (at the same storage ID), at epoch
/// boundaries. The task watches the epoch of the same ledger gRPC service that the package cache
/// reads from, so that a fetch after an eviction sees the upgraded packages.
pub struct SystemPackageTask {
    /// The ledger gRPC service backing the package cache.
    ledger_grpc_reader: LedgerGrpcReader,

    /// The floor `ledger_grpc_reader` stamps on its requests. Raised to the checkpoint that
    /// revealed a new epoch before evicting, so refetches cannot be answered by a replica that is
    /// still in the previous epoch.
    ledger_grpc_floor: CheckpointFloor,

    /// The cached store underlying the package resolver.
    package_cache: Arc<PackageCache>,

    /// How long to wait between checks.
    interval: Duration,
}

impl SystemPackageTaskArgs {
    pub fn epoch_polling_interval(&self) -> Duration {
        Duration::from_millis(self.epoch_polling_interval_ms)
    }
}

impl SystemPackageTask {
    pub fn new(
        args: SystemPackageTaskArgs,
        ledger_grpc_reader: LedgerGrpcReader,
        ledger_grpc_floor: CheckpointFloor,
        package_cache: Arc<PackageCache>,
    ) -> Self {
        Self {
            ledger_grpc_reader,
            ledger_grpc_floor,
            package_cache,
            interval: args.epoch_polling_interval(),
        }
    }

    /// Start a new task that regularly polls the ledger gRPC service for the latest epoch and
    /// evicts system packages if it detects that the epoch has changed (which means that a
    /// framework upgrade could have happened).
    ///
    /// This operation consumes the `self` and returns a Service handle.
    pub fn run(self) -> Service {
        Service::new().spawn_aborting(async move {
            let mut last_epoch: u64 = 0;
            let mut interval = time::interval(self.interval);

            loop {
                interval.tick().await;
                self.evict_on_new_epoch(&mut last_epoch).await;
            }
        })
    }

    /// Check the ledger gRPC service's latest epoch, and if it is past `last_epoch`, record it and
    /// evict system packages from the cache.
    async fn evict_on_new_epoch(&self, last_epoch: &mut u64) {
        let summary = match self.ledger_grpc_reader.checkpoint_watermark().await {
            Ok(summary) => summary,
            Err(e) => {
                warn!("Failed to fetch latest epoch: {e:#}");
                return;
            }
        };

        let next_epoch = summary.epoch;
        if next_epoch <= *last_epoch {
            return;
        }

        info!(
            last_epoch = *last_epoch,
            next_epoch, "Detected epoch boundary"
        );
        *last_epoch = next_epoch;

        self.ledger_grpc_floor.raise(summary.sequence_number);
        info!(system_packages = ?SYSTEM_PACKAGE_ADDRESSES, "Evicting...");
        self.package_cache
            .evict(SYSTEM_PACKAGE_ADDRESSES.iter().copied());
    }
}

impl Default for SystemPackageTaskArgs {
    fn default() -> Self {
        Self {
            epoch_polling_interval_ms: 10_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use move_binary_format::file_format::empty_module;
    use move_binary_format::file_format_common::VERSION_MAX;
    use move_core_types::account_address::AccountAddress;
    use sui_package_resolver::PackageStore;
    use sui_rpc::proto::sui::rpc::v2 as proto;
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::SequenceNumber;
    use sui_types::digests::CheckpointContentsDigest;
    use sui_types::digests::TransactionDigest;
    use sui_types::gas::GasCostSummary;
    use sui_types::messages_checkpoint::CheckpointSummary;
    use sui_types::move_package::MovePackage;
    use sui_types::object::Data;
    use sui_types::object::Object;

    use super::*;
    use crate::ledger_grpc_reader::test_support::mock_reader;
    use crate::package_resolver::GrpcPackageStore;

    fn checkpoint(epoch: u64, sequence_number: u64) -> CheckpointSummary {
        CheckpointSummary {
            epoch,
            sequence_number,
            network_total_transactions: 0,
            content_digest: CheckpointContentsDigest::new([0; 32]),
            previous_digest: None,
            epoch_rolling_gas_cost_summary: GasCostSummary::default(),
            timestamp_ms: 0,
            checkpoint_commitments: vec![],
            end_of_epoch_data: None,
            version_specific_data: vec![],
        }
    }

    /// A `BatchGetObjects` result holding a package at `id` with a single empty module (the
    /// resolver rejects packages without modules).
    fn package_result(id: AccountAddress) -> proto::GetObjectResult {
        let mut module = Vec::new();
        empty_module()
            .serialize_with_version(VERSION_MAX, &mut module)
            .unwrap();
        let package = MovePackage::new(
            ObjectID::from(id),
            SequenceNumber::from_u64(1),
            BTreeMap::from([("dummy".to_owned(), module)]),
            u64::MAX,
            vec![],
            BTreeMap::new(),
        )
        .unwrap();
        let object =
            Object::new_package_from_data(Data::Package(package), TransactionDigest::default());
        let mut proto_object = proto::Object::default();
        proto_object.bcs = Some(proto::Bcs::serialize(&object).unwrap());
        proto::GetObjectResult::new_object(proto_object)
    }

    #[tokio::test]
    async fn epoch_change_evicts_system_packages_and_raises_floor() {
        let (reader, mock, _server) = mock_reader().await;
        let floor = CheckpointFloor::default();
        let cache = Arc::new(PackageCache::new(GrpcPackageStore::new(&reader)));
        let task = SystemPackageTask::new(
            SystemPackageTaskArgs::default(),
            reader,
            floor.clone(),
            cache.clone(),
        );

        let mut last_epoch = 0;
        mock.set_checkpoint(checkpoint(0, 5));
        mock.set_object_results(vec![package_result(AccountAddress::ONE)]);

        // Within an epoch, the cache serves the system package without going back to the store.
        cache.fetch(AccountAddress::ONE).await.unwrap();
        task.evict_on_new_epoch(&mut last_epoch).await;
        cache.fetch(AccountAddress::ONE).await.unwrap();
        assert_eq!(mock.object_batches().len(), 1);
        assert_eq!(floor.get(), 0);

        // At an epoch boundary, where a framework upgrade may land, the next fetch goes back to the
        // store, with the floor raised to the checkpoint that revealed the new epoch.
        mock.set_checkpoint(checkpoint(1, 10));
        task.evict_on_new_epoch(&mut last_epoch).await;
        assert_eq!(floor.get(), 10);
        cache.fetch(AccountAddress::ONE).await.unwrap();
        assert_eq!(mock.object_batches().len(), 2);
    }
}
