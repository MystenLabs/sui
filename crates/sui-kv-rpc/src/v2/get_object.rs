// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::StreamExt;
use futures::stream;
use futures::stream::BoxStream;
use std::collections::HashMap;
use sui_kvstore::KeyValueStoreReader;
use sui_kvstore::ObjectStatus;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsResponse;
use sui_rpc::proto::sui::rpc::v2::Object as ProtoObject;
use sui_rpc::proto::sui::rpc::v2::{GetObjectRequest, GetObjectResponse, GetObjectResult};
use sui_rpc_api::grpc::v2::ledger_service::{ObjectSelector, validate_get_object_requests};
use sui_rpc_api::{ObjectNotFoundError, RpcError};
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::object::Object;
use sui_types::storage::ObjectKey;

use crate::KvRpcServer;
use crate::PackageResolver;
use crate::bigtable_client::BigTableClient;
use crate::config::{PipelineStage, ResolvedStageConfig};
use crate::pipeline::{SeekUnit, Watermarked, pipelined_chunks};
use crate::render::needs_only_object_id_and_version;
use crate::render::object_to_response;
use crate::render::object_version_to_response;
use crate::served_height::ServedHeight;

pub const MAX_BATCH_REQUESTS: usize = 1000;
/// Most per-object scans a batch runs at once, for each kind of scan: latest-version lookups,
/// and `at_checkpoint` version lookups.
pub const MAX_CONCURRENT_UNVERSIONED_SCANS: usize = 50;

pub(crate) async fn get_object(
    server: &KvRpcServer,
    GetObjectRequest {
        object_id,
        version,
        read_mask,
        at_checkpoint,
        ..
    }: GetObjectRequest,
) -> Result<GetObjectResponse, RpcError> {
    let (requests, read_mask) =
        validate_get_object_requests(vec![(object_id, version, at_checkpoint)], read_mask)?;
    let (address, selector) = requests[0];
    let object_id: ObjectID = address.into();
    let version_only = needs_only_object_id_and_version(&read_mask);
    let mut client = server.client.clone();
    let found = match selector {
        ObjectSelector::Latest => read_latest(&mut client, &object_id, version_only)
            .await?
            .ok_or_else(|| ObjectNotFoundError::new(address))?,
        ObjectSelector::Version(version) => read_exact_version(
            &mut client,
            ObjectKey(object_id, version.into()),
            version_only,
        )
        .await?
        .ok_or_else(|| ObjectNotFoundError::new_with_version(address, version))?,
        ObjectSelector::AtCheckpoint(checkpoint) => {
            let reads = server.at_checkpoint_reads(server.limited_client("GetObject"))?;
            if checkpoint > reads.served_height.checkpoint {
                return Err(not_yet_indexed(checkpoint, reads.served_height));
            }
            let not_found = || ObjectNotFoundError::new_at_checkpoint(address, checkpoint);
            let version = reads
                .live_version_at_checkpoint(object_id, checkpoint)
                .await?
                .ok_or_else(not_found)?;
            // A live version's row is in the objects table, so the index answers a
            // version-only read without touching it.
            if version_only {
                Found::Version(version)
            } else {
                read_exact_version(&mut client, ObjectKey(object_id, version), false)
                    .await?
                    .ok_or_else(not_found)?
            }
        }
    };
    let message = found
        .resolved()
        .render(object_id, &read_mask, &server.package_resolver)
        .await;
    Ok(GetObjectResponse::new(message))
}

