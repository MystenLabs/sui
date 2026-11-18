// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::reader::indexes_disabled;
use crate::{Result, RpcError, RpcService};
use sui_rpc::proto::sui::rpc::v2::{
    ListSystemPackagesRequest, ListSystemPackagesResponse, PackageVersion,
};
use sui_types::{SYSTEM_PACKAGE_ADDRESSES, base_types::ObjectID, storage::RpcIndexes};

/// List the system packages that existed as of the end of `at_checkpoint`, each at its latest
/// version as of that checkpoint, ordered by package id. Without `at_checkpoint`, the request is
/// answered exactly as if it named the highest checkpoint every rpc index has committed: each
/// version's publish checkpoint comes from the `package_versions` index, so the answer is at a
/// height that index has fully recorded.
///
/// The answer is the complete set or an error: `at_checkpoint` is checked against the shared
/// checkpoint-pinned read bounds, the upper bound before the reads and the lower bound after
/// them (see `StateReader::at_checkpoint_upper_bound`), so a checkpoint outside the indexed range
/// fails the whole request. Either way, a node without rpc indexes refuses, as does one whose
/// indexes are empty or still backfilling history after a restore.
///
/// The candidates are the system package ids compiled into this binary. A fullnode cannot execute
/// past a protocol upgrade it does not know, so every system package in the data it serves is in
/// its compiled list. Only the rpc indexes are read, never object contents.
#[tracing::instrument(skip(service))]
pub fn list_system_packages(
    service: &RpcService,
    request: ListSystemPackagesRequest,
) -> Result<ListSystemPackagesResponse> {
    let upper_bound = service.reader.at_checkpoint_upper_bound()?;
    let at_checkpoint = request
        .at_checkpoint
        .unwrap_or_else(|| upper_bound.highest_indexed());
    upper_bound.check(at_checkpoint)?;

    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(indexes_disabled)?;

    let system_packages = system_package_versions(indexes, at_checkpoint);

    service
        .reader
        .at_checkpoint_lower_bound()?
        .check(at_checkpoint)?;
    system_packages.map(ListSystemPackagesResponse::new)
}

