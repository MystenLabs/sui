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
use sui_rpc_api::RpcError;
use sui_rpc_api::grpc::v2::ledger_service::{ObjectSelector, validate_get_object_requests};
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
/// `max_version` lookups, and `object_versions_by_checkpoint` lookups.
pub const MAX_CONCURRENT_UNVERSIONED_SCANS: usize = 50;

/// The version and status an object had at the end of a checkpoint, from the
/// `object_versions_by_checkpoint` index: its newest version written at or before the
/// checkpoint, including a version at which it was deleted or wrapped. `None` if the object did
/// not exist yet.
type VersionAtCheckpoint = Option<(SequenceNumber, ObjectStatus)>;

pub(crate) async fn get_object(
    server: &KvRpcServer,
    mut request: GetObjectRequest,
) -> Result<GetObjectResponse, RpcError> {
    let read_mask = request.read_mask.take();
    let (requests, read_mask) =
        validate_get_object_requests(std::slice::from_ref(&request), read_mask)?;
    let (address, selector) = requests[0];
    let object_id: ObjectID = address.into();
    let version_only = needs_only_object_id_and_version(&read_mask);
    let mut client = server.client.clone();
    let found = match selector {
        ObjectSelector::Latest => read_latest(&mut client, &object_id, version_only).await?,
        ObjectSelector::Version {
            version,
            at_checkpoint,
        } => {
            let key = ObjectKey(object_id, version.into());
            let read = read_exact_version(&mut client, key, version_only);
            read_written_by(server, object_id, at_checkpoint, read).await?
        }
        ObjectSelector::MaxVersion {
            max_version,
            at_checkpoint,
        } => {
            server.check_object_max_version_enabled()?;
            let read = read_max_version(&mut client, object_id, max_version);
            read_written_by(server, object_id, at_checkpoint, read).await?
        }
        ObjectSelector::AtCheckpoint(checkpoint) => {
            let reads = server.at_checkpoint_reads(server.limited_client("GetObject"))?;
            reads.check_indexed(checkpoint)?;
            match reads
                .live_version_at_checkpoint(object_id, checkpoint)
                .await?
            {
                // A live version's row is in the objects table, so the index answers a
                // version-only read without touching it.
                Some(version) if version_only => Some(Found::Version(version)),
                Some(version) => {
                    read_exact_version(&mut client, ObjectKey(object_id, version), false).await?
                }
                None => None,
            }
        }
    }
    .ok_or_else(|| selector.not_found(address))?;
    let message = found
        .resolved()
        .render(object_id, &read_mask, &server.package_resolver)
        .await;
    Ok(GetObjectResponse::new(message))
}

/// Serve a single `version` or `max_version` read: `read` resolves the selected version, and with
/// `at_checkpoint` the object's version at the end of that checkpoint is read alongside it, and
/// the selected version is kept only if it was written by then.
async fn read_written_by(
    server: &KvRpcServer,
    object_id: ObjectID,
    at_checkpoint: Option<u64>,
    read: impl Future<Output = Result<Option<Found>, RpcError>>,
) -> Result<Option<Found>, RpcError> {
    let Some(checkpoint) = at_checkpoint else {
        return read.await;
    };
    // Both checks fail the request whatever the object's state, so they come before the reads.
    let reads = server.at_checkpoint_reads(server.limited_client("GetObject"))?;
    reads.check_indexed(checkpoint)?;
    let (found, version_at_checkpoint) =
        tokio::try_join!(read, reads.version_at_checkpoint(object_id, checkpoint))?;
    Ok(found.filter(|found| written_by_checkpoint(found.version(), version_at_checkpoint)))
}