/// Serve a batch in two stages of reads, plus the latest-version scans running alongside them:
///
/// 1. One `object_versions_by_checkpoint` scan per distinct `(object_id, at_checkpoint)`, each
///    its own gated call, because BigTable cannot return the newest row of several ranges in one
///    `ReadRows` (`rows_limit` covers the whole request).
/// 2. `objects` multi-gets for the exact-version keys and the live versions stage 1 found,
///    planned by the `objects` stage ([`crate::config::StagesConfig::objects`]): reads of at most
///    `max-seeks-per-read` keys, at most `concurrency` of them at a time.
///
/// Both stages share one request-scoped limiter; the latest-version scans do not use it.
///
/// When the read mask asks for nothing but `object_id` and `version`, stage 1 answers the
/// `at_checkpoint` items on its own, and the `objects` reads that remain (stage 2 and the latest
/// scans) fetch only row keys, not object contents.
pub(crate) async fn batch_get_objects(
    server: &KvRpcServer,
    BatchGetObjectsRequest {
        requests,
        read_mask,
        ..
    }: BatchGetObjectsRequest,
) -> Result<BatchGetObjectsResponse, RpcError> {
    if requests.len() > MAX_BATCH_REQUESTS {
        return Err(RpcError::new(
            tonic::Code::InvalidArgument,
            format!("number of batch requests exceed limit of {MAX_BATCH_REQUESTS}"),
        ));
    }
    let requests = requests
        .into_iter()
        .map(|req| (req.object_id, req.version, req.at_checkpoint))
        .collect();
    let (requests, read_mask) = validate_get_object_requests(requests, read_mask)?;

    let version_only = needs_only_object_id_and_version(&read_mask);
    let mut exact_object_keys = Vec::new();
    let mut unversioned_object_ids = Vec::new();
    let mut at_checkpoint_keys = Vec::new();
    for (address, selector) in &requests {
        let object_id: ObjectID = (*address).into();
        match *selector {
            ObjectSelector::Latest => unversioned_object_ids.push(object_id),
            ObjectSelector::Version(version) => {
                exact_object_keys.push(ObjectKey(object_id, version.into()))
            }
            ObjectSelector::AtCheckpoint(checkpoint) => {
                at_checkpoint_keys.push((object_id, checkpoint))
            }
        }
    }

    exact_object_keys.sort();
    exact_object_keys.dedup();
    unversioned_object_ids.sort();
    unversioned_object_ids.dedup();
    at_checkpoint_keys.sort();
    at_checkpoint_keys.dedup();

    // Stage 1 and stage 2 read through this client, under one request-wide limiter. It is
    // created whether or not `at_checkpoint` reads are enabled, because stage 2 always uses it.
    let limited_client = server.limited_client("BatchGetObjects");
    let objects_stage = server.stages.stage(PipelineStage::Objects);
    let at_checkpoint_reads = if at_checkpoint_keys.is_empty() {
        None
    } else {
        Some(server.at_checkpoint_reads(limited_client.clone())?)
    };
    let client = server.client.clone();

    let versioned_future = async {
        // Stage 1: resolve each servable `(object_id, at_checkpoint)` to its live version.
        let mut live_versions_at_checkpoint = HashMap::new();
        if let Some(reads) = &at_checkpoint_reads {
            let scans = at_checkpoint_keys
                .into_iter()
                .filter(|&(_, checkpoint)| checkpoint <= reads.served_height.checkpoint)
                .map(|(object_id, checkpoint)| async move {
                    let version = reads
                        .live_version_at_checkpoint(object_id, checkpoint)
                        .await?;
                    Ok::<_, RpcError>(((object_id, checkpoint), version))
                });
            let results = futures::stream::iter(scans)
                .buffer_unordered(MAX_CONCURRENT_UNVERSIONED_SCANS)
                .collect::<Vec<_>>()
                .await;
            for result in results {
                if let (key, Some(version)) = result? {
                    live_versions_at_checkpoint.insert(key, version);
                }
            }
        }

        // Stage 2: the exact versions, and the live versions found above unless the mask needs
        // only their ids and versions.
        let mut object_keys = exact_object_keys;
        if !version_only {
            object_keys.extend(
                live_versions_at_checkpoint
                    .iter()
                    .map(|(&(object_id, _), &version)| ObjectKey(object_id, version)),
            );
        }
        object_keys.sort();
        object_keys.dedup();
        let found_by_key =
            read_exact_versions(&limited_client, objects_stage, object_keys, version_only).await?;
        Ok::<_, RpcError>((live_versions_at_checkpoint, found_by_key))
    };

    let unversioned_future = async {
        let unversioned_futures = unversioned_object_ids.into_iter().map(|object_id| {
            let mut client = client.clone();
            async move {
                let found = read_latest(&mut client, &object_id, version_only).await?;
                Ok::<_, RpcError>((object_id, found))
            }
        });
        let unversioned_results = futures::stream::iter(unversioned_futures)
            .buffer_unordered(MAX_CONCURRENT_UNVERSIONED_SCANS)
            .collect::<Vec<_>>()
            .await;

        let mut latest_found = HashMap::new();
        for result in unversioned_results {
            if let (object_id, Some(found)) = result? {
                latest_found.insert(object_id, found);
            }
        }
        Ok::<_, RpcError>(latest_found)
    };

    let ((live_versions_at_checkpoint, found_by_key), latest_found) =
        tokio::try_join!(versioned_future, unversioned_future)?;

    let mut objects = Vec::with_capacity(requests.len());
    for (address, selector) in requests {
        let object_id: ObjectID = address.into();
        let resolved = match selector {
            ObjectSelector::Latest => latest_found
                .get(&object_id)
                .map(Found::resolved)
                .ok_or_else(|| ObjectNotFoundError::new(address).into()),
            ObjectSelector::Version(version) => found_by_key
                .get(&ObjectKey(object_id, version.into()))
                .map(Found::resolved)
                .ok_or_else(|| ObjectNotFoundError::new_with_version(address, version).into()),
            ObjectSelector::AtCheckpoint(checkpoint) => match &at_checkpoint_reads {
                Some(reads) if checkpoint > reads.served_height.checkpoint => {
                    Err(not_yet_indexed(checkpoint, reads.served_height))
                }
                _ => live_versions_at_checkpoint
                    .get(&(object_id, checkpoint))
                    .and_then(|&version| {
                        if version_only {
                            Some(Resolved::Version(version))
                        } else {
                            found_by_key
                                .get(&ObjectKey(object_id, version))
                                .map(Found::resolved)
                        }
                    })
                    .ok_or_else(|| {
                        ObjectNotFoundError::new_at_checkpoint(address, checkpoint).into()
                    }),
            },
        };
        match resolved {
            Ok(resolved) => {
                let message = resolved
                    .render(object_id, &read_mask, &server.package_resolver)
                    .await;
                objects.push(GetObjectResult::new_object(message));
            }
            Err(err) => objects.push(GetObjectResult::new_error(err.into_status_proto())),
        }
    }
    Ok(BatchGetObjectsResponse::new(objects))
}

