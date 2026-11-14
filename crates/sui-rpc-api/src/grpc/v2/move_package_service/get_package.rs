// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ErrorReason, Result, RpcError, RpcService,
    grpc::v2::move_package_service::{
        conversions::{convert_error, convert_module},
        load_package, load_package_by_id, load_package_by_id_and_version, parse_package_id,
    },
    reader::{NotReadyCause, indexes_disabled, not_ready},
};
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::get_package_request::Selector;
use sui_rpc::proto::sui::rpc::v2::{GetPackageRequest, GetPackageResponse, Package};
use sui_types::base_types::ObjectID;
use sui_types::move_package::MovePackage;
use sui_types::object::OBJECT_START_VERSION;

#[tracing::instrument(skip(service))]
pub fn get_package(service: &RpcService, request: GetPackageRequest) -> Result<GetPackageResponse> {
    let package_id_str = request.package_id.as_ref().ok_or_else(|| {
        FieldViolation::new("package_id")
            .with_description("missing package_id")
            .with_reason(ErrorReason::FieldMissing)
    })?;

    let package = match request.selector {
        Some(Selector::Version(version)) => {
            load_package_at_version(service, package_id_str, version)?
        }
        Some(Selector::AtCheckpoint(at_checkpoint)) => {
            load_package_at_checkpoint(service, package_id_str, at_checkpoint)?
        }
        None => load_package(service, package_id_str)?,
        _ => {
            return Err(FieldViolation::new("selector")
                .with_description("unknown selector variant")
                .with_reason(ErrorReason::FieldInvalid)
                .into());
        }
    };

    get_package_response(&package)
}

/// Load the package in `package_id_str`'s upgrade lineage with exactly
/// `version`.
///
/// System packages are upgraded in place: all their versions share one ID, so
/// an earlier version is read by that ID and the version, without the
/// package-version index. Nodes without object version history refuse that
/// read even while the version is still stored. Other packages keep each
/// version at its own storage ID, found through the package-version index,
/// whose rows are keyed by the lineage's original package id.
///
/// A version this node does not store is refused (see
/// [`refuse_unstored_version`]). Any other version the package-version index
/// has no row for is `NotFound`, unless the index is still backfilling history
/// after a restore and the row may yet be written.
fn load_package_at_version(
    service: &RpcService,
    package_id_str: &str,
    version: u64,
) -> Result<MovePackage> {
    let package = load_package(service, package_id_str)?;
    let original_id = package.original_package_id();
    if package.version().value() == version {
        return Ok(package);
    }
    // A lineage's versions run from `OBJECT_START_VERSION` up without gaps, so a version
    // below the loaded package's existed.
    let existed = (OBJECT_START_VERSION.value()..package.version().value()).contains(&version);

    // Only a package upgraded in place keeps its original ID past its first version, so the
    // loaded package is its lineage's latest version.
    if package.id() == original_id && package.version() > OBJECT_START_VERSION {
        if !existed {
            return Err(RpcError::not_found());
        }
        service.reader.require_object_version_history()?;
        return match load_package_by_id_and_version(service, original_id, version)? {
            Some(package) => Ok(package),
            None => refuse_unstored_version(service, original_id, version),
        };
    }

    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(indexes_disabled)?;

    let storage_id = indexes
        .get_package_version_storage_id(original_id, version)
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
    let stored = match storage_id {
        Some(storage_id) => load_package_by_id_and_version(service, storage_id, version)?,
        None if existed => None,
        // No lineage has a version below `OBJECT_START_VERSION`.
        None if version < OBJECT_START_VERSION.value() => return Err(RpcError::not_found()),
        None => {
            check_restore_backfill_done(service)?;
            return Err(RpcError::not_found());
        }
    };

    match stored {
        Some(package) => Ok(package),
        None => refuse_unstored_version(service, original_id, version),
    }
}

