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
use sui_rpc_api::AtCheckpointUpperBound;
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

/// Serve `GetObject`. A `max_version` read is bounded by a checkpoint: `at_checkpoint`, or the
/// served height when it is unset, so a version written above the served height, whose deletion
/// or wrap may not be indexed yet, never answers it.
pub(crate) async fn get_object(
    server: &KvRpcServer,
    request: tonic::Request<GetObjectRequest>,
) -> Result<GetObjectResponse, RpcError> {
    let served_height = ServedHeight::of(&request);
    let mut request = request.into_inner();
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
            read_written_by(server, served_height, object_id, at_checkpoint, read).await?
        }
        ObjectSelector::MaxVersion {
            max_version,
            at_checkpoint,
        } => {
            read_bounded(
                server,
                served_height,
                &mut client,
                object_id,
                max_version,
                at_checkpoint,
                version_only,
            )
            .await?
        }
        // An `at_checkpoint` read is a `max_version` read with no bound on the version.
        ObjectSelector::AtCheckpoint(checkpoint) => {
            read_bounded(
                server,
                served_height,
                &mut client,
                object_id,
                u64::MAX,
                Some(checkpoint),
                version_only,
            )
            .await?
        }
    }
    .ok_or_else(|| selector.not_found(address))?;
    let message = found
        .resolved()
        .render(object_id, &read_mask, &server.package_resolver)
        .await;
    Ok(GetObjectResponse::new(message))
}

/// Serve a single read of the newest version of `object_id` at or below `max_version` written
/// by `at_checkpoint`, or by the served height when it is unset (see [`bounded_read`]).
/// `served_height` is the height the request was admitted at.
async fn read_bounded(
    server: &KvRpcServer,
    served_height: Result<ServedHeight, RpcError>,
    client: &mut sui_kvstore::BigTableClient,
    object_id: ObjectID,
    max_version: u64,
    at_checkpoint: Option<u64>,
    version_only: bool,
) -> Result<Option<Found>, RpcError> {
    // Both checks fail the request whatever the object's state, so they come before the reads.
    let reads = server.at_checkpoint_reads(server.limited_client("GetObject"), served_height)?;
    let checkpoint = match at_checkpoint {
        Some(checkpoint) => {
            AtCheckpointUpperBound::new(reads.served_height.checkpoint).check(checkpoint)?;
            checkpoint
        }
        None => reads.served_height.checkpoint,
    };
    let version_at_checkpoint = reads.version_at_checkpoint(object_id, checkpoint).await?;
    match bounded_read(max_version, version_at_checkpoint) {
        BoundedRead::NotFound => Ok(None),
        // A live version's row is in the objects table, so the index answers a version-only read
        // without touching it.
        BoundedRead::Live(version) if version_only => Ok(Some(Found::Version(version))),
        BoundedRead::Live(version) => {
            read_exact_version(client, ObjectKey(object_id, version), false).await
        }
        BoundedRead::ScanFrom(version) => read_max_version(client, object_id, version).await,
    }
}

/// Serve a single `version` read: `read` reads the version, and with `at_checkpoint` the object's
/// version at the end of that checkpoint is read alongside it, and the version is kept only if it
/// was written by then. `served_height` is the height the request was admitted at.
async fn read_written_by(
    server: &KvRpcServer,
    served_height: Result<ServedHeight, RpcError>,
    object_id: ObjectID,
    at_checkpoint: Option<u64>,
    read: impl Future<Output = Result<Option<Found>, RpcError>>,
) -> Result<Option<Found>, RpcError> {
    let Some(checkpoint) = at_checkpoint else {
        return read.await;
    };
    // Both checks fail the request whatever the object's state, so they come before the reads.
    let reads = server.at_checkpoint_reads(server.limited_client("GetObject"), served_height)?;
    AtCheckpointUpperBound::new(reads.served_height.checkpoint).check(checkpoint)?;
    let (found, version_at_checkpoint) =
        tokio::try_join!(read, reads.version_at_checkpoint(object_id, checkpoint))?;
    Ok(found.filter(|found| written_by_checkpoint(found.version(), version_at_checkpoint)))
}