/// What an `objects` table read returned: the object, or, for a read mask that needs only
/// `object_id` and `version`, just the version, read from the row key without the contents.
enum Found {
    Object(Object),
    Version(SequenceNumber),
}

impl Found {
    fn resolved(&self) -> Resolved<'_> {
        match self {
            Found::Object(object) => Resolved::Object(object),
            Found::Version(version) => Resolved::Version(*version),
        }
    }
}

/// What a requested object resolved to: a [`Found`] it shares with other requests for the same
/// object, or an `at_checkpoint` item's version straight from the version index.
enum Resolved<'a> {
    Object(&'a Object),
    Version(SequenceNumber),
}

impl Resolved<'_> {
    async fn render(
        self,
        object_id: ObjectID,
        read_mask: &FieldMaskTree,
        resolver: &PackageResolver,
    ) -> ProtoObject {
        match self {
            Resolved::Object(object) => object_to_response(object, read_mask, resolver).await,
            Resolved::Version(version) => object_version_to_response(object_id, version, read_mask),
        }
    }
}

/// Read the newest version of `object_id` in the `objects` table: the whole object, or with
/// `version_only` just its version.
async fn read_latest(
    client: &mut sui_kvstore::BigTableClient,
    object_id: &ObjectID,
    version_only: bool,
) -> Result<Option<Found>, RpcError> {
    Ok(if version_only {
        client
            .get_latest_object_version(object_id)
            .await?
            .map(Found::Version)
    } else {
        client
            .get_latest_object(object_id)
            .await?
            .map(Found::Object)
    })
}