/// Serve a batch in two stages of reads, plus per-object scans running alongside them:
///
/// 1. One `object_versions_by_checkpoint` scan per distinct `(object_id, at_checkpoint)`, each
///    its own gated call, because BigTable cannot return the newest row of several ranges in one
///    `ReadRows` (`rows_limit` covers the whole request). It resolves an `at_checkpoint` item to
///    its live version, and gives a `version` or `max_version` item bounded by `at_checkpoint`
///    the newest version written by then.
/// 2. `objects` multi-gets for the exact-version keys and the live versions stage 1 found,
///    planned by the `objects` stage ([`crate::config::StagesConfig::objects`]): reads of at most
///    `max-seeks-per-read` keys, at most `concurrency` of them at a time.
///
/// The per-object scans are one `objects` reverse scan per distinct `(object_id, max_version)`,
/// and one per distinct latest-version read. Stages 1 and 2 and the `max_version` scans share
/// one request-scoped limiter; the latest-version scans do not use it. Items whose
/// `at_checkpoint` is above the served height read nothing.
///
/// When the read mask asks for nothing but `object_id` and `version`, stage 1 answers the
/// `at_checkpoint` items on its own, and the `objects` reads of stage 2 and the latest scans
/// fetch only row keys, not object contents.
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
    let (requests, read_mask) = validate_get_object_requests(&requests, read_mask)?;
    let version_only = needs_only_object_id_and_version(&read_mask);

    if requests
        .iter()
        .any(|(_, selector)| matches!(selector, ObjectSelector::MaxVersion { .. }))
    {
        server.check_object_max_version_enabled()?;
    }
    // Stage 1, stage 2 and the `max_version` scans read through this client, under one
    // request-wide limiter. It is created whether or not `at_checkpoint` reads are enabled,
    // because stage 2 always uses it.
    let limited_client = server.limited_client("BatchGetObjects");
    let objects_stage = server.stages.stage(PipelineStage::Objects);
    let at_checkpoint_reads = if requests
        .iter()
        .any(|(_, selector)| selector.at_checkpoint().is_some())
    {
        Some(server.at_checkpoint_reads(limited_client.clone())?)
    } else {
        None
    };
    // An item's `at_checkpoint` and the served height, if the checkpoint is above it.
    let above_served_height = |selector: &ObjectSelector| {
        let checkpoint = selector.at_checkpoint()?;
        let served_height = at_checkpoint_reads.as_ref()?.served_height;
        (checkpoint > served_height.checkpoint).then_some((checkpoint, served_height))
    };

    let mut exact_object_keys = Vec::new();
    let mut unversioned_object_ids = Vec::new();
    let mut max_version_keys = Vec::new();
    let mut at_checkpoint_keys = Vec::new();
    let mut checkpoint_scan_keys = Vec::new();
    for (address, selector) in &requests {
        if above_served_height(selector).is_some() {
            continue;
        }
        let object_id: ObjectID = (*address).into();
        match *selector {
            ObjectSelector::Latest => unversioned_object_ids.push(object_id),
            ObjectSelector::Version { version, .. } => {
                exact_object_keys.push(ObjectKey(object_id, version.into()))
            }
            ObjectSelector::MaxVersion { max_version, .. } => {
                max_version_keys.push((object_id, max_version))
            }
            ObjectSelector::AtCheckpoint(checkpoint) => {
                at_checkpoint_keys.push((object_id, checkpoint))
            }
        }
        if let Some(checkpoint) = selector.at_checkpoint() {
            checkpoint_scan_keys.push((object_id, checkpoint));
        }
    }

    exact_object_keys.sort();
    exact_object_keys.dedup();
    unversioned_object_ids.sort();
    unversioned_object_ids.dedup();
    max_version_keys.sort();
    max_version_keys.dedup();
    at_checkpoint_keys.sort();
    at_checkpoint_keys.dedup();
    checkpoint_scan_keys.sort();
    checkpoint_scan_keys.dedup();

    let client = server.client.clone();

    let versioned_future = async {
        // Stage 1: each servable `(object_id, at_checkpoint)`'s version at the end of the
        // checkpoint.
        let mut versions_at_checkpoint = HashMap::new();
        if let Some(reads) = &at_checkpoint_reads {
            let scans =
                checkpoint_scan_keys
                    .into_iter()
                    .map(|(object_id, checkpoint)| async move {
                        let version = reads.version_at_checkpoint(object_id, checkpoint).await?;
                        Ok::<_, RpcError>(((object_id, checkpoint), version))
                    });
            let results = futures::stream::iter(scans)
                .buffer_unordered(MAX_CONCURRENT_UNVERSIONED_SCANS)
                .collect::<Vec<_>>()
                .await;
            for result in results {
                if let (key, Some(version)) = result? {
                    versions_at_checkpoint.insert(key, version);
                }
            }
        }

        // Stage 2: the exact versions, and the live versions `at_checkpoint` items resolved to
        // above unless the mask needs only their ids and versions.
        let mut object_keys = exact_object_keys;
        if !version_only {
            object_keys.extend(at_checkpoint_keys.iter().filter_map(|key| {
                match versions_at_checkpoint.get(key) {
                    Some(&(version, ObjectStatus::Live)) => Some(ObjectKey(key.0, version)),
                    _ => None,
                }
            }));
        }
        object_keys.sort();
        object_keys.dedup();
        let found_by_key =
            read_exact_versions(&limited_client, objects_stage, object_keys, version_only).await?;
        Ok::<_, RpcError>((versions_at_checkpoint, found_by_key))
    };

    let max_version_future = async {
        let scans = max_version_keys
            .into_iter()
            .map(|(object_id, max_version)| {
                let client = limited_client.clone();
                async move {
                    let object = client
                        .get_object_lt_or_eq_version(object_id, max_version.into())
                        .await?;
                    Ok::<_, RpcError>(((object_id, max_version), object))
                }
            });
        let results = futures::stream::iter(scans)
            .buffer_unordered(MAX_CONCURRENT_UNVERSIONED_SCANS)
            .collect::<Vec<_>>()
            .await;

        let mut max_version_found = HashMap::new();
        for result in results {
            if let (key, Some(object)) = result? {
                max_version_found.insert(key, object);
            }
        }
        Ok::<_, RpcError>(max_version_found)
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

    let ((versions_at_checkpoint, found_by_key), max_version_found, latest_found) =
        tokio::try_join!(versioned_future, max_version_future, unversioned_future)?;

    let mut objects = Vec::with_capacity(requests.len());
    for &(address, selector) in &requests {
        let object_id: ObjectID = address.into();
        let version_at = |checkpoint: u64| -> VersionAtCheckpoint {
            versions_at_checkpoint
                .get(&(object_id, checkpoint))
                .copied()
        };
        let resolved = if let Some((checkpoint, served_height)) = above_served_height(&selector) {
            Err(not_yet_indexed(checkpoint, served_height))
        } else {
            match selector {
                ObjectSelector::Latest => latest_found.get(&object_id).map(Found::resolved),
                ObjectSelector::Version { version, .. } => found_by_key
                    .get(&ObjectKey(object_id, version.into()))
                    .map(Found::resolved),
                ObjectSelector::MaxVersion { max_version, .. } => max_version_found
                    .get(&(object_id, max_version))
                    .map(Resolved::Object),
                ObjectSelector::AtCheckpoint(checkpoint) => match version_at(checkpoint) {
                    Some((version, ObjectStatus::Live)) if version_only => {
                        Some(Resolved::Version(version))
                    }
                    Some((version, ObjectStatus::Live)) => found_by_key
                        .get(&ObjectKey(object_id, version))
                        .map(Found::resolved),
                    Some((_, ObjectStatus::Deleted | ObjectStatus::Wrapped)) | None => None,
                },
            }
            .filter(|resolved| {
                selector.visibility_bound().is_none_or(|checkpoint| {
                    written_by_checkpoint(resolved.version(), version_at(checkpoint))
                })
            })
            .ok_or_else(|| selector.not_found(address).into())
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

/// Whether `version` of an object was written at or before a checkpoint, given the object's
/// [`VersionAtCheckpoint`] there. An object's versions increase in checkpoint order, so a version
/// above its newest one written by the checkpoint was written after it, and an object that did
/// not exist yet had no version written by then.
fn written_by_checkpoint(
    version: SequenceNumber,
    version_at_checkpoint: VersionAtCheckpoint,
) -> bool {
    version_at_checkpoint.is_some_and(|(newest, _)| version <= newest)
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

    fn version(&self) -> SequenceNumber {
        self.resolved().version()
    }
}

/// What a requested object resolved to: a [`Found`] it shares with other requests for the same
/// object, or an `at_checkpoint` item's version straight from the version index.
enum Resolved<'a> {
    Object(&'a Object),
    Version(SequenceNumber),
}

impl Resolved<'_> {
    fn version(&self) -> SequenceNumber {
        match self {
            Resolved::Object(object) => object.version(),
            Resolved::Version(version) => *version,
        }
    }

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

/// Read the newest version of `object_id` at or below `max_version` from the `objects` table,
/// unless the object was deleted or wrapped at that version.
async fn read_max_version(
    client: &mut sui_kvstore::BigTableClient,
    object_id: ObjectID,
    max_version: u64,
) -> Result<Option<Found>, RpcError> {
    Ok(client
        .get_object_lt_or_eq_version(object_id, max_version.into())
        .await?
        .map(Found::Object))
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

    /// `Unimplemented` unless `max_version` object reads are enabled.
    fn check_object_max_version_enabled(&self) -> Result<(), RpcError> {
        if !self.object_max_version_enabled {
            return Err(RpcError::new(
                tonic::Code::Unimplemented,
                "max_version object reads are not enabled on this instance",
            ));
        }
        Ok(())
    }
}

impl AtCheckpointReads {
    /// `NotFound` if `checkpoint` is above the served height (see [`not_yet_indexed`]).
    fn check_indexed(&self, checkpoint: u64) -> Result<(), RpcError> {
        if checkpoint > self.served_height.checkpoint {
            return Err(not_yet_indexed(checkpoint, self.served_height));
        }
        Ok(())
    }

    /// The version and status `object_id` had at the end of `checkpoint`.
    async fn version_at_checkpoint(
        &self,
        object_id: ObjectID,
        checkpoint: u64,
    ) -> Result<VersionAtCheckpoint, RpcError> {
        self.client
            .get_object_version_at_checkpoint(object_id, checkpoint)
            .await
    }

    /// The version `object_id` was live at, at the end of `checkpoint`. `None` if it did not
    /// exist yet, or had been deleted or wrapped.
    async fn live_version_at_checkpoint(
        &self,
        object_id: ObjectID,
        checkpoint: u64,
    ) -> Result<Option<SequenceNumber>, RpcError> {
        Ok(
            match self.version_at_checkpoint(object_id, checkpoint).await? {
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

    use mysten_common::ZipDebugEqIteratorExt;
    use sui_kvstore::WatermarkV1;
    use sui_kvstore::tables::object_versions_by_checkpoint;
    use sui_kvstore::tables::objects;
    use sui_kvstore::testing::MockBigtableServer;
    use sui_rpc::proto::sui::rpc::v2::get_object_result;
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
        enable_object_max_version: bool,
    ) -> KvRpcServer {
        start_kv_rpc_at_heights(
            addr,
            stages,
            enable_object_at_checkpoint,
            enable_object_max_version,
            |_| 10,
        )
        .await
    }

    /// [`start_kv_rpc`], with the watermark of each pipeline it serves at `height(pipeline)`.
    async fn start_kv_rpc_at_heights(
        addr: SocketAddr,
        stages: StagesConfig,
        enable_object_at_checkpoint: bool,
        enable_object_max_version: bool,
        height: impl Fn(&str) -> u64,
    ) -> KvRpcServer {
        let mut client = sui_kvstore::BigTableClient::new_local(addr.to_string(), "test".into())
            .await
            .expect("connect to mock BigTable");
        for pipeline in service_info_watermark_pipelines(
            false,
            enable_object_at_checkpoint,
            enable_object_max_version,
        ) {
            let checkpoint = height(pipeline);
            client
                .create_pipeline_watermark_if_absent(
                    pipeline,
                    &WatermarkV1 {
                        epoch_hi_inclusive: 0,
                        checkpoint_hi_inclusive: Some(checkpoint),
                        tx_hi: 0,
                        timestamp_ms_hi_inclusive: checkpoint * 1_000,
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
            enable_object_max_version,
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
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), true, false).await;

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
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), false, false).await;

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

    /// Write `id`'s history, `(checkpoint, version, status)` for each change in order, to both
    /// the `objects` table (a live row or a tombstone) and the `object_versions_by_checkpoint`
    /// index.
    async fn insert_history(
        mock: &MockBigtableServer,
        id: ObjectID,
        history: &[(u64, u64, ObjectStatus)],
    ) {
        for &(checkpoint, version, status) in history {
            let version = SequenceNumber::from_u64(version);
            let cells = match status {
                ObjectStatus::Live => objects::encode(&Object::with_id_owner_version_for_testing(
                    id,
                    version,
                    Owner::AddressOwner(SuiAddress::ZERO),
                ))
                .unwrap(),
                removed => objects::encode_tombstone(removed).unwrap(),
            };
            mock.insert_row(
                objects::NAME,
                objects::encode_key(&ObjectKey(id, version)),
                cells,
            )
            .await;
            mock.insert_row(
                object_versions_by_checkpoint::NAME,
                object_versions_by_checkpoint::encode_key(&id, checkpoint),
                object_versions_by_checkpoint::encode(version, status),
            )
            .await;
        }
    }

    /// A `GetObjectRequest` for `id` with the given selector fields.
    fn selector_request(
        id: ObjectID,
        version: Option<u64>,
        max_version: Option<u64>,
        at_checkpoint: Option<u64>,
    ) -> GetObjectRequest {
        let mut req = GetObjectRequest::default();
        req.object_id = Some(id.to_canonical_string(true));
        req.version = version;
        req.max_version = max_version;
        req.at_checkpoint = at_checkpoint;
        req
    }

    /// The version a `GetObject` call returned, or its error code.
    fn single_outcome(result: Result<GetObjectResponse, RpcError>) -> Result<u64, tonic::Code> {
        match result {
            Ok(response) => Ok(response.object.unwrap().version.unwrap()),
            Err(err) => Err(tonic::Status::from(err).code()),
        }
    }

    /// The version a `BatchGetObjects` item returned, or its error code.
    fn batch_outcome(item: &GetObjectResult) -> Result<u64, tonic::Code> {
        match &item.result {
            Some(get_object_result::Result::Object(object)) => Ok(object.version.unwrap()),
            Some(get_object_result::Result::Error(status)) => Err(tonic::Code::from(status.code)),
            other => panic!("unexpected batch result {other:?}"),
        }
    }

    /// `max_version` reads the newest version at or below it, and is `NotFound` from a deletion
    /// or wrap until the object is unwrapped or re-created. `at_checkpoint` bounds a `version` or
    /// `max_version` read to versions written by the end of the checkpoint, without falling back
    /// to an older version that was. `GetObject` and `BatchGetObjects` agree on every read, with
    /// and without object contents.
    #[tokio::test]
    async fn test_max_version_and_checkpoint_bounds_follow_the_history() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), true, true).await;

        // Created at version 2 in checkpoint 2, mutated at 4 in checkpoint 4, wrapped at 6 in
        // checkpoint 6, unwrapped at 8 in checkpoint 8, and deleted at 9 in checkpoint 9.
        let id = ObjectID::random();
        let history = [
            (2, 2, ObjectStatus::Live),
            (4, 4, ObjectStatus::Live),
            (6, 6, ObjectStatus::Wrapped),
            (8, 8, ObjectStatus::Live),
            (9, 9, ObjectStatus::Deleted),
        ];
        insert_history(&mock, id, &history).await;

        let live = |version: u64| {
            history
                .iter()
                .any(|&(_, changed, status)| changed == version && status == ObjectStatus::Live)
        };
        let written_by = |version: u64, checkpoint: u64| {
            history
                .iter()
                .any(|&(changed_at, changed, _)| changed == version && changed_at <= checkpoint)
        };
        let newest_at_or_below = |version: u64| {
            history
                .iter()
                .rev()
                .find(|&&(_, changed, _)| changed <= version)
                .filter(|&&(_, _, status)| status == ObjectStatus::Live)
                .map(|&(_, changed, _)| changed)
        };

        // Every version and every checkpoint the history spans, and one past each, each read
        // exactly and as a `max_version`, unbounded and bounded by each checkpoint.
        let mut cases = Vec::new();
        for version in 0..=10 {
            for at_checkpoint in std::iter::once(None).chain((0..=10).map(Some)) {
                let bounded = |found: Option<u64>| {
                    found
                        .filter(|&found| at_checkpoint.is_none_or(|cp| written_by(found, cp)))
                        .ok_or(tonic::Code::NotFound)
                };
                cases.push((
                    selector_request(id, Some(version), None, at_checkpoint),
                    bounded(live(version).then_some(version)),
                ));
                cases.push((
                    selector_request(id, None, Some(version), at_checkpoint),
                    bounded(newest_at_or_below(version)),
                ));
            }
        }

        for paths in [
            &["object_id", "version"][..],
            &["object_id", "version", "digest"],
        ] {
            for (req, expected) in &cases {
                let mut req = req.clone();
                req.read_mask = Some(mask(paths));
                let outcome = single_outcome(get_object(&kv_rpc, req.clone()).await);
                assert_eq!(&outcome, expected, "GetObject {req:?}");
            }

            let mut batch = BatchGetObjectsRequest::default();
            batch.requests = cases.iter().map(|(req, _)| req.clone()).collect();
            batch.read_mask = Some(mask(paths));
            let items = batch_get_objects(&kv_rpc, batch).await.unwrap().objects;
            for (item, (req, expected)) in items.iter().zip_debug_eq(&cases) {
                assert_eq!(
                    &batch_outcome(item),
                    expected,
                    "BatchGetObjects {req:?} {paths:?}"
                );
            }
        }

        // Version 2 was visible at checkpoint 3, but `max_version` 4 resolves to version 4,
        // written in checkpoint 4, so the bounded read is `NotFound`.
        let err = get_object(&kv_rpc, selector_request(id, None, Some(4), Some(3)))
            .await
            .unwrap_err();
        let status = tonic::Status::from(err);
        assert_eq!(status.code(), tonic::Code::NotFound);
        assert_eq!(
            status.message(),
            format!(
                "Object {} with version at most 4 at checkpoint 3 not found",
                id.to_canonical_string(true)
            ),
        );

        mock_server.abort();
    }

    /// `max_version` reads are `Unimplemented` unless enabled, and a `version` or `max_version`
    /// read bounded by `at_checkpoint` also needs `at_checkpoint` reads enabled. A batch with an
    /// item the instance cannot serve fails as a whole.
    #[tokio::test]
    async fn test_max_version_and_checkpoint_bounds_need_their_flags() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let id = ObjectID::random();
        insert_history(&mock, id, &[(2, 2, ObjectStatus::Live)]).await;

        for (at_checkpoint_enabled, max_version_enabled) in [(true, false), (false, true)] {
            let kv_rpc = start_kv_rpc(
                addr,
                StagesConfig::default(),
                at_checkpoint_enabled,
                max_version_enabled,
            )
            .await;
            for (version, max_version, at_checkpoint) in [
                (Some(2), None, Some(5)),
                (None, Some(2), None),
                (None, Some(2), Some(5)),
            ] {
                let served = (max_version.is_none() || max_version_enabled)
                    && (at_checkpoint.is_none() || at_checkpoint_enabled);
                let expected = if served {
                    Ok(2)
                } else {
                    Err(tonic::Code::Unimplemented)
                };
                let context = format!(
                    "at_checkpoint enabled {at_checkpoint_enabled}, max_version enabled \
                     {max_version_enabled}: {version:?} {max_version:?} {at_checkpoint:?}"
                );
                let req = selector_request(id, version, max_version, at_checkpoint);
                let outcome = single_outcome(get_object(&kv_rpc, req.clone()).await);
                assert_eq!(outcome, expected, "GetObject {context}");

                let mut batch = BatchGetObjectsRequest::default();
                batch.requests = vec![selector_request(id, None, None, None), req];
                let outcome = batch_get_objects(&kv_rpc, batch)
                    .await
                    .map(|response| batch_outcome(&response.objects[1]))
                    .unwrap_or_else(|err| Err(tonic::Status::from(err).code()));
                assert_eq!(outcome, expected, "BatchGetObjects {context}");
            }
        }

        mock_server.abort();
    }

    /// With `max_version` reads enabled, the tombstones pipeline's watermark bounds the served
    /// height.
    #[tokio::test]
    async fn test_object_tombstones_bound_the_served_height() {
        let tombstones_behind = |pipeline: &str| {
            if pipeline == sui_kvstore::OBJECT_TOMBSTONES_PIPELINE {
                7
            } else {
                10
            }
        };
        for (enable_object_max_version, served) in [(false, 10), (true, 7)] {
            let mock = MockBigtableServer::new();
            let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
            let kv_rpc = start_kv_rpc_at_heights(
                addr,
                StagesConfig::default(),
                false,
                enable_object_max_version,
                tombstones_behind,
            )
            .await;
            assert_eq!(
                kv_rpc.served_height().unwrap().checkpoint,
                served,
                "max_version enabled {enable_object_max_version}",
            );
            mock_server.abort();
        }
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
        let kv_rpc = start_kv_rpc(addr, stages, false, false).await;

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