/// Serve a batch in two stages of reads, with other reads running alongside them:
///
/// 1. One `object_versions_by_checkpoint` scan per distinct `(object_id, checkpoint)`, each its
///    own gated call, because BigTable cannot return the newest row of several ranges in one
///    `ReadRows` (`rows_limit` covers the whole request). The checkpoint is an item's
///    `at_checkpoint`, or for a `max_version` item without one, the served height. The scan
///    gives a `version` item bounded by `at_checkpoint` the newest version written by then, and
///    decides how an `at_checkpoint` or `max_version` item is read (see [`bounded_read`]).
/// 2. `objects` multi-gets for the live versions stage 1 resolved items to that no exact-version
///    read covers, alongside one single-row forward read of `objects_v2` per distinct
///    `(object_id, version)` that a `max_version` item reads down from.
///
/// The exact versions do not depend on stage 1, so their `objects` multi-gets run alongside it,
/// and one latest-version scan per distinct object runs alongside everything. Multi-gets are
/// planned by the `objects` stage ([`crate::config::StagesConfig::objects`]): reads of at most
/// `max-seeks-per-read` keys, at most `concurrency` of them at a time. Every read but the
/// latest-version scans shares one request-scoped limiter. Items whose `at_checkpoint` is above
/// the served height read nothing.
///
/// When the read mask asks for nothing but `object_id` and `version`, stage 1 answers the items
/// it resolves to a live version on its own, and the other `objects` reads fetch only row keys,
/// not object contents, except the `max_version` reads.
pub(crate) async fn batch_get_objects(
    server: &KvRpcServer,
    request: tonic::Request<BatchGetObjectsRequest>,
) -> Result<BatchGetObjectsResponse, RpcError> {
    let served_height = ServedHeight::of(&request);
    let BatchGetObjectsRequest {
        requests,
        read_mask,
        ..
    } = request.into_inner();
    if requests.len() > MAX_BATCH_REQUESTS {
        return Err(RpcError::new(
            tonic::Code::InvalidArgument,
            format!("number of batch requests exceed limit of {MAX_BATCH_REQUESTS}"),
        ));
    }
    let (requests, read_mask) = validate_get_object_requests(&requests, read_mask)?;
    let version_only = needs_only_object_id_and_version(&read_mask);

    // Every read but the latest-version scans goes through this client, under one request-wide
    // limiter.
    let limited_client = server.limited_client("BatchGetObjects");
    let objects_stage = server.stages.stage(PipelineStage::Objects);
    let at_checkpoint_reads = if requests
        .iter()
        .any(|(_, selector)| selector.needs_object_version_history())
    {
        Some(server.at_checkpoint_reads(limited_client.clone(), served_height)?)
    } else {
        None
    };
    let served_checkpoint = at_checkpoint_reads
        .as_ref()
        .map(|reads| reads.served_height.checkpoint);
    // The error for an item whose `at_checkpoint` is above the served height.
    let upper_bound = served_checkpoint.map(AtCheckpointUpperBound::new);
    let above_served_height =
        |selector: &ObjectSelector| upper_bound?.check(selector.at_checkpoint()?).err();
    // The checkpoint at which an item reads the version index: its `at_checkpoint`, or for a
    // `max_version` item without one, the served height.
    let index_checkpoint = |selector: &ObjectSelector| match *selector {
        ObjectSelector::MaxVersion {
            at_checkpoint: None,
            ..
        } => served_checkpoint,
        _ => selector.at_checkpoint(),
    };
    // How an `at_checkpoint` or `max_version` item is read, given the versions stage 1 found;
    // `None` for other items.
    let bounded_read_of = |object_id: ObjectID,
                           selector: &ObjectSelector,
                           versions_at_checkpoint: &HashMap<
        (ObjectID, u64),
        (SequenceNumber, ObjectStatus),
    >| {
        let max_version = match *selector {
            ObjectSelector::MaxVersion { max_version, .. } => max_version,
            // An `at_checkpoint` read is a `max_version` read with no bound on the version.
            ObjectSelector::AtCheckpoint(_) => u64::MAX,
            ObjectSelector::Latest | ObjectSelector::Version { .. } => return None,
        };
        let checkpoint = index_checkpoint(selector)?;
        let version_at_checkpoint = versions_at_checkpoint
            .get(&(object_id, checkpoint))
            .copied();
        Some(bounded_read(max_version, version_at_checkpoint))
    };

    let mut exact_object_keys = Vec::new();
    let mut unversioned_object_ids = Vec::new();
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
            ObjectSelector::MaxVersion { .. } | ObjectSelector::AtCheckpoint(_) => {}
        }
        if let Some(checkpoint) = index_checkpoint(selector) {
            checkpoint_scan_keys.push((object_id, checkpoint));
        }
    }

    exact_object_keys.sort();
    exact_object_keys.dedup();
    unversioned_object_ids.sort();
    unversioned_object_ids.dedup();
    checkpoint_scan_keys.sort();
    checkpoint_scan_keys.dedup();

    let client = server.client.clone();

    let exact_future = read_exact_versions(
        &limited_client,
        objects_stage,
        exact_object_keys.clone(),
        version_only,
    );

    let indexed_future = async {
        // Stage 1: each servable `(object_id, checkpoint)`'s version at the end of the
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

        // Stage 2: the live versions items resolved to above, unless the mask needs only their
        // ids and versions or an exact-version read covers them, and the scans of `max_version`
        // items from where stage 1 put them.
        let mut live_object_keys = Vec::new();
        let mut max_version_scan_keys = Vec::new();
        for (address, selector) in &requests {
            let object_id: ObjectID = (*address).into();
            match bounded_read_of(object_id, selector, &versions_at_checkpoint) {
                Some(BoundedRead::Live(version)) if !version_only => {
                    let key = ObjectKey(object_id, version);
                    if exact_object_keys.binary_search(&key).is_err() {
                        live_object_keys.push(key);
                    }
                }
                Some(BoundedRead::ScanFrom(version)) => {
                    max_version_scan_keys.push(ObjectKey(object_id, version))
                }
                Some(BoundedRead::Live(_) | BoundedRead::NotFound) | None => {}
            }
        }
        live_object_keys.sort();
        live_object_keys.dedup();
        max_version_scan_keys.sort();
        max_version_scan_keys.dedup();
        let (live_found, max_version_found) = tokio::try_join!(
            read_exact_versions(&limited_client, objects_stage, live_object_keys, false),
            scan_max_versions(&limited_client, max_version_scan_keys),
        )?;
        Ok::<_, RpcError>((versions_at_checkpoint, live_found, max_version_found))
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

    let ((versions_at_checkpoint, live_found, max_version_found), exact_found, latest_found) =
        tokio::try_join!(indexed_future, exact_future, unversioned_future)?;

    let mut objects = Vec::with_capacity(requests.len());
    for &(address, selector) in &requests {
        let object_id: ObjectID = address.into();
        let resolved = if let Some(err) = above_served_height(&selector) {
            Err(err)
        } else {
            match selector {
                ObjectSelector::Latest => latest_found.get(&object_id).map(Found::resolved),
                ObjectSelector::Version {
                    version,
                    at_checkpoint,
                } => exact_found
                    .get(&ObjectKey(object_id, version.into()))
                    .map(Found::resolved)
                    .filter(|resolved| {
                        at_checkpoint.is_none_or(|checkpoint| {
                            let version_at_checkpoint = versions_at_checkpoint
                                .get(&(object_id, checkpoint))
                                .copied();
                            written_by_checkpoint(resolved.version(), version_at_checkpoint)
                        })
                    }),
                ObjectSelector::MaxVersion { .. } | ObjectSelector::AtCheckpoint(_) => {
                    match bounded_read_of(object_id, &selector, &versions_at_checkpoint) {
                        Some(BoundedRead::Live(version)) if version_only => {
                            Some(Resolved::Version(version))
                        }
                        Some(BoundedRead::Live(version)) => {
                            let key = ObjectKey(object_id, version);
                            exact_found
                                .get(&key)
                                .or_else(|| live_found.get(&key))
                                .map(Found::resolved)
                        }
                        Some(BoundedRead::ScanFrom(version)) => max_version_found
                            .get(&ObjectKey(object_id, version))
                            .map(Resolved::Object),
                        Some(BoundedRead::NotFound) | None => None,
                    }
                }
            }
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

/// How a read of an object's newest version at or below `max_version` written by a checkpoint is
/// answered. See [`bounded_read`].
#[derive(Clone, Copy, Debug)]
enum BoundedRead {
    /// No version answers it.
    NotFound,
    /// This version, the object's version at the end of the checkpoint, answers it, and the
    /// object is live at it.
    Live(SequenceNumber),
    /// The newest version at or below this one answers it, from a single-row forward read of
    /// `objects_v2` starting at this version.
    ScanFrom(SequenceNumber),
}

/// How a read of an object's newest version at or below `max_version` written by a checkpoint is
/// answered, given the object's [`VersionAtCheckpoint`] there. An object's versions increase in
/// checkpoint order, so that is its newest version at or below the lower of `max_version` and
/// its version at the checkpoint, and nothing written after the checkpoint changes it. When
/// `max_version` is at or above the version at the checkpoint, the version index alone decides
/// the answer: the object if it is live there, and nothing if it was deleted or wrapped there.
fn bounded_read(max_version: u64, version_at_checkpoint: VersionAtCheckpoint) -> BoundedRead {
    match version_at_checkpoint {
        // The object did not exist yet.
        None => BoundedRead::NotFound,
        Some((newest, _)) if max_version < newest.value() => {
            BoundedRead::ScanFrom(max_version.into())
        }
        Some((newest, ObjectStatus::Live)) => BoundedRead::Live(newest),
        Some((_, ObjectStatus::Deleted | ObjectStatus::Wrapped)) => BoundedRead::NotFound,
    }
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
/// object, or a live version straight from the version index.
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

/// Read the newest version of `object_id` at or below `version` from the `objects` table, unless
/// the object was deleted or wrapped at that version.
async fn read_max_version(
    client: &mut sui_kvstore::BigTableClient,
    object_id: ObjectID,
    version: SequenceNumber,
) -> Result<Option<Found>, RpcError> {
    Ok(client
        .get_object_lt_or_eq_version(object_id, version)
        .await?
        .map(Found::Object))
}

/// For each `ObjectKey(object_id, version)` in `keys`, one single-row forward read of
/// `objects_v2` through the request's gated `client` for the newest version of `object_id` at or
/// below `version` (see [`read_max_version`]), at most [`MAX_CONCURRENT_UNVERSIONED_SCANS`] at a
/// time. Keys whose scan finds no object are absent from the map.
async fn scan_max_versions(
    client: &BigTableClient,
    keys: Vec<ObjectKey>,
) -> Result<HashMap<ObjectKey, Object>, RpcError> {
    let scans = keys.into_iter().map(|key| {
        let client = client.clone();
        async move {
            let object = client.get_object_lt_or_eq_version(key.0, key.1).await?;
            Ok::<_, RpcError>((key, object))
        }
    });
    let results = futures::stream::iter(scans)
        .buffer_unordered(MAX_CONCURRENT_UNVERSIONED_SCANS)
        .collect::<Vec<_>>()
        .await;

    let mut found = HashMap::new();
    for result in results {
        if let (key, Some(object)) = result? {
            found.insert(key, object);
        }
    }
    Ok(found)
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

/// What a request's reads of the version index run with: the request-scoped gated client for
/// their `object_versions_by_checkpoint` scans, and the served height that bounds them.
struct AtCheckpointReads {
    client: BigTableClient,
    served_height: ServedHeight,
}

impl KvRpcServer {
    /// `Unimplemented` unless `at_checkpoint` and `max_version` object reads are enabled;
    /// otherwise the scans read through `client`, the request's gated client, at
    /// `served_height`, the height the request was admitted at (`Unavailable` if it had none).
    fn at_checkpoint_reads(
        &self,
        client: BigTableClient,
        served_height: Result<ServedHeight, RpcError>,
    ) -> Result<AtCheckpointReads, RpcError> {
        if !self.object_at_checkpoint_enabled {
            return Err(RpcError::new(
                tonic::Code::Unimplemented,
                "at_checkpoint and max_version object reads are not enabled on this instance",
            ));
        }
        Ok(AtCheckpointReads {
            served_height: served_height?,
            client,
        })
    }
}

impl AtCheckpointReads {
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
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::time::Duration;

    use mysten_common::ZipDebugEqIteratorExt;
    use sui_kvstore::ObjectSchema;
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
    use crate::tests::seed_watermarks;

    /// A kv-rpc server over the mock BigTable at `addr`, reading the objects table of
    /// `object_schema`, with every watermark it serves at checkpoint 10 and its service info
    /// loaded.
    async fn start_kv_rpc(
        addr: SocketAddr,
        stages: StagesConfig,
        object_schema: ObjectSchema,
        enable_object_at_checkpoint: bool,
    ) -> KvRpcServer {
        start_kv_rpc_at_heights(
            addr,
            stages,
            object_schema,
            enable_object_at_checkpoint,
            |_| 10,
        )
        .await
    }

    /// [`start_kv_rpc`], with the watermark of each pipeline it serves at `height(pipeline)`.
    async fn start_kv_rpc_at_heights(
        addr: SocketAddr,
        stages: StagesConfig,
        object_schema: ObjectSchema,
        enable_object_at_checkpoint: bool,
        height: impl Fn(&str) -> u64,
    ) -> KvRpcServer {
        let mut client = sui_kvstore::BigTableClient::new_local(addr.to_string(), "test".into())
            .await
            .expect("connect to mock BigTable");
        let pipelines = service_info_watermark_pipelines(
            LedgerHistoryConfig::default().bitmap_schema(),
            object_schema,
            enable_object_at_checkpoint && object_schema == ObjectSchema::V2,
        );
        seed_watermarks(&mut client, &pipelines, height).await;
        let config = KvRpcConfig::default();
        let kv_rpc = KvRpcServer::new_local_with_config(
            addr.to_string(),
            "test".to_string(),
            None,
            LedgerHistoryConfig::default(),
            config.request_bigtable_concurrency(),
            config.monotonic_read_wait_timeout(),
            stages,
            config.watermark_source(),
            object_schema,
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

    /// Writes `cells` at `key` of `schema`'s objects table.
    async fn insert_object_row(
        mock: &MockBigtableServer,
        schema: ObjectSchema,
        key: ObjectKey,
        cells: [(&'static str, bytes::Bytes); 1],
    ) {
        mock.insert_row(
            objects::table_name(schema),
            objects::encode_key(schema, &key),
            cells,
        )
        .await;
    }

    /// For each read of `schema`'s objects table since the last clear, whether it asked for
    /// values to be stripped.
    async fn objects_reads_stripped(mock: &MockBigtableServer, schema: ObjectSchema) -> Vec<bool> {
        mock.read_rows_calls()
            .await
            .into_iter()
            .filter(|call| call.table == objects::table_name(schema))
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
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), ObjectSchema::V2, true).await;

        // One object, live at version 3 from checkpoint 5.
        let id = ObjectID::random();
        let version = SequenceNumber::from_u64(3);
        let object = Object::with_id_owner_version_for_testing(
            id,
            version,
            Owner::AddressOwner(SuiAddress::ZERO),
        );
        insert_object_row(
            &mock,
            ObjectSchema::V2,
            ObjectKey(id, version),
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
            let rendered = get_object(
                &kv_rpc,
                kv_rpc.admitted_request(request(selector, &version_only)),
            )
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
        for item in batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
            .await
            .unwrap()
            .objects
        {
            assert_eq!(item.object().version, Some(3), "{item:?}");
        }
        // Latest and exact version, once each for the single reads and the batch; none for
        // `at_checkpoint`.
        assert_eq!(
            objects_reads_stripped(&mock, ObjectSchema::V2).await,
            [true; 4],
            "version-only reads fetch only row keys",
        );

        mock_server.abort();
    }

    /// A read mask that needs no object contents answers a deleted object like a read of its
    /// contents does. Under V2, which records the deletion, both latest and at the version that
    /// deleted it are `NOT_FOUND`. Under V1, which does not, latest is the last live version.
    #[tokio::test]
    async fn test_version_only_reads_of_deleted_object() {
        for (schema, latest) in [(ObjectSchema::V2, None), (ObjectSchema::V1, Some(1))] {
            let mock = MockBigtableServer::new();
            let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
            let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), schema, false).await;

            // Live at version 1, deleted at version 2.
            let id = ObjectID::random();
            let live = SequenceNumber::from_u64(1);
            let object = Object::with_id_owner_version_for_testing(
                id,
                live,
                Owner::AddressOwner(SuiAddress::ZERO),
            );
            insert_object_row(
                &mock,
                schema,
                ObjectKey(id, live),
                objects::encode(&object).unwrap(),
            )
            .await;
            insert_object_row(
                &mock,
                schema,
                ObjectKey(id, SequenceNumber::from_u64(2)),
                objects::encode_tombstone(ObjectStatus::Deleted).unwrap(),
            )
            .await;

            let request = |version: Option<u64>| {
                let mut req = GetObjectRequest::default();
                req.object_id = Some(id.to_canonical_string(true));
                req.version = version;
                req
            };
            let version_of = |result: Result<GetObjectResponse, RpcError>| match result {
                Ok(response) => Some(response.object.unwrap().version.unwrap()),
                Err(err) => {
                    let status: tonic::Status = err.into();
                    assert_eq!(status.code(), tonic::Code::NotFound, "{status:?}");
                    None
                }
            };
            let version_only = ["object_id", "version"];
            for paths in [&version_only[..], &["object_id", "version", "digest"]] {
                for (version, expected) in [(None, latest), (Some(2), None), (Some(1), Some(1))] {
                    let mut req = request(version);
                    req.read_mask = Some(mask(paths));
                    assert_eq!(
                        version_of(get_object(&kv_rpc, kv_rpc.admitted_request(req)).await),
                        expected,
                        "{schema:?} {version:?} {paths:?}",
                    );
                }
            }

            let mut batch = BatchGetObjectsRequest::default();
            batch.requests = vec![request(None), request(Some(2)), request(Some(1))];
            batch.read_mask = Some(mask(&version_only));
            let items = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
                .await
                .unwrap()
                .objects;
            match latest {
                Some(version) => assert_eq!(items[0].object().version, Some(version), "latest"),
                None => assert_eq!(
                    items[0].error().code,
                    tonic::Code::NotFound as i32,
                    "latest"
                ),
            }
            assert_eq!(items[1].error().code, tonic::Code::NotFound as i32, "at 2");
            assert_eq!(items[2].object().version, Some(1), "at 1");

            mock_server.abort();
        }
    }

    /// `at_checkpoint` reads need both `enable-object-at-checkpoint` and
    /// [`ObjectSchema::V2`]: with the flag on under V1 they are rejected with `Unimplemented`, as
    /// with the flag off, whatever the index holds.
    #[tokio::test]
    async fn test_at_checkpoint_reads_need_the_flag_and_object_schema_v2() {
        for (schema, enabled, served) in [
            (ObjectSchema::V1, true, false),
            (ObjectSchema::V1, false, false),
            (ObjectSchema::V2, false, false),
            (ObjectSchema::V2, true, true),
        ] {
            let mock = MockBigtableServer::new();
            let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
            let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), schema, enabled).await;

            // One object, live at version 3 from checkpoint 5.
            let id = ObjectID::random();
            let version = SequenceNumber::from_u64(3);
            let object = Object::with_id_owner_version_for_testing(
                id,
                version,
                Owner::AddressOwner(SuiAddress::ZERO),
            );
            insert_object_row(
                &mock,
                schema,
                ObjectKey(id, version),
                objects::encode(&object).unwrap(),
            )
            .await;
            mock.insert_row(
                object_versions_by_checkpoint::NAME,
                object_versions_by_checkpoint::encode_key(&id, 5),
                object_versions_by_checkpoint::encode(version, ObjectStatus::Live),
            )
            .await;

            let request = || {
                let mut req = GetObjectRequest::default();
                req.object_id = Some(id.to_canonical_string(true));
                req.at_checkpoint = Some(7);
                req.read_mask = Some(mask(&["object_id", "version"]));
                req
            };
            let context = format!("{schema:?}, enable-object-at-checkpoint {enabled}");
            let single = get_object(&kv_rpc, kv_rpc.admitted_request(request())).await;
            let mut batch = BatchGetObjectsRequest::default();
            batch.requests = vec![request()];
            let batch = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch)).await;
            if served {
                assert_eq!(
                    single.unwrap().object.unwrap().version,
                    Some(3),
                    "{context}"
                );
                assert_eq!(
                    batch.unwrap().objects[0].object().version,
                    Some(3),
                    "{context}"
                );
            } else {
                for result in [single.map(|_| ()), batch.map(|_| ())] {
                    let status: tonic::Status = result.unwrap_err().into();
                    assert_eq!(status.code(), tonic::Code::Unimplemented, "{context}");
                }
            }

            mock_server.abort();
        }
    }

    /// Write `id`'s history, `(checkpoint, version, status)` for each change in order, to both
    /// the `objects_v2` table (a live row or a tombstone) and the `object_versions_by_checkpoint`
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
                objects::NAME_V2,
                objects::encode_key(ObjectSchema::V2, &ObjectKey(id, version)),
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
    /// or wrap until the object is unwrapped or re-created. `at_checkpoint` bounds a `version`
    /// read to a version written by the end of the checkpoint, and has a `max_version` read select
    /// among the versions written by then, so versions written after the checkpoint never change
    /// its answer. `GetObject` and `BatchGetObjects` agree on every read, with and without object
    /// contents.
    #[tokio::test]
    async fn test_max_version_and_checkpoint_bounds_follow_the_history() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), ObjectSchema::V2, true).await;

        // Created at version 2 in checkpoint 1, mutated at 4 in checkpoint 3, wrapped at 6 in
        // checkpoint 5, unwrapped at 8 in checkpoint 7, and deleted at 9 in checkpoint 10.
        let id = ObjectID::random();
        let history = [
            (1, 2, ObjectStatus::Live),
            (3, 4, ObjectStatus::Live),
            (5, 6, ObjectStatus::Wrapped),
            (7, 8, ObjectStatus::Live),
            (10, 9, ObjectStatus::Deleted),
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
        // The newest version at or below `version`, among those written by `at_checkpoint` if
        // set, if the object is live at it.
        let newest_at_or_below = |version: u64, at_checkpoint: Option<u64>| {
            history
                .iter()
                .rev()
                .find(|&&(changed_at, changed, _)| {
                    changed <= version && at_checkpoint.is_none_or(|cp| changed_at <= cp)
                })
                .filter(|&&(_, _, status)| status == ObjectStatus::Live)
                .map(|&(_, changed, _)| changed)
        };

        // Every version the history spans and one past it, each read exactly and as a
        // `max_version`, unbounded and bounded by every checkpoint up to the served height.
        let mut cases = Vec::new();
        for version in 0..=10 {
            for at_checkpoint in std::iter::once(None).chain((0..=10).map(Some)) {
                let exact = live(version)
                    .then_some(version)
                    .filter(|&found| at_checkpoint.is_none_or(|cp| written_by(found, cp)));
                cases.push((
                    selector_request(id, Some(version), None, at_checkpoint),
                    exact.ok_or(tonic::Code::NotFound),
                ));
                cases.push((
                    selector_request(id, None, Some(version), at_checkpoint),
                    newest_at_or_below(version, at_checkpoint).ok_or(tonic::Code::NotFound),
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
                let request = kv_rpc.admitted_request(req.clone());
                let outcome = single_outcome(get_object(&kv_rpc, request).await);
                assert_eq!(&outcome, expected, "GetObject {req:?}");
            }

            let mut batch = BatchGetObjectsRequest::default();
            batch.requests = cases.iter().map(|(req, _)| req.clone()).collect();
            batch.read_mask = Some(mask(paths));
            let items = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
                .await
                .unwrap()
                .objects;
            for (item, (req, expected)) in items.iter().zip_debug_eq(&cases) {
                assert_eq!(
                    &batch_outcome(item),
                    expected,
                    "BatchGetObjects {req:?} {paths:?}"
                );
            }
        }

        mock_server.abort();
    }

    /// With version 5 written in checkpoint 90 and version 7 in checkpoint 101, `max_version` 8 at
    /// checkpoint 95 reads version 5 whatever the served height, even with version 7 already in
    /// the tables: a version written after checkpoint 95 never counts.
    #[tokio::test]
    async fn test_max_version_at_checkpoint_ignores_later_versions() {
        for served in [95, 100, 105] {
            let mock = MockBigtableServer::new();
            let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
            let kv_rpc = start_kv_rpc_at_heights(
                addr,
                StagesConfig::default(),
                ObjectSchema::V2,
                true,
                |_| served,
            )
            .await;
            let id = ObjectID::random();
            insert_history(
                &mock,
                id,
                &[(90, 5, ObjectStatus::Live), (101, 7, ObjectStatus::Live)],
            )
            .await;

            let req = selector_request(id, None, Some(8), Some(95));
            let outcome =
                single_outcome(get_object(&kv_rpc, kv_rpc.admitted_request(req.clone())).await);
            assert_eq!(outcome, Ok(5), "GetObject, served height {served}");
            let mut batch = BatchGetObjectsRequest::default();
            batch.requests = vec![req];
            let items = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
                .await
                .unwrap()
                .objects;
            assert_eq!(
                batch_outcome(&items[0]),
                Ok(5),
                "BatchGetObjects, served height {served}"
            );
            mock_server.abort();
        }
    }

    /// A `max_version` read without `at_checkpoint` is bounded by the served height (10 here): a
    /// version written above it never answers, even when its row is already in the tables.
    #[tokio::test]
    async fn test_max_version_without_checkpoint_is_bounded_by_the_served_height() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), ObjectSchema::V2, true).await;

        // `mutated` is live at version 3 from checkpoint 5 and at version 5 from checkpoint 12;
        // `created` is created at version 2 in checkpoint 12.
        let mutated = ObjectID::random();
        insert_history(
            &mock,
            mutated,
            &[(5, 3, ObjectStatus::Live), (12, 5, ObjectStatus::Live)],
        )
        .await;
        let created = ObjectID::random();
        insert_history(&mock, created, &[(12, 2, ObjectStatus::Live)]).await;
        let cases = [
            (selector_request(mutated, None, Some(8), None), Ok(3)),
            (
                selector_request(created, None, Some(8), None),
                Err(tonic::Code::NotFound),
            ),
        ];

        for paths in [
            &["object_id", "version"][..],
            &["object_id", "version", "digest"],
        ] {
            for (req, expected) in &cases {
                let mut req = req.clone();
                req.read_mask = Some(mask(paths));
                let request = kv_rpc.admitted_request(req.clone());
                let outcome = single_outcome(get_object(&kv_rpc, request).await);
                assert_eq!(&outcome, expected, "GetObject {req:?}");
            }

            let mut batch = BatchGetObjectsRequest::default();
            batch.requests = cases.iter().map(|(req, _)| req.clone()).collect();
            batch.read_mask = Some(mask(paths));
            let items = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
                .await
                .unwrap()
                .objects;
            for (item, (req, expected)) in items.iter().zip_debug_eq(&cases) {
                assert_eq!(
                    &batch_outcome(item),
                    expected,
                    "BatchGetObjects {req:?} {paths:?}"
                );
            }
        }

        mock_server.abort();
    }

    /// `max_version` reads, and `version` reads bounded by `at_checkpoint`, are `Unimplemented`
    /// unless `at_checkpoint` and `max_version` reads are enabled, which also needs
    /// [`ObjectSchema::V2`]. A batch with an item the instance cannot serve fails as a whole.
    #[tokio::test]
    async fn test_max_version_and_checkpoint_bounds_need_the_flag() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let id = ObjectID::random();
        insert_history(&mock, id, &[(2, 2, ObjectStatus::Live)]).await;

        for (schema, enabled) in [
            (ObjectSchema::V2, false),
            (ObjectSchema::V1, true),
            (ObjectSchema::V2, true),
        ] {
            let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), schema, enabled).await;
            let expected = if schema == ObjectSchema::V2 && enabled {
                Ok(2)
            } else {
                Err(tonic::Code::Unimplemented)
            };
            for (version, max_version, at_checkpoint) in [
                (Some(2), None, Some(5)),
                (None, Some(2), None),
                (None, Some(2), Some(5)),
            ] {
                let context = format!(
                    "{schema:?}, enabled {enabled}: {version:?} {max_version:?} {at_checkpoint:?}"
                );
                let req = selector_request(id, version, max_version, at_checkpoint);
                let request = kv_rpc.admitted_request(req.clone());
                let outcome = single_outcome(get_object(&kv_rpc, request).await);
                assert_eq!(outcome, expected, "GetObject {context}");

                let mut batch = BatchGetObjectsRequest::default();
                batch.requests = vec![selector_request(id, None, None, None), req];
                let outcome = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
                    .await
                    .map(|response| batch_outcome(&response.objects[1]))
                    .unwrap_or_else(|err| Err(tonic::Status::from(err).code()));
                assert_eq!(outcome, expected, "BatchGetObjects {context}");
            }
        }

        mock_server.abort();
    }

    /// With `at_checkpoint` and `max_version` reads enabled, the versions index's watermark
    /// bounds the served height.
    #[tokio::test]
    async fn test_object_index_bounds_the_served_height() {
        let lagging = sui_kvstore::OBJECT_VERSIONS_BY_CHECKPOINT_PIPELINE;
        for (enabled, served) in [(false, 10), (true, 7)] {
            let mock = MockBigtableServer::new();
            let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
            let kv_rpc = start_kv_rpc_at_heights(
                addr,
                StagesConfig::default(),
                ObjectSchema::V2,
                enabled,
                |pipeline| if pipeline == lagging { 7 } else { 10 },
            )
            .await;
            assert_eq!(
                ServedHeight::of(&kv_rpc.admitted_request(()))
                    .unwrap()
                    .checkpoint,
                served,
                "{lagging} lagging, enabled {enabled}",
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
        let kv_rpc = start_kv_rpc(addr, stages, ObjectSchema::V1, false).await;

        // Versions 1 to 5 are requested; only the odd ones are stored.
        let id = ObjectID::random();
        let mut stored = HashMap::new();
        for version in [1, 3, 5] {
            let object = Object::with_id_owner_version_for_testing(
                id,
                SequenceNumber::from_u64(version),
                Owner::AddressOwner(SuiAddress::ZERO),
            );
            insert_object_row(
                &mock,
                ObjectSchema::V1,
                ObjectKey(id, object.version()),
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
        let response = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
            .await
            .unwrap();

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
            .map(|version| {
                objects::encode_key(
                    ObjectSchema::V1,
                    &ObjectKey(id, SequenceNumber::from_u64(version)),
                )
            })
            .collect();
        assert_eq!(read_keys, requested_keys, "each key is read exactly once");

        mock_server.abort();
    }

    #[tokio::test]
    async fn test_batch_get_objects_limit_exceeded() {
        let mock = MockBigtableServer::new();
        let (addr, server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), ObjectSchema::V1, false).await;

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

        let err = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(req))
            .await
            .unwrap_err();
        let status: tonic::Status = err.into();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);

        server.abort();
    }
}