/// Read one exact version from the `objects` table: the whole object, or with `version_only`
/// just whether the object is live at it.
async fn read_exact_version(
    client: &mut sui_kvstore::BigTableClient,
    key: ObjectKey,
    version_only: bool,
) -> Result<Option<Found>, RpcError> {
    Ok(if version_only {
        client
            .get_existing_object_keys(&[key])
            .await?
            .into_iter()
            .next()
            .map(|key| Found::Version(key.1))
    } else {
        client
            .get_objects(&[key])
            .await?
            .into_iter()
            .next()
            .map(Found::Object)
    })
}

/// Read exact versions from the `objects` table through the request's gated `client`, in
/// multi-gets planned by `objects_stage`, each under one permit: the whole objects, or with
/// `version_only` just which of `keys` are live. Keys that are not found are absent from the map.
async fn read_exact_versions(
    client: &BigTableClient,
    objects_stage: ResolvedStageConfig,
    keys: Vec<ObjectKey>,
    version_only: bool,
) -> Result<HashMap<ObjectKey, Found>, RpcError> {
    if keys.is_empty() {
        return Ok(HashMap::new());
    }
    let mut found_by_key = HashMap::with_capacity(keys.len());
    let key_stream = stream::iter(
        keys.into_iter()
            .map(|key| Ok::<Watermarked<ObjectKey>, RpcError>(Watermarked::Item(key))),
    )
    .boxed();
    let client = client.clone();
    let mut reads = pipelined_chunks(
        key_stream,
        objects_stage.max_seeks_per_read,
        objects_stage.concurrency,
        objects_stage.min_seeks_per_split_read,
        SeekUnit::Item,
        move |chunk: Vec<ObjectKey>| {
            let client = client.clone();
            async move {
                let rows: BoxStream<'static, Result<(ObjectKey, Found), RpcError>> = if version_only
                {
                    let existing = client.get_existing_object_keys(chunk).await?;
                    stream::iter(
                        existing
                            .into_iter()
                            .map(|key| Ok((key, Found::Version(key.1)))),
                    )
                    .boxed()
                } else {
                    client
                        .get_objects_stream(chunk)
                        .await?
                        .map(|row| {
                            row.map(|object| {
                                (
                                    ObjectKey(object.id(), object.version()),
                                    Found::Object(object),
                                )
                            })
                            .map_err(RpcError::from)
                        })
                        .boxed()
                };
                Ok::<_, RpcError>(rows)
            }
        },
    );
    while let Some(frame) = reads.next().await {
        if let Watermarked::Item((key, found)) = frame? {
            found_by_key.insert(key, found);
        }
    }
    Ok(found_by_key)
}

/// What a request's `at_checkpoint` reads run with: the request-scoped gated client for their
/// `object_versions_by_checkpoint` scans, and the served height that bounds them.
struct AtCheckpointReads {
    client: BigTableClient,
    served_height: ServedHeight,
}

impl KvRpcServer {
    /// `Unimplemented` unless `at_checkpoint` object reads are enabled, and `Unavailable` before
    /// the served height is known. The scans read through `client`, the request's gated client.
    fn at_checkpoint_reads(&self, client: BigTableClient) -> Result<AtCheckpointReads, RpcError> {
        if !self.object_at_checkpoint_enabled {
            return Err(RpcError::new(
                tonic::Code::Unimplemented,
                "at_checkpoint object reads are not enabled on this instance",
            ));
        }
        Ok(AtCheckpointReads {
            served_height: self.served_height()?,
            client,
        })
    }
}

impl AtCheckpointReads {
    /// The version `object_id` was live at, at the end of `checkpoint`. `None` if it did not
    /// exist yet, or had been deleted or wrapped.
    async fn live_version_at_checkpoint(
        &self,
        object_id: ObjectID,
        checkpoint: u64,
    ) -> Result<Option<SequenceNumber>, RpcError> {
        Ok(
            match self
                .client
                .get_object_version_at_checkpoint(object_id, checkpoint)
                .await?
            {
                Some((version, ObjectStatus::Live)) => Some(version),
                Some((_, ObjectStatus::Deleted | ObjectStatus::Wrapped)) | None => None,
            },
        )
    }
}

