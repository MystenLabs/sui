// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{load_package_by_id, parse_package_id};
use crate::reader::indexes_disabled;
use crate::{ErrorReason, Result, RpcError, RpcService};
use bytes::Bytes;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::{
    ListPackageVersionsRequest, ListPackageVersionsResponse, PackageVersion,
};
use sui_types::base_types::ObjectID;
use tap::Pipe;

/// Cursor for `ListPackageVersions` pagination. Public so other implementors of
/// `MovePackageService` mint wire-compatible page tokens.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PageToken {
    pub original_package_id: ObjectID,
    pub version: u64,
}

impl PageToken {
    pub fn decode(page_token: &[u8]) -> Result<Self> {
        decode_page_token(page_token)
    }

    pub fn encode(self) -> Bytes {
        encode_page_token(self)
    }
}

/// List the versions of a package's upgrade lineage published at or before the highest
/// checkpoint every rpc index has committed.
///
/// Versions with no recorded publish checkpoint are listed with `checkpoint` unset: they were
/// published before the history the index holds begins. A pruned node keeps the rows of the
/// versions it pruned and lists every row, including those versions. A node restored from a
/// snapshot, or a fork, has no rows for the versions replaced before its history begins, and
/// every listing starts at its lowest row.
#[tracing::instrument(skip(service))]
pub fn list_package_versions(
    service: &RpcService,
    request: ListPackageVersionsRequest,
) -> Result<ListPackageVersionsResponse> {
    let package_id_str = request.package_id.as_ref().ok_or_else(|| {
        FieldViolation::new("package_id")
            .with_description("missing package_id")
            .with_reason(ErrorReason::FieldMissing)
    })?;

    let package_id = parse_package_id(package_id_str)?;

    // While a restore is still backfilling history, a lineage upgraded inside the backfill window,
    // such as a system package's, lacks the versions published in the part of the window the
    // backfill has not reached, and its restored latest version has no recorded publish
    // checkpoint yet, so a listing could come back partial. Every listing waits for the
    // backfill: `at_checkpoint_upper_bound` is `Unavailable` until it finishes. The bound only
    // grows, so reading it before the rows keeps it valid for them.
    let upper_bound = service.reader.at_checkpoint_upper_bound()?;

    let original_package_id = load_package_by_id(service, package_id)?.original_package_id();

    let page_size = request
        .page_size
        .map(|s| (s as usize).clamp(1, 10000))
        .unwrap_or(1000);

    let page_token = request
        .page_token
        .as_deref()
        .map(decode_page_token)
        .transpose()?;

    if let Some(token) = &page_token
        && token.original_package_id != original_package_id
    {
        return Err(FieldViolation::new("page_token")
            .with_description("page token package ID does not match request package ID")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }

    let indexes = service
        .reader
        .inner()
        .indexes()
        .ok_or_else(indexes_disabled)?;

    // The listing stops at the highest indexed checkpoint, so it never includes a version
    // published above every checkpoint this node serves.
    let cp_bound = upper_bound.highest_indexed();
    let mut versions = vec![];
    let iter = indexes
        .package_versions_iter(original_package_id, page_token.as_ref().map(|t| t.version))
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

    for result in iter.take(page_size + 1) {
        let entry = result.map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

        // Recorded publish checkpoints are non-decreasing in version, so the first version
        // published after the bound ends the scan. Versions with no recorded checkpoint come
        // first and were published before the history the index holds begins.
        if let Some(published) = entry.checkpoint
            && published > cp_bound
        {
            break;
        }

        let mut version = PackageVersion::new(&entry.storage_id.into(), entry.version);
        version.checkpoint = entry.checkpoint;
        versions.push(version);
    }

    let next_page_token = if versions.len() > page_size {
        // SAFETY: We've already verified that versions is greater than page_size, which is
        // guaranteed to be >= 1.
        versions.pop().unwrap().pipe(|v| {
            v.version.map(|version| {
                encode_page_token(PageToken {
                    original_package_id,
                    version,
                })
            })
        })
    } else {
        None
    };

    Ok(ListPackageVersionsResponse::new(versions, next_page_token))
}

fn decode_page_token(page_token: &[u8]) -> Result<PageToken> {
    bcs::from_bytes(page_token).map_err(|e| {
        FieldViolation::new("page_token")
            .with_description(format!("invalid page token encoding: {e}"))
            .with_reason(ErrorReason::FieldInvalid)
            .into()
    })
}

fn encode_page_token(page_token: PageToken) -> Bytes {
    bcs::to_bytes(&page_token).unwrap().into()
}

/// Listings through the handler over an rpc-store, whose `package_versions` rows outlive the
/// package objects pruning deletes.
#[cfg(test)]
mod store_tests {
    use std::sync::Arc;

    use move_binary_format::file_format::empty_module;
    use sui_consistent_store::{ChainId, Db, FrameworkSchema, PipelineTaskKey};
    use sui_rpc_store::schema::{objects, package_versions, pruning_watermark};
    use sui_rpc_store::{RpcStoreReader, RpcStoreSchema};
    use sui_types::base_types::SequenceNumber;
    use sui_types::digests::TransactionDigest;
    use sui_types::move_package::MovePackage;
    use sui_types::object::{Data, Object};

    use super::*;
    use crate::reader::tests::commit_every_pipeline;

    /// The highest checkpoint every rpc index of the store has committed.
    const HIGHEST_INDEXED: u64 = 20;

    /// A system package upgraded in place: every version lives under this ID.
    fn system() -> ObjectID {
        ObjectID::from_single_byte(0xAB)
    }

    /// An rpc-store with every rpc index committed through [`HIGHEST_INDEXED`], holding versions
    /// `1..=3` of [`system`], published at checkpoints 2, 5 and 8, with their objects.
    struct Store {
        _dir: tempfile::TempDir,
        db: Db,
        schema: Arc<RpcStoreSchema>,
    }

    impl Store {
        fn with_system_package() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let (db, schema) =
                Db::open::<RpcStoreSchema>(dir.path(), sui_consistent_store::DbOptions::default())
                    .unwrap();
            let mut batch = db.batch();
            batch
                .put(
                    &FrameworkSchema::new(db.clone()).chain_ids,
                    &PipelineTaskKey::new("test"),
                    &ChainId([1; 32]),
                )
                .unwrap();
            for (version, checkpoint) in [(1, 2), (2, 5), (3, 8)] {
                let mut module = empty_module();
                module.address_identifiers[0] = system().into();
                let package = Object::new_package_from_data(
                    Data::Package(MovePackage::new_system(
                        SequenceNumber::from_u64(version),
                        &[module],
                        [],
                    )),
                    TransactionDigest::genesis_marker(),
                );
                let key = objects::Key {
                    id: system(),
                    version: package.version(),
                };
                batch
                    .put(&schema.objects, &key, &objects::store(&package))
                    .unwrap();
                let (key, value) = package_versions::store(system(), version, system(), checkpoint);
                batch.put(&schema.package_versions, &key, &value).unwrap();
            }
            batch.commit().unwrap();
            commit_every_pipeline(&db, HIGHEST_INDEXED);
            Self {
                _dir: dir,
                db,
                schema: Arc::new(schema),
            }
        }

        /// Prune object history below `lowest_available`, deleting `versions` of [`system`], as
        /// the pruner does: the floor rises before the objects are deleted.
        fn prune(&self, lowest_available: u64, versions: &[u64]) {
            let (key, value) = pruning_watermark::store(&pruning_watermark::Watermarks {
                tx_seq_lo: 100,
                checkpoint_lo: lowest_available,
            });
            let mut batch = self.db.batch();
            batch
                .put(&self.schema.pruning_watermark, &key, &value)
                .unwrap();
            batch.commit().unwrap();
            let mut batch = self.db.batch();
            for &version in versions {
                let key = objects::Key {
                    id: system(),
                    version: SequenceNumber::from_u64(version),
                };
                batch.delete(&self.schema.objects, &key).unwrap();
            }
            batch.commit().unwrap();
        }

        /// The versions `request` lists.
        fn list(&self, request: ListPackageVersionsRequest) -> Result<Vec<u64>> {
            let reader = RpcStoreReader::new(self.db.clone(), self.schema.clone());
            let service = RpcService::new(Arc::new(reader));
            let response = list_package_versions(&service, request)?;
            Ok(response
                .versions
                .iter()
                .map(|version| version.version.unwrap())
                .collect())
        }
    }

    /// A request listing [`system`]'s lineage from the page token naming `token`.
    fn lineage(token: Option<u64>) -> ListPackageVersionsRequest {
        let mut request = ListPackageVersionsRequest::default();
        request.package_id = Some(system().to_string());
        request.page_token = token.map(|version| {
            PageToken {
                original_package_id: system(),
                version,
            }
            .encode()
        });
        request
    }

    /// Once pruning deletes versions 1 and 2 but keeps their rows, a listing still lists every
    /// row, pruned or not.
    #[test]
    fn pruned_lineage_lists_every_row() {
        let store = Store::with_system_package();
        store.prune(10, &[1, 2]);

        for (request, expected) in [
            (lineage(None), vec![1, 2, 3]),
            (lineage(Some(2)), vec![2, 3]),
        ] {
            assert_eq!(
                store.list(request.clone()).unwrap(),
                expected,
                "{request:?}"
            );
        }
    }
}