/// Refuse `version` of the package lineage `original_id`, which existed but this node does not
/// store. Nodes without object version history drop replaced versions at once. Otherwise the
/// refusal is not ready while the rpc indexes backfill history after a restore, pruned when this
/// node's object history has a floor (a node restored from a snapshot has none of the versions
/// replaced before it), and an internal error on a node that has pruned nothing, which stores
/// every version.
fn refuse_unstored_version(
    service: &RpcService,
    original_id: ObjectID,
    version: u64,
) -> Result<MovePackage> {
    service.reader.require_object_version_history()?;
    check_restore_backfill_done(service)?;
    let lower_bound = service.reader.at_checkpoint_lower_bound()?;
    if lower_bound.pruned_floor().is_none() {
        return Err(RpcError::new(
            tonic::Code::Internal,
            format!(
                "version {version} of package {original_id} is not stored, although this node \
                 has pruned no object history"
            ),
        ));
    }
    Err(lower_bound.pruned())
}

/// `UNAVAILABLE` (`NOT_READY`, cause `restore_backfill`) while the rpc indexes backfill history
/// after a restore, when an answer that a package version is missing may not hold once the
/// backfill ends.
fn check_restore_backfill_done(service: &RpcService) -> Result<()> {
    if let Some(indexes) = service.reader.inner().indexes()
        && indexes
            .is_history_backfill_pending()
            .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
    {
        return Err(not_ready(
            NotReadyCause::RestoreBackfill,
            "rpc index history is still backfilling after a restore",
        ));
    }
    Ok(())
}

/// Load the latest package in `package_id_str`'s upgrade lineage that
/// existed at or before `at_checkpoint`.
///
/// `at_checkpoint` is checked against the shared checkpoint-pinned read
/// bounds, the upper bound before the package read and the lower bound after
/// it (see `StateReader::at_checkpoint_upper_bound`), so a successful answer
/// is exact as of `at_checkpoint`. Both bounds apply to every outcome of the
/// read, `NotFound` included, so an out-of-range checkpoint is `OutOfRange`
/// whether or not the package exists, as on kv-rpc. Nodes without object
/// version history refuse: system packages are upgraded in place, so their
/// earlier versions are gone there.
fn load_package_at_checkpoint(
    service: &RpcService,
    package_id_str: &str,
    at_checkpoint: u64,
) -> Result<MovePackage> {
    let package_id = parse_package_id(package_id_str)?;

    service.reader.require_object_version_history()?;
    service
        .reader
        .at_checkpoint_upper_bound()?
        .check(at_checkpoint)?;

    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(indexes_disabled)?;

    let package = load_package_by_id(service, package_id).and_then(|package| {
        indexes
            .get_package_at_checkpoint(package.original_package_id(), at_checkpoint)
            .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
            .ok_or_else(RpcError::not_found)
            .and_then(|(version, storage_id)| {
                // The lower bound below replaces any answer at a checkpoint it refuses; at the
                // others, pruning has not reached the version current at `at_checkpoint`.
                load_package_by_id_and_version(service, storage_id, version)?.ok_or_else(|| {
                    RpcError::new(
                        tonic::Code::Internal,
                        format!(
                            "package {storage_id} version {version} has a package-version index \
                             row but no stored object"
                        ),
                    )
                })
            })
    });

    service
        .reader
        .at_checkpoint_lower_bound()?
        .check(at_checkpoint)?;
    package
}