/// The version of each compiled system package as of the end of `at_checkpoint`, ordered by
/// package id, omitting packages that did not exist yet. Each version carries the checkpoint it
/// was published in, unset when the indexes' history does not reach back to it.
fn system_package_versions(
    indexes: &dyn RpcIndexes,
    at_checkpoint: u64,
) -> Result<Vec<PackageVersion>> {
    let mut package_ids: Vec<ObjectID> = SYSTEM_PACKAGE_ADDRESSES
        .iter()
        .copied()
        .map(ObjectID::from)
        .collect();
    package_ids.sort();

    let mut system_packages = Vec::with_capacity(package_ids.len());
    for package_id in package_ids {
        // System packages are never deleted or wrapped, so a resolved version is never a
        // tombstone: it is the live package.
        let Some(version) = indexes
            .get_object_version_at_checkpoint(package_id, at_checkpoint)
            .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        else {
            continue;
        };
        let version = version.value();

        // A system package is its own original package, so its versions are listed under its id.
        let published = indexes
            .package_versions_iter(package_id, version, version.saturating_add(1), false)
            .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
            .next()
            .transpose()
            .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
            .and_then(|entry| entry.checkpoint);

        let mut entry = PackageVersion::new(&package_id.into(), version)
            .with_original_id(package_id.to_canonical_string(true));
        entry.checkpoint = published;
        system_packages.push(entry);
    }
    Ok(system_packages)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use move_binary_format::file_format::{CompiledModule, empty_module};
    use sui_consistent_store::{ChainId, Db, FrameworkSchema, PipelineTaskKey, restore_state};
    use sui_rpc_store::RpcStoreSchema;
    use sui_rpc_store::schema::{object_version_by_checkpoint, objects, package_versions};
    use sui_types::base_types::SequenceNumber;
    use sui_types::digests::TransactionDigest;
    use sui_types::move_package::MovePackage;
    use sui_types::object::{Data, Object};
    use sui_types::{MOVE_STDLIB_PACKAGE_ID, SUI_FRAMEWORK_PACKAGE_ID, SUI_SYSTEM_PACKAGE_ID};

    use super::*;
    use crate::ErrorReason;
    use crate::reader::tests::{assert_refusal, commit_every_pipeline, record_restore_state};

    /// An `RpcService` over an rpc-store holding version 1 of `0x1`, `0x2` and `0x3`, each
    /// indexed as published at checkpoint 0, whose pipelines have committed nothing yet.
    struct Fixture {
        _dir: tempfile::TempDir,
        db: Db,
        schema: Arc<RpcStoreSchema>,
        service: RpcService,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let (db, schema) =
                Db::open::<RpcStoreSchema>(dir.path(), sui_consistent_store::DbOptions::default())
                    .unwrap();
            let schema = Arc::new(schema);
            // `RpcService::new` reads the chain id.
            let mut batch = db.batch();
            batch
                .put(
                    &FrameworkSchema::new(db.clone()).chain_ids,
                    &PipelineTaskKey::new("test"),
                    &ChainId([1; 32]),
                )
                .unwrap();
            batch.commit().unwrap();
            let service = RpcService::new(Arc::new(sui_rpc_store::RpcStoreReader::new(
                db.clone(),
                schema.clone(),
            )));
            let fixture = Self {
                _dir: dir,
                db,
                schema,
                service,
            };

            for package_id in [
                MOVE_STDLIB_PACKAGE_ID,
                SUI_FRAMEWORK_PACKAGE_ID,
                SUI_SYSTEM_PACKAGE_ID,
            ] {
                fixture.store_package(package_id, 1);
                fixture.index_package(package_id, 1, 0);
            }
            fixture
        }

        /// Stores `version` of system package `package_id` in the objects table, as executing
        /// the transaction that wrote it does.
        fn store_package(&self, package_id: ObjectID, version: u64) {
            let mut module: CompiledModule = empty_module();
            module.address_identifiers[0] = package_id.into();
            let package = Object::new_package_from_data(
                Data::Package(MovePackage::new_system(
                    SequenceNumber::from_u64(version),
                    &[module],
                    [],
                )),
                TransactionDigest::genesis_marker(),
            );
            let key = objects::Key {
                id: package.id(),
                version: package.version(),
            };
            let mut batch = self.db.batch();
            batch
                .put(&self.schema.objects, &key, &objects::store(&package))
                .unwrap();
            batch.commit().unwrap();
        }

        /// Writes the index rows recording `version` of system package `package_id` as published
        /// at `checkpoint`. Reads see them once the pipelines commit `checkpoint`.
        fn index_package(&self, package_id: ObjectID, version: u64, checkpoint: u64) {
            let (key, value) = package_versions::store(package_id, version, package_id, checkpoint);
            let (version_key, version_value) = object_version_by_checkpoint::store(
                package_id,
                checkpoint,
                SequenceNumber::from_u64(version),
            );
            let mut batch = self.db.batch();
            batch
                .put(&self.schema.package_versions, &key, &value)
                .unwrap();
            batch
                .put(
                    &self.schema.object_version_by_checkpoint,
                    &version_key,
                    &version_value,
                )
                .unwrap();
            batch.commit().unwrap();
        }

        fn list(&self, at_checkpoint: Option<u64>) -> Result<Vec<PackageVersion>> {
            let request = at_checkpoint.map_or_else(
                ListSystemPackagesRequest::default,
                ListSystemPackagesRequest::new,
            );
            list_system_packages(&self.service, request).map(|response| response.system_packages)
        }
    }

    fn system_package_version(
        package_id: ObjectID,
        version: u64,
        published: u64,
    ) -> PackageVersion {
        let mut entry = PackageVersion::new(&package_id.into(), version)
            .with_original_id(package_id.to_canonical_string(true));
        entry.checkpoint = Some(published);
        entry
    }

    /// Without `at_checkpoint`, the answer is the one for the highest checkpoint every index has
    /// committed. A newer version that execution has written, and whose index rows are written
    /// above that checkpoint, is not listed until the indexes commit its checkpoint.
    #[test]
    fn omitted_checkpoint_answers_at_the_highest_indexed_checkpoint() {
        let fixture = Fixture::new();
        commit_every_pipeline(&fixture.db, 20);
        let genesis = vec![
            system_package_version(MOVE_STDLIB_PACKAGE_ID, 1, 0),
            system_package_version(SUI_FRAMEWORK_PACKAGE_ID, 1, 0),
            system_package_version(SUI_SYSTEM_PACKAGE_ID, 1, 0),
        ];
        assert_eq!(fixture.list(None).unwrap(), genesis);
        assert_eq!(fixture.list(Some(20)).unwrap(), genesis);

        // A framework upgrade at checkpoint 21: executed and partly indexed, but not committed.
        fixture.store_package(SUI_SYSTEM_PACKAGE_ID, 2);
        fixture.index_package(SUI_SYSTEM_PACKAGE_ID, 2, 21);
        assert_eq!(fixture.list(None).unwrap(), genesis);
        assert_eq!(fixture.list(Some(20)).unwrap(), genesis);

        commit_every_pipeline(&fixture.db, 21);
        let upgraded = vec![
            system_package_version(MOVE_STDLIB_PACKAGE_ID, 1, 0),
            system_package_version(SUI_FRAMEWORK_PACKAGE_ID, 1, 0),
            system_package_version(SUI_SYSTEM_PACKAGE_ID, 2, 21),
        ];
        assert_eq!(fixture.list(None).unwrap(), upgraded);
        assert_eq!(fixture.list(Some(21)).unwrap(), upgraded);
        assert_eq!(fixture.list(Some(20)).unwrap(), genesis);
    }

    /// Before the indexes commit a checkpoint, a request without `at_checkpoint` is refused as
    /// not ready.
    #[test]
    fn omitted_checkpoint_is_not_ready_while_the_index_is_empty() {
        let fixture = Fixture::new();
        assert_refusal(
            fixture.list(None).unwrap_err(),
            tonic::Code::Unavailable,
            ErrorReason::NotReady,
            "index_empty",
        );
    }

    /// While a restore's history backfill is pending, the indexes may lack the publish checkpoint
    /// of a version they hold, so a request without `at_checkpoint` is refused as not ready.
    #[test]
    fn omitted_checkpoint_is_not_ready_during_a_restore_backfill() {
        let fixture = Fixture::new();
        commit_every_pipeline(&fixture.db, 20);
        record_restore_state(
            &fixture.db,
            &["object_version_by_checkpoint"],
            restore_state::State::InProgress(restore_state::InProgress::default()),
        );
        assert_refusal(
            fixture.list(None).unwrap_err(),
            tonic::Code::Unavailable,
            ErrorReason::NotReady,
            "restore_backfill",
        );
    }
}