/// The error for an `at_checkpoint` above the served height. The response does not say which
/// checkpoint it was read at, so answering from a partially indexed table could hand the caller
/// a wrong answer to persist; fail closed instead.
fn not_yet_indexed(checkpoint: u64, served_height: ServedHeight) -> RpcError {
    RpcError::new(
        tonic::Code::NotFound,
        format!(
            "requested checkpoint {checkpoint} is not yet indexed; highest indexed checkpoint is \
             {}",
            served_height.checkpoint,
        ),
    )
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::time::Duration;

    use sui_kvstore::WatermarkV1;
    use sui_kvstore::tables::object_versions_by_checkpoint;
    use sui_kvstore::tables::objects;
    use sui_kvstore::testing::MockBigtableServer;
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::SuiAddress;
    use sui_types::object::Owner;

    use super::*;
    use crate::KvRpcConfig;
    use crate::LedgerHistoryConfig;
    use crate::StageConfig;
    use crate::StagesConfig;
    use crate::service_info_watermark_pipelines;

    /// A kv-rpc server over the mock BigTable at `addr`, with every watermark it serves at
    /// checkpoint 10 and its service info loaded.
    async fn start_kv_rpc(
        addr: SocketAddr,
        stages: StagesConfig,
        enable_object_at_checkpoint: bool,
    ) -> KvRpcServer {
        let mut client = sui_kvstore::BigTableClient::new_local(addr.to_string(), "test".into())
            .await
            .expect("connect to mock BigTable");
        for pipeline in service_info_watermark_pipelines(false, enable_object_at_checkpoint) {
            client
                .create_pipeline_watermark_if_absent(
                    pipeline,
                    &WatermarkV1 {
                        epoch_hi_inclusive: 0,
                        checkpoint_hi_inclusive: Some(10),
                        tx_hi: 0,
                        timestamp_ms_hi_inclusive: 10_000,
                        reader_lo: 0,
                        pruner_hi: 0,
                        pruner_timestamp_ms: 0,
                        bucket_start_cp: None,
                    },
                )
                .await
                .expect("seed watermark");
        }
        let config = KvRpcConfig::default();
        let kv_rpc = KvRpcServer::new_local_with_config(
            addr.to_string(),
            "test".to_string(),
            None,
            LedgerHistoryConfig::default(),
            config.request_bigtable_concurrency(),
            config.monotonic_read_wait_timeout(),
            stages,
            false,
            enable_object_at_checkpoint,
        )
        .await
        .expect("start kv-rpc server");
        tokio::time::timeout(
            Duration::from_secs(5),
            kv_rpc.cache.subscribe().wait_for(Option::is_some),
        )
        .await
        .expect("service info never refreshed")
        .expect("service info sender dropped");
        kv_rpc
    }

    fn mask(paths: &[&str]) -> prost_types::FieldMask {
        prost_types::FieldMask {
            paths: paths.iter().map(|path| path.to_string()).collect(),
        }
    }

    /// For each `objects`-table read since the last clear, whether it asked for values to be
    /// stripped.
    async fn objects_reads_stripped(mock: &MockBigtableServer) -> Vec<bool> {
        mock.read_rows_calls()
            .await
            .into_iter()
            .filter(|call| call.table == objects::NAME)
            .map(|call| call.values_stripped)
            .collect()
    }

    /// A read mask that asks only for `object_id` and `version` never reads object contents: the
    /// latest and exact-version reads fetch only row keys, and `at_checkpoint` reads are answered
    /// by the version index without an `objects` read.
    #[tokio::test]
    async fn test_version_only_masks_skip_object_contents() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), true).await;

        // One object, live at version 3 from checkpoint 5.
        let id = ObjectID::random();
        let version = SequenceNumber::from_u64(3);
        let object = Object::with_id_owner_version_for_testing(
            id,
            version,
            Owner::AddressOwner(SuiAddress::ZERO),
        );
        mock.insert_row(
            objects::NAME,
            objects::encode_key(&ObjectKey(id, version)),
            objects::encode(&object).unwrap(),
        )
        .await;
        mock.insert_row(
            object_versions_by_checkpoint::NAME,
            object_versions_by_checkpoint::encode_key(&id, 5),
            object_versions_by_checkpoint::encode(version, ObjectStatus::Live),
        )
        .await;

        // Latest, exact version, and at checkpoint 7.
        let selectors = [(None, None), (Some(3), None), (None, Some(7))];
        let request = |(version, at_checkpoint): (Option<u64>, Option<u64>), paths: &[&str]| {
            let mut req = GetObjectRequest::default();
            req.object_id = Some(id.to_canonical_string(true));
            req.version = version;
            req.at_checkpoint = at_checkpoint;
            req.read_mask = Some(mask(paths));
            req
        };

        let version_only = ["object_id", "version"];
        mock.clear_read_rows_calls().await;
        for selector in selectors {
            let rendered = get_object(&kv_rpc, request(selector, &version_only))
                .await
                .unwrap()
                .object
                .unwrap();
            assert_eq!(
                (rendered.object_id, rendered.version),
                (Some(id.to_canonical_string(true)), Some(3)),
                "{selector:?}",
            );
        }
        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = selectors.map(|selector| request(selector, &[])).to_vec();
        batch.read_mask = Some(mask(&version_only));
        for item in batch_get_objects(&kv_rpc, batch).await.unwrap().objects {
            assert_eq!(item.object().version, Some(3), "{item:?}");
        }
        // Latest and exact version, once each for the single reads and the batch; none for
        // `at_checkpoint`.
        assert_eq!(
            objects_reads_stripped(&mock).await,
            [true; 4],
            "version-only reads fetch only row keys",
        );

        mock_server.abort();
    }

    /// A read mask that needs no object contents answers a deleted object as `NOT_FOUND`, like a
    /// read of its contents does, both latest and at the version that deleted it.
    #[tokio::test]
    async fn test_version_only_reads_of_deleted_object_are_not_found() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), false).await;

        // Live at version 1, deleted at version 2.
        let id = ObjectID::random();
        let live = SequenceNumber::from_u64(1);
        let object = Object::with_id_owner_version_for_testing(
            id,
            live,
            Owner::AddressOwner(SuiAddress::ZERO),
        );
        mock.insert_row(
            objects::NAME,
            objects::encode_key(&ObjectKey(id, live)),
            objects::encode(&object).unwrap(),
        )
        .await;
        mock.insert_row(
            objects::NAME,
            objects::encode_key(&ObjectKey(id, SequenceNumber::from_u64(2))),
            objects::encode_tombstone(ObjectStatus::Deleted).unwrap(),
        )
        .await;

        let request = |version: Option<u64>| {
            let mut req = GetObjectRequest::default();
            req.object_id = Some(id.to_canonical_string(true));
            req.version = version;
            req
        };
        let version_only = ["object_id", "version"];
        for paths in [&version_only[..], &["object_id", "version", "digest"]] {
            for version in [None, Some(2)] {
                let mut req = request(version);
                req.read_mask = Some(mask(paths));
                let status: tonic::Status = get_object(&kv_rpc, req).await.unwrap_err().into();
                assert_eq!(
                    status.code(),
                    tonic::Code::NotFound,
                    "{version:?} {paths:?}"
                );
            }
            let mut req = request(Some(1));
            req.read_mask = Some(mask(paths));
            let found = get_object(&kv_rpc, req).await.unwrap().object.unwrap();
            assert_eq!(found.version, Some(1), "{paths:?}");
        }

        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = vec![request(None), request(Some(2)), request(Some(1))];
        batch.read_mask = Some(mask(&version_only));
        let items = batch_get_objects(&kv_rpc, batch).await.unwrap().objects;
        assert_eq!(
            items[0].error().code,
            tonic::Code::NotFound as i32,
            "latest"
        );
        assert_eq!(items[1].error().code, tonic::Code::NotFound as i32, "at 2");
        assert_eq!(items[2].object().version, Some(1), "at 1");

        mock_server.abort();
    }

    /// On an instance without `at_checkpoint` reads, a batch of exact versions goes out as
    /// `objects` multi-gets of at most the stage's `max-seeks-per-read` keys, and every item is
    /// answered, whether its version is stored or not.
    #[tokio::test]
    async fn test_batch_exact_versions_split_into_objects_stage_reads() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let stages = StagesConfig {
            objects: Some(StageConfig {
                max_seeks_per_read: Some(2),
                ..Default::default()
            }),
            ..Default::default()
        };
        let kv_rpc = start_kv_rpc(addr, stages, false).await;

        // Versions 1 to 5 are requested; only the odd ones are stored.
        let id = ObjectID::random();
        let mut stored = HashMap::new();
        for version in [1, 3, 5] {
            let object = Object::with_id_owner_version_for_testing(
                id,
                SequenceNumber::from_u64(version),
                Owner::AddressOwner(SuiAddress::ZERO),
            );
            mock.insert_row(
                objects::NAME,
                objects::encode_key(&ObjectKey(id, object.version())),
                objects::encode(&object).unwrap(),
            )
            .await;
            stored.insert(version, object);
        }
        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = (1..=5)
            .map(|version| {
                let mut req = GetObjectRequest::default();
                req.object_id = Some(id.to_canonical_string(true));
                req.version = Some(version);
                req
            })
            .collect();
        batch.read_mask = Some(mask(&["version", "digest"]));

        mock.clear_read_rows_calls().await;
        let response = batch_get_objects(&kv_rpc, batch).await.unwrap();

        assert_eq!(response.objects.len(), 5);
        for (index, item) in response.objects.iter().enumerate() {
            let version = index as u64 + 1;
            match stored.get(&version) {
                Some(object) => {
                    assert_eq!(item.object().version, Some(version), "{item:?}");
                    assert_eq!(
                        item.object().digest,
                        Some(object.digest().to_string()),
                        "{item:?}"
                    );
                }
                None => assert_eq!(item.error().code, tonic::Code::NotFound as i32, "{item:?}"),
            }
        }

        let reads: Vec<_> = mock
            .read_rows_calls()
            .await
            .into_iter()
            .filter(|call| call.table == objects::NAME)
            .map(|call| call.row_keys)
            .collect();
        assert_eq!(
            reads.len(),
            3,
            "five keys in reads of at most two: {reads:?}"
        );
        assert!(reads.iter().all(|keys| keys.len() <= 2), "{reads:?}");
        let mut read_keys: Vec<_> = reads.into_iter().flatten().collect();
        read_keys.sort();
        let requested_keys: Vec<_> = (1..=5)
            .map(|version| objects::encode_key(&ObjectKey(id, SequenceNumber::from_u64(version))))
            .collect();
        assert_eq!(read_keys, requested_keys, "each key is read exactly once");

        mock_server.abort();
    }

    #[tokio::test]
    async fn test_batch_get_objects_limit_exceeded() {
        let mock = MockBigtableServer::new();
        let (addr, server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = KvRpcServer::new_local(addr.to_string(), "test".to_string(), None)
            .await
            .expect("start kv-rpc server");

        let requests = (0..MAX_BATCH_REQUESTS + 1)
            .map(|_| {
                let mut req = GetObjectRequest::default();
                req.object_id = Some(ObjectID::random().to_canonical_string(true));
                req.version = Some(1);
                req
            })
            .collect();

        let mut req = BatchGetObjectsRequest::default();
        req.requests = requests;

        let err = batch_get_objects(&kv_rpc, req).await.unwrap_err();
        let status: tonic::Status = err.into();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);

        server.abort();
    }
}