/// Build a `GetPackage` response from an already-loaded package. Shared with other implementors
/// of `MovePackageService` (e.g. sui-kv-rpc) that load packages from a different backend.
pub fn get_package_response(package: &MovePackage) -> Result<GetPackageResponse> {
    let package_id = package.id();

    let resolved_package =
        sui_package_resolver::Package::read_from_package(package).map_err(convert_error)?;

    let modules: Vec<_> = resolved_package
        .modules()
        .iter()
        .map(|(module_name, resolver_module)| {
            convert_module(module_name, resolver_module, &package_id)
        })
        .collect::<Result<Vec<_>>>()?;

    let mut message = Package::default();
    message.storage_id = Some(package_id.to_canonical_string(true));
    message.original_id = Some(package.original_package_id().to_canonical_string(true));
    message.version = Some(package.version().value());
    message.modules = modules;

    Ok(GetPackageResponse::new(message))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use move_binary_format::file_format::{CompiledModule, empty_module};
    use sui_consistent_store::{ChainId, Db, FrameworkSchema, PipelineTaskKey, restore_state};
    use sui_protocol_config::ProtocolConfig;
    use sui_rpc_store::RpcStoreSchema;
    use sui_rpc_store::schema::{objects, package_versions, pruning_watermark};
    use sui_types::base_types::SequenceNumber;
    use sui_types::digests::TransactionDigest;
    use sui_types::object::{Data, Object};

    use super::*;
    use crate::ledger_history::query_options::assert_below_serving_floor_error;
    use crate::reader::tests::{assert_refusal, commit_every_pipeline, record_restore_state};

    /// An `RpcService` over an rpc-store holding `packages` and the package-version index rows
    /// `(original ID, version, storage ID)` in `index_rows`.
    struct Fixture {
        _dir: tempfile::TempDir,
        db: Db,
        schema: Arc<RpcStoreSchema>,
        service: RpcService,
    }

    impl Fixture {
        fn new(packages: &[Object], index_rows: &[(ObjectID, u64, ObjectID)]) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let (db, schema) =
                Db::open::<RpcStoreSchema>(dir.path(), sui_consistent_store::DbOptions::default())
                    .unwrap();
            let schema = Arc::new(schema);

            let mut batch = db.batch();
            for package in packages {
                let key = objects::Key {
                    id: package.id(),
                    version: package.version(),
                };
                batch
                    .put(&schema.objects, &key, &objects::store(package))
                    .unwrap();
            }
            for &(original_id, version, storage_id) in index_rows {
                let (key, value) = package_versions::store(original_id, version, storage_id, 0);
                batch.put(&schema.package_versions, &key, &value).unwrap();
            }
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
            Self {
                _dir: dir,
                db,
                schema,
                service,
            }
        }

        fn get_version(&self, package_id: ObjectID, version: u64) -> Result<GetPackageResponse> {
            let mut request = GetPackageRequest::default();
            request.package_id = Some(package_id.to_string());
            request.selector = Some(Selector::Version(version));
            get_package(&self.service, request)
        }

        fn assert_served(&self, package_id: ObjectID, version: u64) {
            let package = self
                .get_version(package_id, version)
                .unwrap()
                .package
                .unwrap();
            assert_eq!(package.version, Some(version));
        }

        fn assert_code(&self, package_id: ObjectID, version: u64, code: tonic::Code) {
            let status: tonic::Status = self.get_version(package_id, version).unwrap_err().into();
            assert_eq!(status.code(), code, "{package_id} v{version}: {status:?}");
        }

        fn assert_restore_backfill(&self, package_id: ObjectID, version: u64) {
            assert_refusal(
                self.get_version(package_id, version).unwrap_err(),
                tonic::Code::Unavailable,
                ErrorReason::NotReady,
                "restore_backfill",
            );
        }

        fn start_restore_backfill(&self) {
            record_restore_state(
                &self.db,
                &["object_version_by_checkpoint"],
                restore_state::State::InProgress(restore_state::InProgress::default()),
            );
        }

        fn finish_restore_backfill(&self) {
            record_restore_state(
                &self.db,
                &["object_version_by_checkpoint"],
                restore_state::State::Complete(restore_state::Complete { restored_at: 20 }),
            );
            commit_every_pipeline(&self.db, 20);
        }

        /// Prune object history below `lowest_available`.
        fn prune(&self, lowest_available: u64) {
            let (key, value) = pruning_watermark::store(&pruning_watermark::Watermarks {
                tx_seq_lo: 100,
                checkpoint_lo: lowest_available,
            });
            let mut batch = self.db.batch();
            batch
                .put(&self.schema.pruning_watermark, &key, &value)
                .unwrap();
            batch.commit().unwrap();
        }
    }

    /// A module whose self-address, and so its package's original ID, is `original_id`.
    fn module(original_id: ObjectID) -> CompiledModule {
        let mut module = empty_module();
        module.address_identifiers[0] = original_id.into();
        module
    }

    /// A system package upgraded in place to version 3 whose version 2 is not stored, while a
    /// restore's history backfill is pending, after it completes, and once the node has pruned.
    /// A stored replaced version is served throughout; the missing one is refused as not ready,
    /// then as an internal error on a node that has pruned nothing, then as pruned.
    #[test]
    fn replaced_system_package_version_is_read_before_it_is_refused() {
        let package_id = ObjectID::from_single_byte(0xAB);
        let packages: Vec<_> = [1, 3]
            .into_iter()
            .map(|version| {
                Object::new_package_from_data(
                    Data::Package(MovePackage::new_system(
                        SequenceNumber::from_u64(version),
                        &[module(package_id)],
                        [],
                    )),
                    TransactionDigest::genesis_marker(),
                )
            })
            .collect();
        let fixture = Fixture::new(&packages, &[]);

        fixture.start_restore_backfill();
        fixture.assert_served(package_id, 1);
        fixture.assert_served(package_id, 3);
        fixture.assert_restore_backfill(package_id, 2);

        fixture.finish_restore_backfill();
        fixture.assert_served(package_id, 1);
        fixture.assert_code(package_id, 2, tonic::Code::Internal);

        fixture.prune(10);
        fixture.assert_served(package_id, 1);
        assert_below_serving_floor_error(fixture.get_version(package_id, 2).unwrap_err(), 9);

        // Versions this lineage never had.
        fixture.assert_code(package_id, 0, tonic::Code::NotFound);
        fixture.assert_code(package_id, 4, tonic::Code::NotFound);
    }

    /// A user package lineage `v1` at `v1_id`, `v2` at `v2_id` and `v3` at `v3_id`, read through
    /// the package-version index while it lacks `v3`'s row and the node lacks `v2`'s object. A
    /// version without an index row is refused as not ready during a restore's history backfill
    /// and is `NotFound` after it, except version 0, which no lineage has and is always
    /// `NotFound`; an indexed version without its object is refused as not ready, then as an
    /// internal error on a node that has pruned nothing, then as pruned.
    #[test]
    fn user_package_version_missing_from_index_or_store_is_refused() {
        let v1_id = ObjectID::from_single_byte(0xA1);
        let v2_id = ObjectID::from_single_byte(0xB2);
        let v3_id = ObjectID::from_single_byte(0xC3);
        let config = ProtocolConfig::get_for_max_version_UNSAFE();
        let upgrade = |previous: &Object, storage_id| {
            let Data::Package(previous) = &previous.data else {
                unreachable!("packages only");
            };
            let upgraded = previous
                .new_upgraded(storage_id, &[module(v1_id)], &config, [])
                .unwrap();
            Object::new_package_from_data(
                Data::Package(upgraded),
                TransactionDigest::genesis_marker(),
            )
        };
        let v1 = Object::new_package(
            &[module(v1_id)],
            TransactionDigest::genesis_marker(),
            &config,
            [],
        )
        .unwrap();
        let v2 = upgrade(&v1, v2_id);
        let v3 = upgrade(&v2, v3_id);
        let fixture = Fixture::new(&[v1, v3], &[(v1_id, 1, v1_id), (v1_id, 2, v2_id)]);

        fixture.start_restore_backfill();
        fixture.assert_served(v3_id, 1);
        fixture.assert_restore_backfill(v3_id, 2);
        fixture.assert_restore_backfill(v1_id, 3);
        fixture.assert_code(v3_id, 0, tonic::Code::NotFound);

        fixture.finish_restore_backfill();
        fixture.assert_served(v3_id, 1);
        fixture.assert_code(v3_id, 2, tonic::Code::Internal);
        fixture.assert_code(v1_id, 3, tonic::Code::NotFound);

        fixture.prune(10);
        assert_below_serving_floor_error(fixture.get_version(v3_id, 2).unwrap_err(), 9);
    }
}
