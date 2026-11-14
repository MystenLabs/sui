// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::StreamExt;
use futures::TryStreamExt;
use futures::future;
use futures::stream;
use futures::stream::BoxStream;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use sui_kvstore::ObjectStatus;
use sui_kvstore::tables::objects::ObjectTombstone;
use sui_kvstore::tables::objects::ObjectVersionRow;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsResponse;
use sui_rpc::proto::sui::rpc::v2::Object as ProtoObject;
use sui_rpc::proto::sui::rpc::v2::{GetObjectRequest, GetObjectResponse, GetObjectResult};
use sui_rpc_api::AtCheckpointUpperBound;
use sui_rpc_api::NotServedHereCause;
use sui_rpc_api::ObjectNotFoundError;
use sui_rpc_api::ObjectRemoval;
use sui_rpc_api::RpcError;
use sui_rpc_api::grpc::v2::ledger_service::{ObjectSelector, validate_get_object_requests};
use sui_rpc_api::ledger_history::object_versions::ObjectRemovalKind;
use sui_rpc_api::not_served_here;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::object::Object;
use sui_types::storage::ObjectKey;

use crate::KvRpcServer;
use crate::PackageResolver;
use crate::bigtable_client::BigTableClient;
use crate::config::{DEFAULT_RENDER_AHEAD, PipelineStage};
use crate::pipeline::{
    KeyedBatchOutput, MarkedKeyedUpstream, MissingRow, RenderAheadError, ResolvedWatermarked,
    SeekUnit, Watermarked, pipelined_chunks, pipelined_keyed_batches, render_ahead,
};
use crate::render::needs_only_object_id_and_version;
use crate::render::object_to_response;
use crate::render::object_version_to_response;
use crate::request_cache::{CacheFetcher, RequestCache};
use crate::served_height::ServedHeight;

pub const MAX_BATCH_REQUESTS: usize = 1000;

/// The version and status an object had at the end of a checkpoint, from the
/// `object_versions_by_checkpoint` index: its newest version written at or before the
/// checkpoint, including a version at which it was deleted or wrapped. `None` if the object did
/// not exist yet.
type VersionAtCheckpoint = Option<(SequenceNumber, ObjectStatus)>;

/// Serve `GetObject`. A `max_version` read counts versions up to the served height, with or
/// without `at_checkpoint`, so a version written above the served height, whose deletion or wrap
/// may not be indexed yet, never answers it. A read that resolves to a deletion or wrap answers
/// `NOT_FOUND` with an `OBJECT_REMOVED` detail (see [`respond`]). Every BigTable read goes through
/// the request's limiter.
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
    let client = server.limited_client("GetObject");
    // Both checks fail the request whatever the object's state, so they come before the reads.
    let at_checkpoint_reads = if selector.needs_object_version_history() {
        Some(Arc::new(
            server.at_checkpoint_reads(client.clone(), served_height)?,
        ))
    } else {
        None
    };
    let objects = ObjectReads {
        client,
        version_only: needs_only_object_id_and_version(&read_mask),
    };
    let answer = match first_step(object_id, selector, at_checkpoint_reads.as_ref()) {
        FirstStep::Answer(answer) => answer,
        FirstStep::Scan(scan) => {
            let scanned = vec![((), scan.run(&objects).await?)];
            // One scan, so one answer.
            let ((), answer) = answer_scans(&objects, scanned).await.remove(0);
            answer
        }
    };
    let mut rows = HashMap::new();
    if let Some(key) = answer.row_key()
        && let Some(row) = objects.exact(key).await?
    {
        rows.insert(key, row);
    }
    let found = answer.complete(&rows)?;
    let message = respond(
        found.as_ref(),
        object_id,
        selector.not_found(address),
        &read_mask,
        &server.package_resolver,
    )
    .await?;
    Ok(GetObjectResponse::new(message))
}

/// Serve a batch as its distinct lookups, read and rendered as they become ready:
///
/// 1. Each lookup first learns which `objects` row answers it, if any. An exact version names its
///    row. A latest lookup reads its object with one reverse scan of `objects`. Every other
///    lookup reads `object_versions_by_checkpoint` (see [`AtCheckpointReads`]): a `version`
///    lookup bounded by `at_checkpoint` keeps its row only if the version was written by then,
///    and an `at_checkpoint` or `max_version` lookup is answered as [`bounded_read`] decides, by
///    a live version's row, by the removal the index records (its tombstone row, read in the
///    scan and best effort, adds the removing transaction), or by a reverse scan of `objects`
///    down from `max_version`. The scans run in groups of the `objects` stage's
///    `max-seeks-per-read`, each group one read of a [`pipelined_chunks`] stage, and every group
///    starts at once. A group's answers move on together, so the rows they name fill at most one
///    `objects` read.
/// 2. The rows the lookups name are read as the lookups arrive, by [`pipelined_keyed_batches`]
///    under the `objects` stage ([`crate::config::StagesConfig::objects`]): reads of at most
///    `max-seeks-per-read` keys, at most `concurrency` of them at a time, and each row at most
///    once per request. A lookup whose row is missing is `NOT_FOUND`, unless the version index
///    named the row as live: then the index and `objects` disagree, and the lookup is `INTERNAL`,
///    as it is when such a row is a tombstone. A lookup whose row is a tombstone is otherwise
///    `NOT_FOUND` with an `OBJECT_REMOVED` detail (see [`respond`]).
/// 3. Each lookup is rendered once, at most `DEFAULT_RENDER_AHEAD` at a time (the list endpoints'
///    default render-ahead), and every request for it gets the result.
///
/// Every BigTable read of the batch, scans and `objects` reads alike, shares one request-scoped
/// limiter, and each `(object_id, checkpoint)` of the version index is read at most once per
/// request. Rendering is not limited by it: package loads go through the server-wide
/// [`PackageResolver`]. A lookup whose `at_checkpoint` is above the served height fails without
/// a read, and a batch with a lookup that the instance cannot serve fails as a whole.
///
/// When the read mask asks for nothing but `object_id` and `version`, a lookup that the version
/// index resolves to a live version is answered by its scans alone, and the `objects` reads fetch
/// only row keys and tombstones, not object contents.
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

    // Requests for the same object state share one lookup: `request_lookups[i]` is the index in
    // `lookups` of request `i`'s lookup.
    let mut lookups = Vec::new();
    let mut lookup_indexes = HashMap::new();
    let request_lookups: Vec<usize> = requests
        .into_iter()
        .map(|lookup| {
            *lookup_indexes.entry(lookup).or_insert_with(|| {
                lookups.push(lookup);
                lookups.len() - 1
            })
        })
        .collect();
    let lookup_count = lookups.len();

    // Every read of the batch goes through this client, under one request-wide limiter.
    let limited_client = server.limited_client("BatchGetObjects");
    let at_checkpoint_reads = if lookups
        .iter()
        .any(|(_, selector)| selector.needs_object_version_history())
    {
        Some(Arc::new(server.at_checkpoint_reads(
            limited_client.clone(),
            served_height,
        )?))
    } else {
        None
    };
    let objects = ObjectReads {
        client: limited_client,
        version_only,
    };

    let mut answered_without_scan = Vec::new();
    let mut scans = Vec::new();
    for (index, &(address, selector)) in lookups.iter().enumerate() {
        match first_step(address.into(), selector, at_checkpoint_reads.as_ref()) {
            FirstStep::Answer(answer) => answered_without_scan.push((index, answer)),
            FirstStep::Scan(scan) => scans.push((index, scan)),
        }
    }

    // A group's drainer task runs its scans at once and to their ends, so each scan returns its
    // permit whether or not the `objects` reads below are taking lookups. Every group starts at
    // once: the request limiter alone bounds how many scans reach BigTable.
    let objects_stage = server.stages.stage(PipelineStage::Objects);
    let scans_per_group = objects_stage.max_seeks_per_read;
    let scan_groups = scans.len().div_ceil(scans_per_group).max(1);
    let scans: BoxStream<'static, Result<Watermarked<(usize, Scan)>, RpcError>> =
        stream::iter(scans.into_iter().map(|scan| Ok(Watermarked::Item(scan)))).boxed();
    let scanned = pipelined_chunks(scans, scans_per_group, scan_groups, None, SeekUnit::Item, {
        let objects = objects.clone();
        move |group: Vec<(usize, Scan)>| {
            let objects = objects.clone();
            async move {
                let scanned = future::try_join_all(group.into_iter().map(|(index, scan)| {
                    let objects = &objects;
                    async move { Ok::<_, RpcError>((index, scan.run(objects).await?)) }
                }))
                .await?;
                let answers = answer_scans(&objects, scanned).await;
                Ok::<_, RpcError>(stream::iter(answers.into_iter().map(Ok)).boxed())
            }
        }
    });
    // Exact versions go out in key order, so an `objects` read covers one run of adjacent keys.
    answered_without_scan.sort_by_key(|(_, answer)| answer.row_key());
    let answered_without_scan = stream::iter(
        answered_without_scan
            .into_iter()
            .map(|answer| Ok(Watermarked::Item(answer))),
    )
    .boxed();
    let lookups_with_keys: MarkedKeyedUpstream<(usize, ObjectsStageInput), ObjectKey, RpcError> =
        stream::select(answered_without_scan, scanned)
            .map_ok(|frame| {
                frame.map_item(|(index, answer)| {
                    let keys = answer.row_keys();
                    ((index, answer), keys)
                })
            })
            .boxed();

    let fetcher: Arc<dyn CacheFetcher<ObjectKey, Found>> = Arc::new(objects);
    let answered: BoxStream<'static, Result<ResolvedWatermarked<LookupWithRows>, RpcError>> =
        pipelined_keyed_batches(
            lookups_with_keys,
            objects_stage.max_seeks_per_read,
            objects_stage.concurrency,
            objects_stage.min_seeks_per_split_read,
            fetcher,
            MissingRow::Omit,
        )
        // The lookups carry no watermarks.
        .try_filter_map(|frame| {
            future::ready(Ok(match frame {
                Watermarked::Item(item) => Some(ResolvedWatermarked::Item(item)),
                Watermarked::Watermark(_) => None,
            }))
        })
        .boxed();

    let mut rendered = render_ahead(answered, DEFAULT_RENDER_AHEAD, {
        let read_mask = Arc::new(read_mask);
        let resolver = server.package_resolver.clone();
        move |((index, answer), rows)| {
            let (address, selector) = lookups[index];
            let read_mask = read_mask.clone();
            let resolver = resolver.clone();
            async move {
                let result = match answer.complete(&rows) {
                    Ok(found) => respond(
                        found.as_ref(),
                        address.into(),
                        selector.not_found(address),
                        &read_mask,
                        &resolver,
                    )
                    .await
                    .map(GetObjectResult::new_object),
                    Err(err) => Err(err),
                };
                let result = result
                    .unwrap_or_else(|err| GetObjectResult::new_error(err.into_status_proto()));
                Ok::<_, Infallible>((index, result))
            }
        }
    });

    let mut results: Vec<Option<GetObjectResult>> = vec![None; lookup_count];
    while let Some(frame) = rendered.next().await {
        match frame {
            Ok(ResolvedWatermarked::Item((index, result))) => results[index] = Some(result),
            Ok(ResolvedWatermarked::Watermark { .. }) => {}
            Err(RenderAheadError::Upstream(err)) => return Err(err),
            Err(RenderAheadError::Render(never)) => match never {},
        }
    }

    // A lookup's result goes to every request for it; the last of those requests takes it.
    let mut requests_left = vec![0usize; lookup_count];
    for &lookup in &request_lookups {
        requests_left[lookup] += 1;
    }
    let objects = request_lookups
        .into_iter()
        .map(|lookup| {
            requests_left[lookup] -= 1;
            let result = if requests_left[lookup] == 0 {
                results[lookup].take()
            } else {
                results[lookup].clone()
            };
            result.ok_or_else(|| {
                RpcError::new(
                    tonic::Code::Internal,
                    "BatchGetObjects: a lookup ended without a result",
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
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

/// The bounds of an `at_checkpoint` or `max_version` read: it selects the object's newest version
/// at or below `max_version` written by the end of checkpoint `height`, and finds nothing if that
/// version was written after `checkpoint`, or deleted or wrapped the object.
#[derive(Clone, Copy, Debug)]
struct Bounds {
    max_version: u64,
    checkpoint: u64,
    height: u64,
}

impl Bounds {
    /// The bounds of an `at_checkpoint` or `max_version` read at `served_checkpoint`; `None` for
    /// other selectors. A `max_version` read counts versions up to the served height, and its
    /// `at_checkpoint` defaults to it. An `at_checkpoint` read is a `max_version` read with no
    /// bound on the version, counting versions up to the checkpoint.
    fn of(selector: &ObjectSelector, served_checkpoint: u64) -> Option<Self> {
        match *selector {
            ObjectSelector::MaxVersion {
                max_version,
                at_checkpoint,
            } => Some(Self {
                max_version,
                checkpoint: at_checkpoint.unwrap_or(served_checkpoint),
                height: served_checkpoint,
            }),
            ObjectSelector::AtCheckpoint(checkpoint) => Some(Self {
                max_version: u64::MAX,
                checkpoint,
                height: checkpoint,
            }),
            ObjectSelector::Latest | ObjectSelector::Version { .. } => None,
        }
    }
}

/// How an `at_checkpoint` or `max_version` read is answered. See [`bounded_read`].
#[derive(Clone, Copy, Debug)]
enum BoundedRead {
    /// No version answers it.
    NotFound,
    /// This version, the object's version at the end of the checkpoint, answers it, and the
    /// object is live at it.
    Live(SequenceNumber),
    /// This version, the object's version at the end of the checkpoint, answers it, and the
    /// object was deleted or wrapped at it, as `status` says. Its tombstone row in the `objects`
    /// table names the removing transaction; that read is best effort (see [`best_effort`]), and
    /// if it fails the `OBJECT_REMOVED` detail names no transaction.
    Removed {
        version: SequenceNumber,
        status: ObjectStatus,
    },
    /// The newest version at or below `from`, from an `objects` reverse scan, answers it if it is
    /// at or below `at_most`, the object's version at the end of the checkpoint; a newer one was
    /// written after the checkpoint.
    ScanFrom {
        from: SequenceNumber,
        at_most: SequenceNumber,
    },
    /// The object was deleted or wrapped at `removed_at`, its version at the end of the
    /// checkpoint, and changed again before the height. The newest version at or below `from`,
    /// from an `objects` reverse scan, is selected: the removal if the scan finds its tombstone,
    /// and nothing otherwise, since any newer version was written after the checkpoint. Either
    /// way the answer is `NOT_FOUND`, so the scan only decides the `OBJECT_REMOVED` detail, and
    /// is best effort: if it fails, the answer goes without the detail.
    ScanForRemoval {
        from: SequenceNumber,
        removed_at: SequenceNumber,
    },
}

/// How a read with [`Bounds`] is answered, given the object's [`VersionAtCheckpoint`] at
/// `checkpoint` and at `height`. An object's versions increase in checkpoint order, so a version
/// above its version at the end of a checkpoint was written after it.
///
/// Below the version at the checkpoint, every version at or below `max_version` was written by
/// the checkpoint, so a reverse scan from `max_version` answers. Otherwise, if the object did not
/// change between the checkpoint and the height, the version index alone decides the answer: the
/// object if it is live at its version at the checkpoint, and its removal if it was deleted or
/// wrapped there. If it did change and `max_version` reaches its version at the height, that newer
/// version is selected, and nothing answers. Below it, a reverse scan from `max_version` answers
/// only if it finds the version at the checkpoint itself. Every version the scans can reach is at
/// or below the version at the height, so versions written above the height never count.
fn bounded_read(
    max_version: u64,
    version_at_checkpoint: VersionAtCheckpoint,
    version_at_height: VersionAtCheckpoint,
) -> BoundedRead {
    let Some((at_checkpoint, status)) = version_at_checkpoint else {
        // Every version of the object was written after the checkpoint.
        return BoundedRead::NotFound;
    };
    let from = max_version.into();
    if max_version < at_checkpoint.value() {
        return BoundedRead::ScanFrom {
            from,
            at_most: at_checkpoint,
        };
    }
    match version_at_height {
        Some((at_height, _)) if at_height > at_checkpoint => {
            if max_version >= at_height.value() {
                BoundedRead::NotFound
            } else if status == ObjectStatus::Live {
                BoundedRead::ScanFrom {
                    from,
                    at_most: at_checkpoint,
                }
            } else {
                BoundedRead::ScanForRemoval {
                    from,
                    removed_at: at_checkpoint,
                }
            }
        }
        _ => match status {
            ObjectStatus::Live => BoundedRead::Live(at_checkpoint),
            ObjectStatus::Deleted | ObjectStatus::Wrapped => BoundedRead::Removed {
                version: at_checkpoint,
                status,
            },
        },
    }
}

/// The removal the version index records at `version`, with `status`, and the removing
/// transaction from `row`, the `objects` row at that version, when it is a tombstone that names
/// it.
fn indexed_removal(
    version: SequenceNumber,
    status: ObjectStatus,
    row: Option<&Found>,
) -> ObjectRemoval {
    let transaction = match row {
        Some(Found::Removed(removal)) if removal.version == version.value() => removal.transaction,
        _ => None,
    };
    ObjectRemoval {
        version: version.value(),
        kind: removal_kind(status),
        transaction,
    }
}

/// The kind of removal a tombstone or version index row with `status` records.
pub(super) fn removal_kind(status: ObjectStatus) -> ObjectRemovalKind {
    match status {
        ObjectStatus::Deleted => ObjectRemovalKind::Deleted,
        ObjectStatus::Wrapped => ObjectRemovalKind::Wrapped,
        ObjectStatus::Live => unreachable!("removals are deleted or wrapped"),
    }
}

/// An object as a read found it: the whole object, or, for a read mask that needs only
/// `object_id` and `version`, just its version; or the removal a tombstone row records.
#[derive(Clone)]
enum Found {
    Object(Arc<Object>),
    Version(SequenceNumber),
    Removed(ObjectRemoval),
}

impl Found {
    /// The `objects` row at `version`.
    fn from_row(version: SequenceNumber, row: ObjectVersionRow) -> Self {
        match row {
            ObjectVersionRow::Live(Some(object)) => Found::Object(Arc::new(object)),
            ObjectVersionRow::Live(None) => Found::Version(version),
            ObjectVersionRow::Tombstone(ObjectTombstone {
                status,
                transaction,
            }) => Found::Removed(ObjectRemoval {
                version: version.value(),
                kind: removal_kind(status),
                transaction,
            }),
        }
    }

    fn version(&self) -> SequenceNumber {
        match self {
            Found::Object(object) => object.version(),
            Found::Version(version) => *version,
            Found::Removed(removal) => SequenceNumber::from_u64(removal.version),
        }
    }

    /// Whether this is the tombstone of a removal at `version`.
    fn is_removal_at(&self, version: SequenceNumber) -> bool {
        matches!(self, Found::Removed(removal) if removal.version == version.value())
    }
}

/// The answer to a read of `object_id` that found `found`: the live version rendered with
/// `read_mask`, or `not_found`, with an `OBJECT_REMOVED` detail when the read resolved to a
/// deletion or wrap.
async fn respond(
    found: Option<&Found>,
    object_id: ObjectID,
    not_found: ObjectNotFoundError,
    read_mask: &FieldMaskTree,
    resolver: &PackageResolver,
) -> Result<ProtoObject, RpcError> {
    match found {
        Some(Found::Object(object)) => Ok(object_to_response(object, read_mask, resolver).await),
        Some(Found::Version(version)) => Ok(object_version_to_response(object_id, *version)),
        Some(Found::Removed(removal)) => Err(not_found.removed(*removal).into()),
        None => Err(not_found.into()),
    }
}

/// A batch lookup after its `objects` reads: its index in the batch's lookups, its
/// [`ObjectsStageInput`], and the rows of the keys it listed.
type LookupWithRows = KeyedBatchOutput<(usize, ObjectsStageInput), ObjectKey, Found>;

/// How a lookup starts: with its answer known up to one `objects` row, or with a scan.
enum FirstStep {
    Answer(ObjectsStageInput),
    Scan(Scan),
}

/// The first step of a lookup of `object_id` by `selector`. A selector that needs object version
/// history reads the version index through `at_checkpoint_reads`, and fails without a read if its
/// `at_checkpoint` is above the served height.
fn first_step(
    object_id: ObjectID,
    selector: ObjectSelector,
    at_checkpoint_reads: Option<&Arc<AtCheckpointReads>>,
) -> FirstStep {
    match selector {
        ObjectSelector::Latest => FirstStep::Scan(Scan::Latest(object_id)),
        ObjectSelector::Version {
            version,
            at_checkpoint: None,
        } => FirstStep::Answer(ObjectsStageInput::RequestedRow(ObjectKey(
            object_id,
            version.into(),
        ))),
        ObjectSelector::Version {
            at_checkpoint: Some(_),
            ..
        }
        | ObjectSelector::MaxVersion { .. }
        | ObjectSelector::AtCheckpoint(_) => {
            let scan = at_checkpoint_reads
                .ok_or_else(|| {
                    RpcError::new(
                        tonic::Code::Internal,
                        "a read of object version history has no version index reads",
                    )
                })
                .and_then(|reads| reads.scan(object_id, selector));
            match scan {
                Ok(scan) => FirstStep::Scan(scan),
                Err(err) => FirstStep::Answer(ObjectsStageInput::Failed(err)),
            }
        }
    }
}

/// A scan that a lookup starts with, before any `objects` multi-get.
enum Scan {
    /// The newest version of the object, from one reverse scan of `objects`.
    Latest(ObjectID),
    /// The version `key` names, if the object's version at the end of `checkpoint` shows that it
    /// was written by then.
    WrittenBy {
        reads: Arc<AtCheckpointReads>,
        key: ObjectKey,
        checkpoint: u64,
    },
    /// An `at_checkpoint` or `max_version` read, answered as [`bounded_read`] decides.
    Bounded {
        reads: Arc<AtCheckpointReads>,
        object_id: ObjectID,
        bounds: Bounds,
    },
}

/// What a [`Scan`] found: the lookup's answer as far as the scan knows it, or a removal the
/// version index records, whose tombstone row [`answer_scans`] reads for the removing
/// transaction.
enum ScanResult {
    Answer(ObjectsStageInput),
    IndexedRemoval {
        key: ObjectKey,
        status: ObjectStatus,
    },
}

/// The answers of `scanned`, a group of scans' results keyed by `I`. The tombstone rows of the
/// removals the version index records are read together in one `objects` read, which only adds
/// the removing transaction to the version and kind the index records, so it is best effort (see
/// [`best_effort`]): if it fails, each of those removals answers `NOT_FOUND` with an
/// `OBJECT_REMOVED` detail that names no transaction, as when the row is missing. A group holds
/// at most `max-seeks-per-read` scans, so the read names at most that many keys.
async fn answer_scans<I>(
    objects: &ObjectReads,
    scanned: Vec<(I, ScanResult)>,
) -> Vec<(I, ObjectsStageInput)> {
    let mut removal_keys: Vec<ObjectKey> = scanned
        .iter()
        .filter_map(|(_, result)| match result {
            ScanResult::IndexedRemoval { key, .. } => Some(*key),
            ScanResult::Answer(_) => None,
        })
        .collect();
    removal_keys.sort();
    removal_keys.dedup();
    let removal_rows = if removal_keys.is_empty() {
        HashMap::new()
    } else {
        best_effort(&removal_keys, objects.rows(removal_keys.clone()).await).unwrap_or_default()
    };
    scanned
        .into_iter()
        .map(|(index, result)| {
            let answer = match result {
                ScanResult::Answer(answer) => answer,
                ScanResult::IndexedRemoval { key, status } => ObjectsStageInput::Scanned(Some(
                    Found::Removed(indexed_removal(key.1, status, removal_rows.get(&key))),
                )),
            };
            (index, answer)
        })
        .collect()
}

impl Scan {
    async fn run(self, objects: &ObjectReads) -> Result<ScanResult, RpcError> {
        Ok(ScanResult::Answer(match self {
            Scan::Latest(object_id) => ObjectsStageInput::Scanned(objects.latest(object_id).await?),
            Scan::WrittenBy {
                reads,
                key,
                checkpoint,
            } => {
                let version_at_checkpoint = reads.version_at_checkpoint(key.0, checkpoint).await?;
                if written_by_checkpoint(key.1, version_at_checkpoint) {
                    ObjectsStageInput::RequestedRow(key)
                } else {
                    ObjectsStageInput::Scanned(None)
                }
            }
            Scan::Bounded {
                reads,
                object_id,
                bounds,
            } => {
                // Both reads of the version index start at once. The version at the height
                // decides nothing when the object did not exist at the checkpoint or `max_version`
                // is below its version there; reading it anyway costs a read in those cases and
                // saves a round trip in the others.
                let (version_at_checkpoint, version_at_height) =
                    if bounds.height == bounds.checkpoint {
                        let version = reads
                            .version_at_checkpoint(object_id, bounds.checkpoint)
                            .await?;
                        (version, version)
                    } else {
                        future::try_join(
                            reads.version_at_checkpoint(object_id, bounds.checkpoint),
                            reads.version_at_checkpoint(object_id, bounds.height),
                        )
                        .await?
                    };
                match bounded_read(bounds.max_version, version_at_checkpoint, version_at_height) {
                    BoundedRead::NotFound => ObjectsStageInput::Scanned(None),
                    // A live version's row is in the objects table, so the index answers a
                    // version-only read without touching it.
                    BoundedRead::Live(version) if objects.version_only => {
                        ObjectsStageInput::Scanned(Some(Found::Version(version)))
                    }
                    BoundedRead::Live(version) => {
                        ObjectsStageInput::IndexedRow(ObjectKey(object_id, version))
                    }
                    // The tombstone row adds only the removing transaction to what the index
                    // records.
                    BoundedRead::Removed { version, status } => {
                        return Ok(ScanResult::IndexedRemoval {
                            key: ObjectKey(object_id, version),
                            status,
                        });
                    }
                    BoundedRead::ScanFrom { from, at_most } => ObjectsStageInput::Scanned(
                        objects
                            .at_or_below(object_id, from)
                            .await?
                            .filter(|found| found.version() <= at_most),
                    ),
                    // Only a tombstone answers, so the scan skips live objects' contents.
                    BoundedRead::ScanForRemoval { from, removed_at } => {
                        let key = ObjectKey(object_id, from);
                        ObjectsStageInput::Scanned(
                            best_effort(&[key], objects.newest(object_id, from, false).await)
                                .flatten()
                                .filter(|found| found.is_removal_at(removed_at)),
                        )
                    }
                }
            }
        }))
    }
}

/// What a lookup carries into its `objects` reads: its answer as far as it is known before
/// them. A batch hands it to the `objects` stage; `GetObject` reads its row with
/// `ObjectReads::exact`.
enum ObjectsStageInput {
    /// The `objects` row at the version the request asked for answers the lookup; without the
    /// row it is `NOT_FOUND`.
    RequestedRow(ObjectKey),
    /// The `objects` row at this key, which the version index names as live, answers the
    /// lookup. Without the row, or if it is a tombstone, the index and `objects` disagree, and the
    /// lookup is `INTERNAL`.
    IndexedRow(ObjectKey),
    /// The lookup's scan answered it; `None` makes it `NOT_FOUND`.
    Scanned(Option<Found>),
    /// The lookup fails with this error, without a read.
    Failed(RpcError),
}

impl ObjectsStageInput {
    /// The `objects` row the lookup still needs, if any.
    fn row_key(&self) -> Option<ObjectKey> {
        match self {
            ObjectsStageInput::RequestedRow(key) | ObjectsStageInput::IndexedRow(key) => Some(*key),
            ObjectsStageInput::Scanned(_) | ObjectsStageInput::Failed(_) => None,
        }
    }

    /// The `objects` rows the lookup still needs.
    fn row_keys(&self) -> Vec<ObjectKey> {
        self.row_key().into_iter().collect()
    }

    /// The lookup's answer, given the rows the `objects` reads returned for its keys. `Ok(None)`
    /// means `NOT_FOUND`.
    fn complete(self, rows: &HashMap<ObjectKey, Found>) -> Result<Option<Found>, RpcError> {
        match self {
            ObjectsStageInput::RequestedRow(key) => Ok(rows.get(&key).cloned()),
            ObjectsStageInput::IndexedRow(key) => match rows.get(&key) {
                Some(Found::Removed(_)) | None => Err(missing_indexed_row(key)),
                Some(found) => Ok(Some(found.clone())),
            },
            ObjectsStageInput::Scanned(found) => Ok(found),
            ObjectsStageInput::Failed(err) => Err(err),
        }
    }
}

/// The error for a version that `object_versions_by_checkpoint` names as live but that has no
/// live `objects` row, because the row is missing or a tombstone: the index and storage diverged,
/// so the object's absence is not an answer.
fn missing_indexed_row(key: ObjectKey) -> RpcError {
    RpcError::new(
        tonic::Code::Internal,
        format!("object_versions_by_checkpoint names {key:?} as live, but objects has no live row"),
    )
}

/// The result of a read that only adds to the `OBJECT_REMOVED` detail of a `NOT_FOUND` answer:
/// the value read, or `None`, logged, if the read of `keys` failed, so the answer goes without
/// what the read would add rather than fail.
fn best_effort<T>(keys: &[ObjectKey], read: Result<T, RpcError>) -> Option<T> {
    read.inspect_err(|err| {
        tracing::warn!(
            ?keys,
            "reading removals' objects rows failed, answering without them: {err}"
        )
    })
    .ok()
}

/// Reads of the `objects` table through a request's gated client, live rows and tombstones
/// alike: whole objects, or, with `version_only`, just the versions of the live ones, read
/// without the objects' contents. A tombstone reads as the removal it records.
#[derive(Clone)]
struct ObjectReads {
    client: BigTableClient,
    version_only: bool,
}

impl ObjectReads {
    /// The newest row of `object_id`.
    async fn latest(&self, object_id: ObjectID) -> Result<Option<Found>, RpcError> {
        self.newest(object_id, SequenceNumber::MAX, !self.version_only)
            .await
    }

    /// The row at the version `key` names.
    async fn exact(&self, key: ObjectKey) -> Result<Option<Found>, RpcError> {
        Ok(self.rows(vec![key]).await?.remove(&key))
    }

    /// The rows at the versions `keys` name, in one read; a key without a row has no entry.
    async fn rows(&self, keys: Vec<ObjectKey>) -> Result<HashMap<ObjectKey, Found>, RpcError> {
        // The read runs to its end, so it finishes as completed rather than cut off.
        self.fetch(keys).await?.try_collect().await
    }

    /// The newest row of `object_id` at or below `version`, from one reverse scan of `objects`:
    /// whole, or, with `version_only`, without a live object's contents.
    async fn at_or_below(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Result<Option<Found>, RpcError> {
        self.newest(object_id, version, !self.version_only).await
    }

    /// The newest row of `object_id` at or below `version`, from one reverse scan of `objects`,
    /// with live objects' contents if `with_contents`.
    async fn newest(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
        with_contents: bool,
    ) -> Result<Option<Found>, RpcError> {
        // The scan runs to its end, so it finishes as completed rather than cut off.
        let rows: Vec<_> = self
            .client
            .object_versions_stream(
                object_id,
                SequenceNumber::MIN..=version,
                1,
                true,
                with_contents,
            )
            .await?
            .try_collect()
            .await?;
        Ok(rows
            .into_iter()
            .next()
            .map(|(version, row)| Found::from_row(version, row)))
    }
}

#[async_trait::async_trait]
impl CacheFetcher<ObjectKey, Found> for ObjectReads {
    async fn fetch(
        &self,
        keys: Vec<ObjectKey>,
    ) -> Result<BoxStream<'static, Result<(ObjectKey, Found), RpcError>>, RpcError> {
        Ok(self
            .client
            .get_object_rows_stream(keys, !self.version_only)
            .await?
            .map_ok(|(key, row)| (key, Found::from_row(key.1, row)))
            .map_err(RpcError::from)
            .boxed())
    }
}

/// What a request's reads of the version index run with: the served height that bounds them, and
/// a cache that reads each `(object_id, checkpoint)` at most once per request, through the
/// request's gated client.
pub(super) struct AtCheckpointReads {
    pub(super) served_height: ServedHeight,
    versions: Arc<RequestCache<(ObjectID, u64), (SequenceNumber, ObjectStatus)>>,
}

impl KvRpcServer {
    /// `Unimplemented` / `NOT_SERVED_HERE` (cause `disabled_by_config`) unless object version
    /// history reads (`at_checkpoint` and `max_version` object reads, and `ListObjectVersions`)
    /// are enabled; otherwise the scans read through `client`, the request's gated client, at
    /// `served_height`, the height the request was admitted at (`Unavailable` if it had none).
    pub(super) fn at_checkpoint_reads(
        &self,
        client: BigTableClient,
        served_height: Result<ServedHeight, RpcError>,
    ) -> Result<AtCheckpointReads, RpcError> {
        if !self.object_at_checkpoint_enabled {
            return Err(not_served_here(
                NotServedHereCause::DisabledByConfig,
                "at_checkpoint and max_version object reads, and ListObjectVersions, are not \
                 enabled on this instance",
            ));
        }
        Ok(AtCheckpointReads {
            served_height: served_height?,
            versions: RequestCache::new(Arc::new(VersionIndexReads { client })),
        })
    }
}

impl AtCheckpointReads {
    /// The scan of a lookup that needs object version history: a `version` bounded by
    /// `at_checkpoint`, a `max_version`, or an `at_checkpoint` alone. Fails if the lookup's
    /// `at_checkpoint` is above the served height.
    fn scan(
        self: &Arc<Self>,
        object_id: ObjectID,
        selector: ObjectSelector,
    ) -> Result<Scan, RpcError> {
        let served_checkpoint = self.served_height.checkpoint;
        if let Some(checkpoint) = selector.at_checkpoint() {
            AtCheckpointUpperBound::new(served_checkpoint).check(checkpoint)?;
        }
        let reads = self.clone();
        Ok(match selector {
            ObjectSelector::Version {
                version,
                at_checkpoint: Some(checkpoint),
            } => Scan::WrittenBy {
                reads,
                key: ObjectKey(object_id, version.into()),
                checkpoint,
            },
            _ => {
                let bounds = Bounds::of(&selector, served_checkpoint).ok_or_else(|| {
                    RpcError::new(
                        tonic::Code::Internal,
                        "not an at_checkpoint or max_version read",
                    )
                })?;
                Scan::Bounded {
                    reads,
                    object_id,
                    bounds,
                }
            }
        })
    }

    /// The version and status `object_id` had at the end of `checkpoint`. The first call for an
    /// `(object_id, checkpoint)` reads it in a task the cache owns, so the read returns its permit
    /// whether or not its caller is polled; later calls share that read.
    pub(super) async fn version_at_checkpoint(
        &self,
        object_id: ObjectID,
        checkpoint: u64,
    ) -> Result<VersionAtCheckpoint, RpcError> {
        let key = (object_id, checkpoint);
        let mut reservation = self.versions.reserve(&[key]);
        let unread = if reservation.is_unread(&key) {
            vec![key]
        } else {
            Vec::new()
        };
        let _read_end = self.versions.read(reservation.take_unread(&unread));
        reservation.value(&key).await
    }
}

/// Reads of `object_versions_by_checkpoint` through a request's gated client: one scan per
/// `(object_id, checkpoint)`, because BigTable cannot return the newest row of several ranges in
/// one `ReadRows` (`rows_limit` covers the whole request).
struct VersionIndexReads {
    client: BigTableClient,
}

#[async_trait::async_trait]
impl CacheFetcher<(ObjectID, u64), (SequenceNumber, ObjectStatus)> for VersionIndexReads {
    async fn fetch(
        &self,
        keys: Vec<(ObjectID, u64)>,
    ) -> Result<
        BoxStream<'static, Result<((ObjectID, u64), (SequenceNumber, ObjectStatus)), RpcError>>,
        RpcError,
    > {
        let client = self.client.clone();
        Ok(stream::iter(keys)
            .then(move |(object_id, checkpoint)| {
                let client = client.clone();
                async move {
                    let version = client
                        .get_object_version_at_checkpoint(object_id, checkpoint)
                        .await?;
                    Ok::<_, RpcError>(version.map(|version| ((object_id, checkpoint), version)))
                }
            })
            .try_filter_map(|row| future::ready(Ok(row)))
            .boxed())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use mysten_common::ZipDebugEqIteratorExt;
    use sui_kvstore::tables::object_versions_by_checkpoint;
    use sui_kvstore::tables::objects;
    use sui_kvstore::testing::MockBigtableServer;
    use sui_rpc::proto::sui::rpc::v2::get_object_result;
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::SuiAddress;
    use sui_types::digests::TransactionDigest;
    use sui_types::object::Owner;

    use super::*;
    use crate::StageConfig;
    use crate::StagesConfig;
    use crate::v2::test_utils::insert_history;
    use crate::v2::test_utils::mask;
    use crate::v2::test_utils::start_kv_rpc;
    use crate::v2::test_utils::start_kv_rpc_at_heights;
    use crate::v2::test_utils::start_kv_rpc_with_request_concurrency;

    /// For each `objects`-table read since the last clear, whether it asked for live objects'
    /// contents to be stripped: every value, or only the live-object column's, keeping
    /// tombstones whole.
    async fn objects_reads_stripped(mock: &MockBigtableServer) -> Vec<bool> {
        mock.read_rows_calls()
            .await
            .into_iter()
            .filter(|call| call.table == objects::NAME)
            .map(|call| {
                call.values_stripped || call.values_stripped_in == [objects::col::OBJECT.as_bytes()]
            })
            .collect()
    }

    /// A read mask that asks for no field beyond `object_id` and `version`, including one that
    /// names only `version` and an empty one, never reads object contents: the latest and
    /// exact-version reads, and the reverse scan of a `max_version` below the version at the
    /// checkpoint, fetch only row keys, and `at_checkpoint` reads are answered by the version
    /// index without an `objects` read. Like a read of the object, the answer always carries both
    /// `object_id` and `version`, and nothing else.
    #[tokio::test]
    async fn test_version_only_masks_skip_object_contents() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), true).await;

        // One object, live at version 2 from checkpoint 4 and at version 3 from checkpoint 5.
        let id = ObjectID::random();
        for (version, checkpoint) in [(2, 4), (3, 5)] {
            let version = SequenceNumber::from_u64(version);
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
                object_versions_by_checkpoint::encode_key(&id, checkpoint),
                object_versions_by_checkpoint::encode(version, ObjectStatus::Live),
            )
            .await;
        }

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

        let mut expected = ProtoObject::default();
        expected.object_id = Some(id.to_canonical_string(true));
        expected.version = Some(3);
        for paths in [&["object_id", "version"][..], &["version"], &[]] {
            mock.clear_read_rows_calls().await;
            for selector in selectors {
                let rendered =
                    get_object(&kv_rpc, kv_rpc.admitted_request(request(selector, paths)))
                        .await
                        .unwrap()
                        .object
                        .unwrap();
                assert_eq!(rendered, expected, "{selector:?} {paths:?}");
            }
            let mut batch = BatchGetObjectsRequest::default();
            batch.requests = selectors.map(|selector| request(selector, &[])).to_vec();
            batch.read_mask = Some(mask(paths));
            for item in batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
                .await
                .unwrap()
                .objects
            {
                assert_eq!(item.object(), &expected, "{item:?} {paths:?}");
            }
            // Latest and exact version, once each for the single reads and the batch; none for
            // `at_checkpoint`.
            assert_eq!(
                objects_reads_stripped(&mock).await,
                [true; 4],
                "version-only reads fetch only row keys: {paths:?}",
            );
        }

        // `max_version` 2 at checkpoint 7, below the version 3 at the checkpoint, scans down
        // from 2.
        let version_only = ["object_id", "version"];
        let below = |paths: &[&str]| {
            let mut req = request((None, Some(7)), paths);
            req.max_version = Some(2);
            req
        };
        mock.clear_read_rows_calls().await;
        let rendered = get_object(&kv_rpc, kv_rpc.admitted_request(below(&version_only)))
            .await
            .unwrap()
            .object
            .unwrap();
        assert_eq!(rendered.version, Some(2));
        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = vec![below(&[])];
        batch.read_mask = Some(mask(&version_only));
        let objects = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
            .await
            .unwrap()
            .objects;
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].object().version, Some(2), "{:?}", objects[0]);
        assert_eq!(
            objects_reads_stripped(&mock).await,
            [true; 2],
            "version-only `max_version` scans fetch only row keys",
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
            objects::encode_tombstone(ObjectStatus::Deleted, &TransactionDigest::random()).unwrap(),
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
                let status: tonic::Status = get_object(&kv_rpc, kv_rpc.admitted_request(req))
                    .await
                    .unwrap_err()
                    .into();
                assert_eq!(
                    status.code(),
                    tonic::Code::NotFound,
                    "{version:?} {paths:?}"
                );
            }
            let mut req = request(Some(1));
            req.read_mask = Some(mask(paths));
            let found = get_object(&kv_rpc, kv_rpc.admitted_request(req))
                .await
                .unwrap()
                .object
                .unwrap();
            assert_eq!(found.version, Some(1), "{paths:?}");
        }

        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = vec![request(None), request(Some(2)), request(Some(1))];
        batch.read_mask = Some(mask(&version_only));
        let items = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
            .await
            .unwrap()
            .objects;
        assert_eq!(
            items[0].error().code,
            tonic::Code::NotFound as i32,
            "latest"
        );
        assert_eq!(items[1].error().code, tonic::Code::NotFound as i32, "at 2");
        assert_eq!(items[2].object().version, Some(1), "at 1");

        mock_server.abort();
    }

    /// A version that the version index names as live but that has no `objects` row means the
    /// index and storage diverged: an `at_checkpoint` read of it is `INTERNAL`, not `NOT_FOUND`,
    /// both alone and in a batch. A request that names the same version itself is `NOT_FOUND`.
    #[tokio::test]
    async fn test_at_checkpoint_version_missing_from_objects_is_internal() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), true).await;

        // The index has the object live at version 3 from checkpoint 5; `objects` has no row.
        let id = ObjectID::random();
        mock.insert_row(
            object_versions_by_checkpoint::NAME,
            object_versions_by_checkpoint::encode_key(&id, 5),
            object_versions_by_checkpoint::encode(SequenceNumber::from_u64(3), ObjectStatus::Live),
        )
        .await;

        let request = |version: Option<u64>, at_checkpoint: Option<u64>| {
            let mut req = GetObjectRequest::default();
            req.object_id = Some(id.to_canonical_string(true));
            req.version = version;
            req.at_checkpoint = at_checkpoint;
            req
        };
        let contents = ["object_id", "version", "digest"];

        let mut req = request(None, Some(7));
        req.read_mask = Some(mask(&contents));
        let status: tonic::Status = get_object(&kv_rpc, kv_rpc.admitted_request(req))
            .await
            .unwrap_err()
            .into();
        assert_eq!(status.code(), tonic::Code::Internal, "{status:?}");

        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = vec![request(None, Some(7)), request(Some(3), None)];
        batch.read_mask = Some(mask(&contents));
        let items = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
            .await
            .unwrap()
            .objects;
        assert_eq!(
            items[0].error().code,
            tonic::Code::Internal as i32,
            "at checkpoint 7"
        );
        assert_eq!(
            items[1].error().code,
            tonic::Code::NotFound as i32,
            "version 3"
        );

        mock_server.abort();
    }

    /// On an instance without `at_checkpoint` reads, an `at_checkpoint` request is refused as
    /// `Unimplemented` with `NOT_SERVED_HERE` / `disabled_by_config`, and so is a whole batch
    /// that contains one. The refusal depends only on the request's shape, so a checkpoint above
    /// the served height gets it too, not `OUT_OF_RANGE`.
    #[tokio::test]
    async fn test_at_checkpoint_refused_as_disabled_by_config() {
        use sui_rpc::proto::google::rpc::ErrorInfo;
        use sui_rpc::proto::sui::rpc::v2::ErrorReason;

        fn assert_disabled_by_config(error: RpcError) {
            let status = error.into_status_proto();
            assert_eq!(status.code, tonic::Code::Unimplemented as i32, "{status:?}");
            let error_infos: Vec<ErrorInfo> = status
                .details
                .iter()
                .filter_map(|detail| detail.to_msg::<ErrorInfo>().ok())
                .collect();
            let [error_info] = error_infos.as_slice() else {
                panic!("expected exactly one ErrorInfo in {status:?}");
            };
            assert_eq!(
                error_info.reason,
                ErrorReason::NotServedHere.as_str_name(),
                "{status:?}"
            );
            assert_eq!(error_info.domain, ErrorReason::DOMAIN, "{status:?}");
            assert_eq!(
                error_info.metadata,
                [("cause".to_owned(), "disabled_by_config".to_owned())].into(),
                "{status:?}"
            );
        }

        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), false).await;

        let request = |at_checkpoint: Option<u64>| {
            let mut req = GetObjectRequest::default();
            req.object_id = Some(ObjectID::random().to_canonical_string(true));
            req.at_checkpoint = at_checkpoint;
            req
        };

        // The served height is 10.
        for checkpoint in [5, 11] {
            assert_disabled_by_config(
                get_object(&kv_rpc, kv_rpc.admitted_request(request(Some(checkpoint))))
                    .await
                    .unwrap_err(),
            );
        }

        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = vec![request(None), request(Some(5)), request(Some(11))];
        assert_disabled_by_config(
            batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
                .await
                .unwrap_err(),
        );

        mock_server.abort();
    }

    /// A version that the version index names as live but whose `objects` row is a tombstone is
    /// the same index and storage divergence: an `at_checkpoint` read of it is `INTERNAL`, alone
    /// and in a batch. A request that names the version itself reads the tombstone as a removal.
    #[tokio::test]
    async fn test_at_checkpoint_live_version_with_tombstone_row_is_internal() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), true).await;

        // The index has the object live at version 3 from checkpoint 5; `objects` has a
        // tombstone there.
        let id = ObjectID::random();
        mock.insert_row(
            object_versions_by_checkpoint::NAME,
            object_versions_by_checkpoint::encode_key(&id, 5),
            object_versions_by_checkpoint::encode(SequenceNumber::from_u64(3), ObjectStatus::Live),
        )
        .await;
        mock.insert_row(
            objects::NAME,
            objects::encode_key(&ObjectKey(id, SequenceNumber::from_u64(3))),
            objects::encode_tombstone(ObjectStatus::Deleted, &TransactionDigest::random()).unwrap(),
        )
        .await;
        let contents = ["object_id", "version", "digest"];

        let mut req = selector_request(id, None, None, Some(7));
        req.read_mask = Some(mask(&contents));
        let status: tonic::Status = get_object(&kv_rpc, kv_rpc.admitted_request(req))
            .await
            .unwrap_err()
            .into();
        assert_eq!(status.code(), tonic::Code::Internal, "{status:?}");

        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = vec![
            selector_request(id, None, None, Some(7)),
            selector_request(id, Some(3), None, None),
        ];
        batch.read_mask = Some(mask(&contents));
        let items = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
            .await
            .unwrap()
            .objects;
        assert_eq!(
            items[0].error().code,
            tonic::Code::Internal as i32,
            "at checkpoint 7"
        );
        assert!(
            matches!(
                removal_outcome(items[1].error()),
                Err(Some((3, _, Some(_))))
            ),
            "version 3: {:?}",
            items[1]
        );

        mock_server.abort();
    }

    /// When the version index records the removal a read resolves to, the `objects` read that
    /// only adds the removing transaction is best effort: if it fails, the read answers
    /// `NOT_FOUND` with the version and kind the index records and no transaction, rather than
    /// fail, alone and in a batch, and the batch's other items are answered. The scan that looks
    /// for a removal's tombstone when the object changed again after the checkpoint is best
    /// effort too: if it fails, the read answers `NOT_FOUND` without the detail.
    #[tokio::test]
    async fn test_failed_removal_row_reads_answer_not_found() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), true).await;

        // Created at version 2 in checkpoint 1, deleted at 6 in checkpoint 5, and created again
        // at 8 in checkpoint 7. The deletion's tombstone row cannot be decoded, so reading it
        // fails.
        let id = ObjectID::random();
        insert_history(
            &mock,
            id,
            &[
                (1, 2, ObjectStatus::Live),
                (5, 6, ObjectStatus::Deleted),
                (7, 8, ObjectStatus::Live),
            ],
        )
        .await;
        mock.insert_row(
            objects::NAME,
            objects::encode_key(&ObjectKey(id, SequenceNumber::from_u64(6))),
            [(objects::col::STATUS, vec![1u8, 2].into())],
        )
        .await;

        // The index records the deletion at checkpoint 5, and `max_version` 7 at checkpoint 5
        // scans for its tombstone, since the object changed again by the served height.
        let deleted_at_6: RemovalOutcome = Err(Some((6, "DELETED".to_owned(), None)));
        let removal_reads = [
            (
                selector_request(id, None, None, Some(5)),
                deleted_at_6.clone(),
            ),
            (selector_request(id, None, Some(7), Some(5)), Err(None)),
        ];
        for (req, expected) in &removal_reads {
            let mut req = req.clone();
            req.read_mask = Some(mask(&["object_id", "version"]));
            let err = get_object(&kv_rpc, kv_rpc.admitted_request(req.clone()))
                .await
                .unwrap_err();
            assert_eq!(
                &removal_outcome(&err.into_status_proto()),
                expected,
                "{req:?}"
            );
        }

        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = removal_reads.iter().map(|(req, _)| req.clone()).collect();
        batch
            .requests
            .push(selector_request(id, None, None, Some(7)));
        batch.read_mask = Some(mask(&["object_id", "version"]));
        let items = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
            .await
            .unwrap()
            .objects;
        let outcomes: Vec<_> = items
            .iter()
            .map(|item| match &item.result {
                Some(get_object_result::Result::Object(object)) => Ok(object.version.unwrap()),
                Some(get_object_result::Result::Error(status)) => removal_outcome(status),
                other => panic!("unexpected batch result {other:?}"),
            })
            .collect();
        assert_eq!(outcomes, vec![deleted_at_6, Err(None), Ok(8)]);

        mock_server.abort();
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
    /// read to a version written by the end of the checkpoint, and fails a `max_version` read
    /// whose selected version was written after the checkpoint, as Postgres does. `GetObject` and
    /// `BatchGetObjects` agree on every read, with and without object contents.
    #[tokio::test]
    async fn test_max_version_and_checkpoint_bounds_follow_the_history() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), true).await;

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
        // The newest version at or below `version` written by the served height (10), if it was
        // written by `at_checkpoint` when that is set and the object is live at it.
        let newest_at_or_below = |version: u64, at_checkpoint: Option<u64>| {
            history
                .iter()
                .rev()
                .find(|&&(changed_at, changed, _)| changed <= version && changed_at <= 10)
                .filter(|&&(changed_at, _, status)| {
                    status == ObjectStatus::Live && at_checkpoint.is_none_or(|cp| changed_at <= cp)
                })
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

    /// How a read ended: the version it found, or `NOT_FOUND` with the removal its
    /// `OBJECT_REMOVED` detail describes, if it has one, as `(removedVersion, removalKind,
    /// transaction)`.
    type RemovalOutcome = Result<u64, Option<(u64, String, Option<String>)>>;

    /// The [`RemovalOutcome`] of a read that failed with `status`.
    fn removal_outcome(status: &sui_rpc::proto::google::rpc::Status) -> RemovalOutcome {
        use sui_rpc::proto::google::rpc::ErrorInfo;
        use sui_rpc_api::ErrorReason;

        assert_eq!(status.code, tonic::Code::NotFound as i32, "{status:?}");
        let Some(info) = status
            .details
            .iter()
            .find_map(|detail| detail.to_msg::<ErrorInfo>().ok())
        else {
            return Err(None);
        };
        assert_eq!(info.reason, ErrorReason::ObjectRemoved.as_str_name());
        assert_eq!(info.domain, ErrorReason::DOMAIN);
        Err(Some((
            info.metadata["removedVersion"].parse().unwrap(),
            info.metadata["removalKind"].clone(),
            info.metadata.get("transaction").cloned(),
        )))
    }

    /// A read that resolves to a deletion or wrap answers `NOT_FOUND` with an `OBJECT_REMOVED`
    /// detail naming the removal's version and kind, and its transaction when the tombstone row
    /// records one; a read of a never-existed object, or one bounded by `at_checkpoint` whose
    /// resolved version was written after the checkpoint, answers plain `NOT_FOUND`. Latest,
    /// exact-version, `max_version` and `at_checkpoint` reads, alone and combined, single and in
    /// one batch with live and never-existed items, with and without object contents.
    #[tokio::test]
    async fn test_removed_objects_report_their_removal() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), true).await;

        // Created at version 2 in checkpoint 1, mutated at 4 in checkpoint 3, wrapped at 6 in
        // checkpoint 5 by a tombstone without its transaction, unwrapped at 8 in checkpoint 7,
        // and deleted at 9 in checkpoint 10 by a tombstone that names its transaction.
        let id = ObjectID::random();
        let never_existed = ObjectID::random();
        let deletion = TransactionDigest::random();
        let history = [
            (1, 2, ObjectStatus::Live, None),
            (3, 4, ObjectStatus::Live, None),
            (5, 6, ObjectStatus::Wrapped, None),
            (7, 8, ObjectStatus::Live, None),
            (10, 9, ObjectStatus::Deleted, Some(deletion)),
        ];
        let statuses: Vec<_> = history
            .iter()
            .map(|&(checkpoint, version, status, _)| (checkpoint, version, status))
            .collect();
        insert_history(&mock, id, &statuses).await;
        for &(_, version, status, transaction) in &history {
            if let Some(transaction) = transaction {
                mock.insert_row(
                    objects::NAME,
                    objects::encode_key(&ObjectKey(id, SequenceNumber::from_u64(version))),
                    objects::encode_tombstone(status, &transaction).unwrap(),
                )
                .await;
            }
        }

        // The outcome of reading `entry` of the history: its version if live, else its removal.
        let outcome = |entry: Option<&(u64, u64, ObjectStatus, Option<TransactionDigest>)>| {
            let Some(&(_, version, status, transaction)) = entry else {
                return Err(None);
            };
            let kind = match status {
                ObjectStatus::Live => return Ok(version),
                ObjectStatus::Deleted => "DELETED",
                ObjectStatus::Wrapped => "WRAPPED",
            };
            Err(Some((
                version,
                kind.to_owned(),
                transaction.map(|digest| digest.to_string()),
            )))
        };
        // The served height is 10, so every change counts toward `max_version`.
        let written_by =
            |checkpoint: Option<u64>, changed_at: u64| checkpoint.is_none_or(|cp| changed_at <= cp);

        let mut cases = vec![
            (
                selector_request(id, None, None, None),
                outcome(history.last()),
            ),
            (selector_request(never_existed, None, None, None), Err(None)),
            (
                selector_request(never_existed, Some(1), None, None),
                Err(None),
            ),
            (
                selector_request(never_existed, None, Some(5), None),
                Err(None),
            ),
            (
                selector_request(never_existed, None, None, Some(10)),
                Err(None),
            ),
        ];
        for checkpoint in 0..=10 {
            let at_checkpoint = history.iter().rfind(|entry| entry.0 <= checkpoint);
            cases.push((
                selector_request(id, None, None, Some(checkpoint)),
                outcome(at_checkpoint),
            ));
        }
        for version in 0..=10 {
            for at_checkpoint in std::iter::once(None).chain((0..=10).map(Some)) {
                let exact = history
                    .iter()
                    .find(|entry| entry.1 == version)
                    .filter(|entry| written_by(at_checkpoint, entry.0));
                cases.push((
                    selector_request(id, Some(version), None, at_checkpoint),
                    outcome(exact),
                ));
                let newest = history
                    .iter()
                    .rfind(|entry| entry.1 <= version)
                    .filter(|entry| written_by(at_checkpoint, entry.0));
                cases.push((
                    selector_request(id, None, Some(version), at_checkpoint),
                    outcome(newest),
                ));
            }
        }
        // Both kinds and both transaction cases are covered.
        for removed in [6, 9] {
            assert!(
                cases
                    .iter()
                    .any(|(_, expected)| matches!(expected, Err(Some((v, ..))) if *v == removed))
            );
        }

        for paths in [
            &["object_id", "version"][..],
            &["object_id", "version", "digest"],
        ] {
            for (req, expected) in &cases {
                let mut req = req.clone();
                req.read_mask = Some(mask(paths));
                let outcome = match get_object(&kv_rpc, kv_rpc.admitted_request(req.clone())).await
                {
                    Ok(response) => Ok(response.object.unwrap().version.unwrap()),
                    Err(err) => removal_outcome(&err.into_status_proto()),
                };
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
                let outcome = match &item.result {
                    Some(get_object_result::Result::Object(object)) => Ok(object.version.unwrap()),
                    Some(get_object_result::Result::Error(status)) => removal_outcome(status),
                    other => panic!("unexpected batch result {other:?}"),
                };
                assert_eq!(&outcome, expected, "BatchGetObjects {req:?} {paths:?}");
            }
        }

        // A latest or exact-version read lands on the tombstone row, which names the transaction,
        // so a removal costs it no extra read. An `at_checkpoint` read learns the removal from the
        // version index and reads only the tombstone row, for the transaction.
        for (version, at_checkpoint) in [(None, None), (Some(9), None), (None, Some(10))] {
            mock.clear_read_rows_calls().await;
            let req = selector_request(id, version, None, at_checkpoint);
            get_object(&kv_rpc, kv_rpc.admitted_request(req))
                .await
                .unwrap_err();
            assert_eq!(
                objects_reads_stripped(&mock).await.len(),
                1,
                "{version:?} {at_checkpoint:?}"
            );
        }

        mock_server.abort();
    }

    /// With version 5 written in checkpoint 10 and version 8 in checkpoint 20, `max_version` 8 at
    /// checkpoint 15 selects version 8 once the served height reaches checkpoint 20, and then
    /// fails because version 8 was written after checkpoint 15. Below that served height, version
    /// 8 does not count even with its row already in the tables, and version 5 answers.
    #[tokio::test]
    async fn test_max_version_at_checkpoint_counts_versions_up_to_the_served_height() {
        for served in [15, 19, 20, 25] {
            let mock = MockBigtableServer::new();
            let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
            let kv_rpc =
                start_kv_rpc_at_heights(addr, StagesConfig::default(), true, |_| served).await;
            let id = ObjectID::random();
            insert_history(
                &mock,
                id,
                &[(10, 5, ObjectStatus::Live), (20, 8, ObjectStatus::Live)],
            )
            .await;

            let mut cases = vec![
                (
                    selector_request(id, None, Some(8), Some(15)),
                    if served < 20 {
                        Ok(5)
                    } else {
                        Err(tonic::Code::NotFound)
                    },
                ),
                (selector_request(id, None, Some(7), Some(15)), Ok(5)),
                (
                    selector_request(id, None, Some(8), Some(9)),
                    Err(tonic::Code::NotFound),
                ),
            ];
            if served >= 20 {
                cases.push((selector_request(id, None, Some(8), Some(20)), Ok(8)));
                cases.push((selector_request(id, None, Some(8), Some(served)), Ok(8)));
            }
            for (req, expected) in &cases {
                let outcome =
                    single_outcome(get_object(&kv_rpc, kv_rpc.admitted_request(req.clone())).await);
                assert_eq!(
                    &outcome, expected,
                    "GetObject, served height {served}: {req:?}"
                );
            }
            let mut batch = BatchGetObjectsRequest::default();
            batch.requests = cases.iter().map(|(req, _)| req.clone()).collect();
            let items = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
                .await
                .unwrap()
                .objects;
            for (item, (req, expected)) in items.iter().zip_debug_eq(&cases) {
                assert_eq!(
                    &batch_outcome(item),
                    expected,
                    "BatchGetObjects, served height {served}: {req:?}"
                );
            }
            mock_server.abort();
        }
    }

    /// A `max_version` read without `at_checkpoint` is bounded by the served height (10 here): a
    /// version written above it never answers, even when its row is already in the tables.
    #[tokio::test]
    async fn test_max_version_without_checkpoint_is_bounded_by_the_served_height() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), true).await;

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
    /// with `NOT_SERVED_HERE` / `disabled_by_config` unless `at_checkpoint` and `max_version`
    /// reads are enabled, even when `at_checkpoint` is above the served height. A batch with an
    /// item the instance cannot serve fails as a whole.
    #[tokio::test]
    async fn test_max_version_and_checkpoint_bounds_need_the_flag() {
        use sui_rpc::proto::google::rpc::ErrorInfo;
        use sui_rpc::proto::sui::rpc::v2::ErrorReason;

        /// The code of a failed request, and the `cause` of its `NOT_SERVED_HERE` refusal.
        fn refusal(error: RpcError) -> (tonic::Code, Option<String>) {
            let status = error.into_status_proto();
            let cause = status
                .details
                .iter()
                .filter_map(|detail| detail.to_msg::<ErrorInfo>().ok())
                .find(|info| {
                    info.domain == ErrorReason::DOMAIN
                        && info.reason == ErrorReason::NotServedHere.as_str_name()
                })
                .and_then(|info| info.metadata.get("cause").cloned());
            (tonic::Code::from(status.code), cause)
        }

        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let id = ObjectID::random();
        insert_history(&mock, id, &[(2, 2, ObjectStatus::Live)]).await;

        let disabled = Err((
            tonic::Code::Unimplemented,
            Some("disabled_by_config".to_owned()),
        ));
        for enabled in [false, true] {
            let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), enabled).await;
            // The served height is 10.
            for (version, max_version, at_checkpoint, when_enabled) in [
                (Some(2), None, Some(5), Ok(2)),
                (None, Some(2), None, Ok(2)),
                (None, Some(2), Some(5), Ok(2)),
                (
                    Some(2),
                    None,
                    Some(11),
                    Err((tonic::Code::OutOfRange, None)),
                ),
            ] {
                let expected = if enabled {
                    when_enabled
                } else {
                    disabled.clone()
                };
                let context =
                    format!("enabled {enabled}: {version:?} {max_version:?} {at_checkpoint:?}");
                let req = selector_request(id, version, max_version, at_checkpoint);
                let request = kv_rpc.admitted_request(req.clone());
                let outcome = get_object(&kv_rpc, request)
                    .await
                    .map(|response| response.object.unwrap().version.unwrap())
                    .map_err(refusal);
                assert_eq!(outcome, expected, "GetObject {context}");

                let mut batch = BatchGetObjectsRequest::default();
                batch.requests = vec![selector_request(id, None, None, None), req];
                let outcome = match batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch)).await
                {
                    Ok(response) => {
                        batch_outcome(&response.objects[1]).map_err(|code| (code, None))
                    }
                    Err(err) => Err(refusal(err)),
                };
                assert_eq!(outcome, expected, "BatchGetObjects {context}");
            }
        }

        mock_server.abort();
    }

    /// With `at_checkpoint` and `max_version` reads enabled, the watermarks of both the versions
    /// index and the tombstones pipeline bound the served height.
    #[tokio::test]
    async fn test_object_indexes_bound_the_served_height() {
        for lagging in [
            sui_kvstore::OBJECT_VERSIONS_BY_CHECKPOINT_PIPELINE,
            sui_kvstore::OBJECT_TOMBSTONES_PIPELINE,
        ] {
            for (enabled, served) in [(false, 10), (true, 7)] {
                let mock = MockBigtableServer::new();
                let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
                let kv_rpc =
                    start_kv_rpc_at_heights(addr, StagesConfig::default(), enabled, |pipeline| {
                        if pipeline == lagging { 7 } else { 10 }
                    })
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
            .map(|version| objects::encode_key(&ObjectKey(id, SequenceNumber::from_u64(version))))
            .collect();
        assert_eq!(read_keys, requested_keys, "each key is read exactly once");

        mock_server.abort();
    }

    /// The `objects` rows that `at_checkpoint` scans name go out together, as reads of the
    /// stage's `max-seeks-per-read` keys, even when the scans finish a few at a time.
    #[tokio::test]
    async fn test_batch_at_checkpoint_rows_go_out_as_objects_stage_reads() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let stages = StagesConfig {
            objects: Some(StageConfig {
                max_seeks_per_read: Some(4),
                ..Default::default()
            }),
            ..Default::default()
        };
        // Two permits per request, and 10ms per row read, so the scans finish two at a time.
        let kv_rpc = start_kv_rpc_with_request_concurrency(addr, stages, true, 2).await;
        mock.set_read_rows_latency(Duration::from_millis(10)).await;

        // Eight objects, each live at version 1 from checkpoint 1.
        let stored: Vec<Object> = (0..8)
            .map(|_| {
                Object::with_id_owner_version_for_testing(
                    ObjectID::random(),
                    SequenceNumber::from_u64(1),
                    Owner::AddressOwner(SuiAddress::ZERO),
                )
            })
            .collect();
        for object in &stored {
            mock.insert_row(
                objects::NAME,
                objects::encode_key(&ObjectKey(object.id(), object.version())),
                objects::encode(object).unwrap(),
            )
            .await;
            mock.insert_row(
                object_versions_by_checkpoint::NAME,
                object_versions_by_checkpoint::encode_key(&object.id(), 1),
                object_versions_by_checkpoint::encode(object.version(), ObjectStatus::Live),
            )
            .await;
        }
        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = stored
            .iter()
            .map(|object| {
                let mut req = GetObjectRequest::default();
                req.object_id = Some(object.id().to_canonical_string(true));
                req.at_checkpoint = Some(5);
                req
            })
            .collect();
        batch.read_mask = Some(mask(&["version", "digest"]));

        mock.clear_read_rows_calls().await;
        let response = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
            .await
            .unwrap();
        for item in &response.objects {
            assert_eq!(item.object().version, Some(1), "{item:?}");
        }

        let read_sizes: Vec<usize> = mock
            .read_rows_calls()
            .await
            .into_iter()
            .filter(|call| call.table == objects::NAME)
            .map(|call| call.row_keys.len())
            .collect();
        assert_eq!(read_sizes, [4, 4], "eight rows in reads of four");

        mock_server.abort();
    }

    /// Every request of a batch gets its own lookup's answer, in request order, including a
    /// request that repeats an earlier one. Each `objects` row is read once: an exact version and
    /// the lookups at a checkpoint that resolve to it share one read. Each `(object_id,
    /// checkpoint)` of the version index is read once too: the three lookups at checkpoint 7 share
    /// one scan.
    #[tokio::test]
    async fn test_batch_answers_each_request_and_reads_each_row_once() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), true).await;

        // `pinned` is live at version 3 from checkpoint 5; `latest` has one version, 1.
        let pinned = Object::with_id_owner_version_for_testing(
            ObjectID::random(),
            SequenceNumber::from_u64(3),
            Owner::AddressOwner(SuiAddress::ZERO),
        );
        let latest = Object::with_id_owner_version_for_testing(
            ObjectID::random(),
            SequenceNumber::from_u64(1),
            Owner::AddressOwner(SuiAddress::ZERO),
        );
        for object in [&pinned, &latest] {
            mock.insert_row(
                objects::NAME,
                objects::encode_key(&ObjectKey(object.id(), object.version())),
                objects::encode(object).unwrap(),
            )
            .await;
        }
        mock.insert_row(
            object_versions_by_checkpoint::NAME,
            object_versions_by_checkpoint::encode_key(&pinned.id(), 5),
            object_versions_by_checkpoint::encode(pinned.version(), ObjectStatus::Live),
        )
        .await;

        let request = |object: &Object,
                       version: Option<u64>,
                       max_version: Option<u64>,
                       at_checkpoint: Option<u64>| {
            selector_request(object.id(), version, max_version, at_checkpoint)
        };
        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = vec![
            request(&pinned, Some(3), None, None),
            request(&latest, None, None, None),
            request(&pinned, None, None, Some(7)),
            request(&pinned, Some(3), None, None),
            request(&pinned, Some(4), None, None),
            request(&latest, None, None, None),
            request(&pinned, None, None, Some(11)),
            request(&pinned, Some(3), None, Some(7)),
            request(&pinned, None, Some(3), Some(7)),
        ];
        batch.read_mask = Some(mask(&["object_id", "version", "digest"]));

        mock.clear_read_rows_calls().await;
        let answers: Vec<_> = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
            .await
            .unwrap()
            .objects
            .into_iter()
            .map(|item| match item.result {
                Some(get_object_result::Result::Object(object)) => {
                    Ok((object.object_id, object.version, object.digest))
                }
                Some(get_object_result::Result::Error(status)) => Err(status.code),
                other => panic!("unexpected result: {other:?}"),
            })
            .collect();
        let found = |object: &Object| -> Result<_, i32> {
            Ok((
                Some(object.id().to_canonical_string(true)),
                Some(object.version().value()),
                Some(object.digest().to_string()),
            ))
        };
        assert_eq!(
            answers,
            vec![
                found(&pinned),
                found(&latest),
                found(&pinned),
                found(&pinned),
                Err(tonic::Code::NotFound as i32),
                found(&latest),
                Err(tonic::Code::OutOfRange as i32),
                found(&pinned),
                found(&pinned),
            ],
        );

        let calls = mock.read_rows_calls().await;
        let pinned_key = objects::encode_key(&ObjectKey(pinned.id(), pinned.version()));
        let pinned_reads = calls
            .iter()
            .filter(|call| call.table == objects::NAME)
            .flat_map(|call| &call.row_keys)
            .filter(|key| **key == pinned_key)
            .count();
        assert_eq!(pinned_reads, 1, "four lookups share one read of the row");
        // One scan at checkpoint 7, and one at the served height for the `max_version` lookup.
        let index_scans = calls
            .iter()
            .filter(|call| call.table == object_versions_by_checkpoint::NAME)
            .count();
        assert_eq!(index_scans, 2, "{calls:?}");

        mock_server.abort();
    }

    /// A batch whose scans outnumber its request's permits still completes: a scan that holds a
    /// permit runs to its end and returns the permit, whether or not the `objects` reads it feeds
    /// are taking lookups.
    #[tokio::test]
    async fn test_batch_scans_do_not_starve_object_reads_of_permits() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        // Two permits per request, and one `objects` read of one key at a time.
        let stages = StagesConfig {
            objects: Some(StageConfig {
                max_seeks_per_read: Some(1),
                concurrency: Some(1),
                ..Default::default()
            }),
            ..Default::default()
        };
        let kv_rpc = start_kv_rpc_with_request_concurrency(addr, stages, true, 2).await;

        // Eight objects, each live at version 1 from checkpoint 1.
        let stored: Vec<Object> = (0..8)
            .map(|_| {
                Object::with_id_owner_version_for_testing(
                    ObjectID::random(),
                    SequenceNumber::from_u64(1),
                    Owner::AddressOwner(SuiAddress::ZERO),
                )
            })
            .collect();
        for object in &stored {
            mock.insert_row(
                objects::NAME,
                objects::encode_key(&ObjectKey(object.id(), object.version())),
                objects::encode(object).unwrap(),
            )
            .await;
            mock.insert_row(
                object_versions_by_checkpoint::NAME,
                object_versions_by_checkpoint::encode_key(&object.id(), 1),
                object_versions_by_checkpoint::encode(object.version(), ObjectStatus::Live),
            )
            .await;
        }

        // Each object at version 1, latest, and at checkpoint 5.
        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = stored
            .iter()
            .flat_map(|object| {
                [(Some(1), None), (None, None), (None, Some(5))].map(|(version, at_checkpoint)| {
                    let mut req = GetObjectRequest::default();
                    req.object_id = Some(object.id().to_canonical_string(true));
                    req.version = version;
                    req.at_checkpoint = at_checkpoint;
                    req
                })
            })
            .collect();
        batch.read_mask = Some(mask(&["object_id", "version", "digest"]));

        let response = tokio::time::timeout(
            Duration::from_secs(10),
            batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch)),
        )
        .await
        .expect("the batch stalled on its own limiter")
        .unwrap();
        assert_eq!(response.objects.len(), 24);
        for item in &response.objects {
            assert_eq!(item.object().version, Some(1), "{item:?}");
        }

        mock_server.abort();
    }

    #[tokio::test]
    async fn test_batch_get_objects_limit_exceeded() {
        let mock = MockBigtableServer::new();
        let (addr, server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), false).await;

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
