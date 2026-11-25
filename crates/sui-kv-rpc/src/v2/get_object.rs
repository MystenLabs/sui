// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::StreamExt;
use futures::TryStreamExt;
use futures::future;
use futures::stream;
use futures::stream::BoxStream;
use futures::stream::FuturesUnordered;
use std::collections::HashMap;
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
use crate::request_cache::{CacheFetcher, CachedValue, RequestCache};
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
/// `NOT_FOUND`, with an `OBJECT_REMOVED` detail where object version history is enabled (see
/// [`respond`]). Every BigTable read goes through the request's limiter.
pub(crate) async fn get_object(
    server: &KvRpcServer,
    request: tonic::Request<GetObjectRequest>,
) -> Result<GetObjectResponse, RpcError> {
    let served_height = ServedHeight::of(&request);
    let mut request = request.into_inner();
    let read_mask = request.read_mask.take();
    let (requests, read_mask) =
        validate_get_object_requests(std::slice::from_ref(&request), read_mask, false)?;
    let (address, selector) = requests[0];
    let object_id: ObjectID = address.into();
    let client = server.limited_client("GetObject");
    // Both checks fail the request whatever the object's state, so they come before the reads.
    let at_checkpoint_reads = if selector.selects_version_by_bound() {
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
    let answer = match plan_lookup(object_id, selector, at_checkpoint_reads.as_ref()) {
        LookupPlan::Resolved(answer) => answer,
        LookupPlan::NeedsScan(scan) => scan.run(&objects).await?,
    };
    let rows = match answer.row_key() {
        Some(key) => objects.rows(vec![key]).await?,
        None => HashMap::new(),
    };
    let found = answer.complete(&rows).await??;
    let message = respond(
        found.as_ref(),
        object_id,
        selector.not_found(address),
        server.alpha_features.object_at_checkpoint,
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
///    lookup bounded by `at_checkpoint` names its row at once and starts its index read as it is
///    planned, and its row counts only if the index shows the version was written by then; an
///    `at_checkpoint` or `max_version` lookup is answered as [`bounded_read`] decides, by a live
///    version's row, by the tombstone row of the removal the index records, or by a reverse scan
///    of `objects` down from `max_version`. The scans run in groups of at most the `objects`
///    stage's `max-seeks-per-read`, each group one read of a [`pipelined_chunks`] stage, at most
///    the stage's `concurrency` groups at a time. Each lookup's answer moves on as its own scan
///    ends, without waiting for the rest of its group.
/// 2. The rows the lookups name are read as the lookups arrive, by [`pipelined_keyed_batches`]
///    under the `objects` stage ([`crate::config::StagesConfig::objects`]): reads of at most
///    `max-seeks-per-read` keys, at most `concurrency` of them at a time, and each row at most
///    once per request. A lookup whose row is missing is `NOT_FOUND`, unless the version index
///    named the row: then the index and `objects` disagree, and the lookup is `INTERNAL`, as it
///    is when the row is not the live object or the tombstone the index records. A lookup whose
///    row is a tombstone is otherwise `NOT_FOUND`, with an `OBJECT_REMOVED` detail where object
///    version history is enabled (see [`respond`]).
/// 3. Each lookup is rendered once, at most `DEFAULT_RENDER_AHEAD` at a time (the list endpoints'
///    default render-ahead), and every request for it gets the result.
///
/// Every BigTable read of the batch, scans and `objects` reads alike, shares one request-scoped
/// limiter, and each `(object_id, checkpoint)` of the version index is read at most once per
/// request. Rendering is not limited by it: package loads go through the server-wide
/// [`PackageResolver`]. A lookup whose `at_checkpoint` is above the served height fails without
/// a read, and a batch with a lookup that the instance cannot serve fails as a whole.
///
/// When the read mask asks for nothing but `object_id`, `version` and `display`, which kv-rpc
/// does not render, a lookup that the version index resolves to a live version is answered by
/// its scans alone, and the `objects` reads fetch only row keys and tombstones, not object
/// contents.
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
    let (requests, read_mask) = validate_get_object_requests(&requests, read_mask, true)?;
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
        .any(|(_, selector)| selector.selects_version_by_bound())
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

    let mut resolved_without_scan = Vec::new();
    let mut scans = Vec::new();
    for (index, &(address, selector)) in lookups.iter().enumerate() {
        match plan_lookup(address.into(), selector, at_checkpoint_reads.as_ref()) {
            LookupPlan::Resolved(answer) => resolved_without_scan.push((index, answer)),
            LookupPlan::NeedsScan(scan) => scans.push((index, scan)),
        }
    }

    // Scans run in groups of at most the `objects` stage's `max-seeks-per-read`, at most its
    // `concurrency` groups at a time. A group's drainer task runs its scans at once and each to
    // its end, so a scan returns its permit whether or not the `objects` reads below are taking
    // lookups, and each lookup's answer moves on as its own scan ends. Answers carry their
    // lookup's index, so the order they end in does not matter.
    let objects_stage = server.stages.stage(PipelineStage::Objects);
    let scans: BoxStream<'static, Result<Watermarked<(usize, Scan)>, RpcError>> =
        stream::iter(scans.into_iter().map(|scan| Ok(Watermarked::Item(scan)))).boxed();
    let scanned = pipelined_chunks(
        scans,
        objects_stage.max_seeks_per_read,
        objects_stage.concurrency,
        None,
        SeekUnit::Item,
        {
            let objects = objects.clone();
            move |group: Vec<(usize, Scan)>| {
                let answers: FuturesUnordered<_> = group
                    .into_iter()
                    .map(|(index, scan)| {
                        let objects = objects.clone();
                        async move { Ok::<_, RpcError>((index, scan.run(&objects).await?)) }
                    })
                    .collect();
                future::ready(Ok::<_, RpcError>(answers.boxed()))
            }
        },
    );
    // Exact versions go out in key order, so an `objects` read covers one run of adjacent keys.
    resolved_without_scan.sort_by_key(|(_, answer)| answer.row_key());
    let resolved_without_scan = stream::iter(
        resolved_without_scan
            .into_iter()
            .map(|answer| Ok(Watermarked::Item(answer))),
    )
    .boxed();
    let lookups_with_keys: MarkedKeyedUpstream<(usize, ObjectsStageInput), ObjectKey, RpcError> =
        stream::select(resolved_without_scan, scanned)
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
        let removal_details = server.alpha_features.object_at_checkpoint;
        move |((index, answer), rows)| {
            let (address, selector) = lookups[index];
            let read_mask = read_mask.clone();
            let resolver = resolver.clone();
            async move {
                // A failed read fails the whole batch; the lookup's own answer, error or not,
                // goes to its requests.
                let result = match answer.complete(&rows).await? {
                    Ok(found) => respond(
                        found.as_ref(),
                        address.into(),
                        selector.not_found(address),
                        removal_details,
                        &read_mask,
                        &resolver,
                    )
                    .await
                    .map(GetObjectResult::new_object),
                    Err(err) => Err(err),
                };
                let result = result
                    .unwrap_or_else(|err| GetObjectResult::new_error(err.into_status_proto()));
                Ok::<_, RpcError>((index, result))
            }
        }
    });

    let mut results: Vec<Option<GetObjectResult>> = vec![None; lookup_count];
    while let Some(frame) = rendered.next().await {
        match frame {
            Ok(ResolvedWatermarked::Item((index, result))) => results[index] = Some(result),
            Ok(ResolvedWatermarked::Watermark { .. }) => {}
            Err(RenderAheadError::Upstream(err) | RenderAheadError::Render(err)) => {
                return Err(err);
            }
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

/// The bounds of an `at_checkpoint` or `max_version` read: it selects the object's newest version
/// at or below `max_version` written by the end of checkpoint `height`, and finds nothing if that
/// version was written after `checkpoint`, or deleted or wrapped the object.
#[derive(Clone, Copy, Debug)]
struct Bounds {
    max_version: u64,
    checkpoint: u64,
    height: u64,
}

/// How an `at_checkpoint` or `max_version` read is answered. See [`bounded_read`].
#[derive(Clone, Copy, Debug)]
enum BoundedRead {
    /// No version answers it.
    NotFound,
    /// The object's entry at `version`, its version at the end of the checkpoint, answers it: the
    /// object if `status` is `Live`, else the removal's tombstone row in the `objects` table.
    Indexed {
        version: SequenceNumber,
        status: ObjectStatus,
    },
    /// The newest version at or below `from`, from an `objects` reverse scan, answers it. `from`
    /// is below the object's version at the end of the checkpoint, so every version the scan can
    /// find was written by then.
    ScanBelow(SequenceNumber),
    /// The object's entry at the end of the checkpoint is at `version`, with `status`, and the
    /// object changed again before the height. The newest version at or below `from`, from an
    /// `objects` reverse scan, is selected: one above `version` was written after the checkpoint,
    /// so nothing answers; otherwise it must be the index's entry (see [`indexed_entry`]), which
    /// answers.
    ScanToIndexed {
        from: SequenceNumber,
        version: SequenceNumber,
        status: ObjectStatus,
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
        return BoundedRead::ScanBelow(from);
    }
    match version_at_height {
        Some((at_height, _)) if at_height > at_checkpoint => {
            if max_version >= at_height.value() {
                BoundedRead::NotFound
            } else {
                BoundedRead::ScanToIndexed {
                    from,
                    version: at_checkpoint,
                    status,
                }
            }
        }
        _ => BoundedRead::Indexed {
            version: at_checkpoint,
            status,
        },
    }
}

/// The entry the version index records at `key`, with `status`, from `row`, the `objects` row a
/// read found for it: the live object at `key`'s version if `status` is `Live`, else the tombstone
/// of a removal of that kind at that version. Anything else, no row included, means the index and
/// `objects` diverged, which is `INTERNAL`.
fn indexed_entry(
    key: ObjectKey,
    status: ObjectStatus,
    row: Option<&Found>,
) -> Result<Found, RpcError> {
    let agrees = |found: &Found| match found {
        Found::Object(_) | Found::Version(_) => status == ObjectStatus::Live,
        Found::Removed(removal) => {
            status != ObjectStatus::Live && removal.kind == removal_kind(status)
        }
    };
    match row {
        Some(found) if found.version() == key.1 && agrees(found) => Ok(found.clone()),
        _ => Err(RpcError::new(
            tonic::Code::Internal,
            format!(
                "object_versions_by_checkpoint names {key:?} as {status:?}, but objects has no \
                 such row"
            ),
        )),
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
            ObjectVersionRow::Tombstone(ObjectTombstone { status, .. }) => {
                Found::Removed(ObjectRemoval {
                    version: version.value(),
                    kind: removal_kind(status),
                })
            }
        }
    }

    fn version(&self) -> SequenceNumber {
        match self {
            Found::Object(object) => object.version(),
            Found::Version(version) => *version,
            Found::Removed(removal) => SequenceNumber::from_u64(removal.version),
        }
    }
}

/// The answer to a read of `object_id` that found `found`: the live version rendered with
/// `read_mask`, or `not_found`. With `removal_details`, object version history is enabled
/// (`enable-object-at-checkpoint`), the operator's statement that every tombstone is backfilled:
/// a read that resolved to a deletion or wrap adds an `OBJECT_REMOVED` detail. Without it,
/// tombstones may be missing, so no read describes a removal.
async fn respond(
    found: Option<&Found>,
    object_id: ObjectID,
    not_found: ObjectNotFoundError,
    removal_details: bool,
    read_mask: &FieldMaskTree,
    resolver: &PackageResolver,
) -> Result<ProtoObject, RpcError> {
    match found {
        Some(Found::Object(object)) => Ok(object_to_response(object, read_mask, resolver).await),
        Some(Found::Version(version)) => Ok(object_version_to_response(object_id, *version)),
        Some(Found::Removed(_)) if !removal_details => Err(not_found.into()),
        Some(Found::Removed(removal)) => Err(not_found.removed(*removal).into()),
        None => Err(not_found.into()),
    }
}

/// A batch lookup after its `objects` reads: its index in the batch's lookups, its
/// [`ObjectsStageInput`], and the rows of the keys it listed.
type LookupWithRows = KeyedBatchOutput<(usize, ObjectsStageInput), ObjectKey, Found>;

/// How a lookup is answered before any `objects` multi-get: resolved up to one `objects` row (or
/// already failed), possibly with a version index read already running, or only after a scan.
enum LookupPlan {
    Resolved(ObjectsStageInput),
    NeedsScan(Scan),
}

/// Plans the lookup of `object_id` by `selector`. A selector that picks its version by a bound
/// reads the version index through `at_checkpoint_reads`, and fails without a read if its
/// `at_checkpoint` is above the served height.
fn plan_lookup(
    object_id: ObjectID,
    selector: ObjectSelector,
    at_checkpoint_reads: Option<&Arc<AtCheckpointReads>>,
) -> LookupPlan {
    match selector {
        ObjectSelector::Latest => LookupPlan::NeedsScan(Scan::Latest(object_id)),
        ObjectSelector::Version {
            version,
            at_checkpoint: None,
        } => LookupPlan::Resolved(ObjectsStageInput::RequestedRow(ObjectKey(
            object_id,
            version.into(),
        ))),
        ObjectSelector::Version {
            at_checkpoint: Some(_),
            ..
        }
        | ObjectSelector::MaxVersion { .. }
        | ObjectSelector::AtCheckpoint(_) => at_checkpoint_reads
            .ok_or_else(|| {
                RpcError::new(
                    tonic::Code::Internal,
                    "a read that picks its version by a bound has no version index reads",
                )
            })
            .and_then(|reads| reads.plan(object_id, selector))
            .unwrap_or_else(|err| LookupPlan::Resolved(ObjectsStageInput::Failed(err))),
    }
}

/// A scan that a lookup starts with, before any `objects` multi-get.
enum Scan {
    /// The newest version of the object, from one reverse scan of `objects`.
    Latest(ObjectID),
    /// An `at_checkpoint` or `max_version` read, answered as [`bounded_read`] decides.
    Bounded {
        reads: Arc<AtCheckpointReads>,
        object_id: ObjectID,
        bounds: Bounds,
    },
}

impl Scan {
    async fn run(self, objects: &ObjectReads) -> Result<ObjectsStageInput, RpcError> {
        Ok(match self {
            Scan::Latest(object_id) => ObjectsStageInput::Scanned(objects.latest(object_id).await?),
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
                    BoundedRead::Indexed {
                        version,
                        status: ObjectStatus::Live,
                    } if objects.version_only => {
                        ObjectsStageInput::Scanned(Some(Found::Version(version)))
                    }
                    BoundedRead::Indexed { version, status } => ObjectsStageInput::Indexed {
                        key: ObjectKey(object_id, version),
                        status,
                    },
                    BoundedRead::ScanBelow(from) => ObjectsStageInput::Scanned(
                        objects
                            .newest(object_id, from, !objects.version_only)
                            .await?,
                    ),
                    BoundedRead::ScanToIndexed {
                        from,
                        version,
                        status,
                    } => {
                        // Only a live entry answers with contents, so a scan for a removal skips
                        // live objects' contents.
                        let with_contents = status == ObjectStatus::Live && !objects.version_only;
                        match objects.newest(object_id, from, with_contents).await? {
                            Some(found) if found.version() > version => {
                                ObjectsStageInput::Scanned(None)
                            }
                            found => match indexed_entry(
                                ObjectKey(object_id, version),
                                status,
                                found.as_ref(),
                            ) {
                                Ok(entry) => ObjectsStageInput::Scanned(Some(entry)),
                                Err(err) => ObjectsStageInput::Failed(err),
                            },
                        }
                    }
                }
            }
        })
    }
}

/// What a lookup carries into its `objects` reads: its answer as far as it is known before
/// them. A batch hands it to the `objects` stage; `GetObject` reads its row with
/// `ObjectReads::rows`.
enum ObjectsStageInput {
    /// The `objects` row at the version the request asked for answers the lookup; without the
    /// row it is `NOT_FOUND`.
    RequestedRow(ObjectKey),
    /// A `version` lookup bounded by a checkpoint: the `objects` row at `key` answers it if
    /// `version_at_checkpoint`, the object's [`VersionAtCheckpoint`] there, read alongside the
    /// row, shows that the version was written by the end of the checkpoint. When that version is
    /// `key`'s, the row must be the index's entry (see [`indexed_entry`]).
    WrittenBy {
        key: ObjectKey,
        version_at_checkpoint: CachedValue<(SequenceNumber, ObjectStatus)>,
    },
    /// The `objects` row at this key, which the version index names as live or as a removal, as
    /// `status` says, answers the lookup (see [`indexed_entry`]).
    Indexed {
        key: ObjectKey,
        status: ObjectStatus,
    },
    /// The lookup's scan answered it; `None` makes it `NOT_FOUND`.
    Scanned(Option<Found>),
    /// The lookup fails with this error, without a read.
    Failed(RpcError),
}

impl ObjectsStageInput {
    /// The `objects` row the lookup still needs, if any.
    fn row_key(&self) -> Option<ObjectKey> {
        match self {
            ObjectsStageInput::RequestedRow(key)
            | ObjectsStageInput::WrittenBy { key, .. }
            | ObjectsStageInput::Indexed { key, .. } => Some(*key),
            ObjectsStageInput::Scanned(_) | ObjectsStageInput::Failed(_) => None,
        }
    }

    /// The `objects` rows the lookup still needs.
    fn row_keys(&self) -> Vec<ObjectKey> {
        self.row_key().into_iter().collect()
    }

    /// The lookup's answer, given the rows the `objects` reads returned for its keys: the outer
    /// error is a failed read, which fails the request (a whole batch); the inner result is the
    /// lookup's own answer, where `Ok(None)` means `NOT_FOUND`.
    async fn complete(
        self,
        rows: &HashMap<ObjectKey, Found>,
    ) -> Result<Result<Option<Found>, RpcError>, RpcError> {
        Ok(match self {
            ObjectsStageInput::RequestedRow(key) => Ok(rows.get(&key).cloned()),
            ObjectsStageInput::WrittenBy {
                key,
                version_at_checkpoint,
            } => match version_at_checkpoint.await? {
                // The index records the requested version, so its row must agree with the index,
                // as for an `at_checkpoint` read alone.
                Some((version, status)) if version == key.1 => {
                    indexed_entry(key, status, rows.get(&key)).map(Some)
                }
                // An object's versions increase in checkpoint order, so a version below its
                // version at the end of the checkpoint was written by then.
                Some((version, _)) if key.1 < version => Ok(rows.get(&key).cloned()),
                // Written after the checkpoint, or the object did not exist yet.
                _ => Ok(None),
            },
            ObjectsStageInput::Indexed { key, status } => {
                indexed_entry(key, status, rows.get(&key)).map(Some)
            }
            ObjectsStageInput::Scanned(found) => Ok(found),
            ObjectsStageInput::Failed(err) => Err(err),
        })
    }
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

    /// The rows at the versions `keys` name, in one read; a key without a row has no entry.
    async fn rows(&self, keys: Vec<ObjectKey>) -> Result<HashMap<ObjectKey, Found>, RpcError> {
        // The read runs to its end, so it finishes as completed rather than cut off.
        self.fetch(keys).await?.try_collect().await
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
        if !self.alpha_features.object_at_checkpoint {
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
    /// The plan of a lookup that picks its version by a bound: a `version` bounded by
    /// `at_checkpoint`, a `max_version`, or an `at_checkpoint` alone. Fails if the lookup's
    /// `at_checkpoint` is above the served height.
    ///
    /// A `version` bounded by `at_checkpoint` names its row at once, and its version index read
    /// starts here, so it runs alongside the row's read. A `max_version` read counts versions up
    /// to the served height, and its `at_checkpoint` defaults to it. An `at_checkpoint` read is a
    /// `max_version` read with no bound on the version, counting versions up to the checkpoint.
    fn plan(
        self: &Arc<Self>,
        object_id: ObjectID,
        selector: ObjectSelector,
    ) -> Result<LookupPlan, RpcError> {
        let served_checkpoint = self.served_height.checkpoint;
        if let Some(checkpoint) = selector.at_checkpoint() {
            AtCheckpointUpperBound::new(served_checkpoint).check(checkpoint)?;
        }
        let reads = self.clone();
        let scan = match selector {
            ObjectSelector::Version {
                version,
                at_checkpoint: Some(checkpoint),
            } => {
                return Ok(LookupPlan::Resolved(ObjectsStageInput::WrittenBy {
                    key: ObjectKey(object_id, version.into()),
                    version_at_checkpoint: self.version_at_checkpoint(object_id, checkpoint),
                }));
            }
            ObjectSelector::MaxVersion {
                max_version,
                at_checkpoint,
            } => Scan::Bounded {
                reads,
                object_id,
                bounds: Bounds {
                    max_version,
                    checkpoint: at_checkpoint.unwrap_or(served_checkpoint),
                    height: served_checkpoint,
                },
            },
            ObjectSelector::AtCheckpoint(checkpoint) => Scan::Bounded {
                reads,
                object_id,
                bounds: Bounds {
                    max_version: u64::MAX,
                    checkpoint,
                    height: checkpoint,
                },
            },
            ObjectSelector::Latest
            | ObjectSelector::Version {
                at_checkpoint: None,
                ..
            } => {
                return Err(RpcError::new(
                    tonic::Code::Internal,
                    "not an at_checkpoint or max_version read",
                ));
            }
        };
        Ok(LookupPlan::NeedsScan(scan))
    }

    /// The version and status `object_id` had at the end of `checkpoint`. The first call for an
    /// `(object_id, checkpoint)` starts reading it at once, in a task the cache owns, so the read
    /// runs and returns its permit whether or not the returned future is polled; later calls share
    /// that read.
    pub(super) fn version_at_checkpoint(
        &self,
        object_id: ObjectID,
        checkpoint: u64,
    ) -> CachedValue<(SequenceNumber, ObjectStatus)> {
        let key = (object_id, checkpoint);
        let mut reservation = self.versions.reserve(&[key]);
        if reservation.is_unread(&key) {
            // The read's end needs no wait: its value arrives through the reservation.
            let _read_end = self.versions.read(reservation.take_unread(&[key]));
        }
        reservation.value(&key)
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
    use crate::AlphaFeatures;
    use crate::StageConfig;
    use crate::StagesConfig;
    use crate::v2::test_utils::OBJECT_AT_CHECKPOINT;
    use crate::v2::test_utils::insert_history;
    use crate::v2::test_utils::mask;
    use crate::v2::test_utils::start_kv_rpc;
    use crate::v2::test_utils::start_kv_rpc_at_heights;
    use crate::v2::test_utils::start_kv_rpc_with_request_concurrency;

    /// For each `objects`-table read since the last clear, whether it asked for live objects'
    /// contents to be stripped, keeping tombstones whole.
    async fn objects_reads_stripped(mock: &MockBigtableServer) -> Vec<bool> {
        mock.read_rows_calls()
            .await
            .into_iter()
            .filter(|call| call.table == objects::NAME)
            .map(|call| call.values_stripped_in == [objects::col::OBJECT.as_bytes()])
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
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), OBJECT_AT_CHECKPOINT).await;

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
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), AlphaFeatures::default()).await;

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
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), OBJECT_AT_CHECKPOINT).await;

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

    /// A `version` read bounded by `at_checkpoint` reads the object's row alongside its version
    /// index read, alone and in a batch: with every BigTable response held back, both reads
    /// arrive before either is answered.
    #[tokio::test]
    async fn test_version_at_checkpoint_reads_row_alongside_index() {
        /// Waits while `read` runs until reads of both the version index and `objects` have
        /// arrived at `mock`. Fails if `read` ends first or they do not arrive in time.
        async fn assert_reads_overlap(
            mock: &MockBigtableServer,
            read: impl Future<Output = ()>,
            name: &str,
        ) {
            let both_arrived = async {
                loop {
                    let calls = mock.read_rows_calls().await;
                    let arrived = |table: &str| calls.iter().any(|call| call.table == table);
                    if arrived(object_versions_by_checkpoint::NAME) && arrived(objects::NAME) {
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            };
            tokio::select! {
                () = read => panic!("{name} ended before BigTable answered"),
                arrived = tokio::time::timeout(Duration::from_secs(10), both_arrived) => {
                    arrived.unwrap_or_else(|_| {
                        panic!("{name} read the objects row only after the version index")
                    });
                }
            }
        }

        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), OBJECT_AT_CHECKPOINT).await;
        let id = ObjectID::random();
        insert_history(&mock, id, &[(5, 3, ObjectStatus::Live)]).await;
        // Every row of a response waits a minute, far longer than the reads are waited for.
        mock.set_read_rows_latency(Duration::from_secs(60)).await;

        let req = selector_request(id, Some(3), None, Some(7));
        let single = async {
            let _ = get_object(&kv_rpc, kv_rpc.admitted_request(req.clone())).await;
        };
        assert_reads_overlap(&mock, single, "GetObject").await;

        mock.clear_read_rows_calls().await;
        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = vec![req];
        let batched = async {
            let _ = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch)).await;
        };
        assert_reads_overlap(&mock, batched, "BatchGetObjects").await;

        mock_server.abort();
    }

    /// A `max_version` read whose scan from `max_version` must find the version the index names
    /// as live at the checkpoint, but finds an older one, a tombstone there, or no row at all, is
    /// `INTERNAL`, alone and in a batch, with and without object contents: the index and
    /// `objects` disagree.
    #[tokio::test]
    async fn test_max_version_scan_below_indexed_version_is_internal() {
        /// What `objects` holds at or below version 6.
        #[derive(Debug)]
        enum AtOrBelowSix {
            OlderVersion,
            TombstoneAtIndexedVersion,
            NoRow,
        }

        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), OBJECT_AT_CHECKPOINT).await;

        // `objects` holds version 8, and at or below 6 version 3, a tombstone at 5, or nothing.
        // The index has the object live at 5 from checkpoint 3, and at 8 from checkpoint 6.
        for at_or_below_six in [
            AtOrBelowSix::OlderVersion,
            AtOrBelowSix::TombstoneAtIndexedVersion,
            AtOrBelowSix::NoRow,
        ] {
            let id = ObjectID::random();
            let live_versions: &[u64] = match at_or_below_six {
                AtOrBelowSix::OlderVersion => &[3, 8],
                AtOrBelowSix::TombstoneAtIndexedVersion | AtOrBelowSix::NoRow => &[8],
            };
            for &version in live_versions {
                let version = SequenceNumber::from_u64(version);
                mock.insert_row(
                    objects::NAME,
                    objects::encode_key(&ObjectKey(id, version)),
                    objects::encode(&Object::with_id_owner_version_for_testing(
                        id,
                        version,
                        Owner::AddressOwner(SuiAddress::ZERO),
                    ))
                    .unwrap()
                    .to_vec(),
                )
                .await;
            }
            if let AtOrBelowSix::TombstoneAtIndexedVersion = at_or_below_six {
                mock.insert_row(
                    objects::NAME,
                    objects::encode_key(&ObjectKey(id, SequenceNumber::from_u64(5))),
                    objects::encode_tombstone(ObjectStatus::Deleted, &TransactionDigest::random())
                        .unwrap(),
                )
                .await;
            }
            for (checkpoint, version) in [(3, 5), (6, 8)] {
                mock.insert_row(
                    object_versions_by_checkpoint::NAME,
                    object_versions_by_checkpoint::encode_key(&id, checkpoint),
                    object_versions_by_checkpoint::encode(
                        SequenceNumber::from_u64(version),
                        ObjectStatus::Live,
                    ),
                )
                .await;
            }

            for read_mask in [
                &["object_id", "version"][..],
                &["object_id", "version", "digest"],
            ] {
                let context = format!("{at_or_below_six:?}, {read_mask:?}");
                let mut req = selector_request(id, None, Some(6), Some(4));
                req.read_mask = Some(mask(read_mask));
                assert_eq!(
                    single_outcome(get_object(&kv_rpc, kv_rpc.admitted_request(req.clone())).await),
                    Err(tonic::Code::Internal),
                    "{context}"
                );

                let mut batch = BatchGetObjectsRequest::default();
                batch.requests = vec![selector_request(id, Some(8), None, None), req];
                batch.read_mask = Some(mask(read_mask));
                let items = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
                    .await
                    .unwrap()
                    .objects;
                assert_eq!(batch_outcome(&items[0]), Ok(8), "{context}");
                assert_eq!(
                    batch_outcome(&items[1]),
                    Err(tonic::Code::Internal),
                    "{context}"
                );
            }
        }

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
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), AlphaFeatures::default()).await;

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
    /// the same index and storage divergence: an `at_checkpoint` read of it, alone or naming that
    /// `version`, is `INTERNAL`, alone and in a batch. A request that names the version without
    /// `at_checkpoint` reads the tombstone as a removal.
    #[tokio::test]
    async fn test_at_checkpoint_live_version_with_tombstone_row_is_internal() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), OBJECT_AT_CHECKPOINT).await;

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

        for req in [
            selector_request(id, None, None, Some(7)),
            selector_request(id, Some(3), None, Some(7)),
        ] {
            let mut req = req;
            req.read_mask = Some(mask(&contents));
            let status: tonic::Status = get_object(&kv_rpc, kv_rpc.admitted_request(req.clone()))
                .await
                .unwrap_err()
                .into();
            assert_eq!(status.code(), tonic::Code::Internal, "{req:?}: {status:?}");
        }

        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = vec![
            selector_request(id, None, None, Some(7)),
            selector_request(id, Some(3), None, None),
            selector_request(id, Some(3), None, Some(7)),
        ];
        batch.read_mask = Some(mask(&contents));
        let items = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
            .await
            .unwrap()
            .objects;
        for item in [&items[0], &items[2]] {
            assert_eq!(item.error().code, tonic::Code::Internal as i32, "{item:?}");
        }
        assert!(
            matches!(removal_outcome(items[1].error()), Err(Some((3, _)))),
            "version 3: {:?}",
            items[1]
        );

        mock_server.abort();
    }

    /// With object version history enabled, a removal's tombstone row is required. A read that
    /// resolves to a removal fails if the row cannot be read, alone and with its whole batch, as
    /// any failed `objects` read does. It is `INTERNAL`, alone and as a batch item, if the version
    /// index records the removal but `objects` has no tombstone there. Without object version
    /// history, a read that lands on a tombstone answers plain `NOT_FOUND`.
    #[tokio::test]
    async fn test_removal_tombstones_are_required_with_object_version_history() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), OBJECT_AT_CHECKPOINT).await;

        // Each object is created at version 2 in checkpoint 1, deleted at 6 in checkpoint 5, and
        // created again at 8 in checkpoint 7: `at_checkpoint` 5, alone and with `version` 6, reads
        // the deletion's tombstone row, and `max_version` 7 at checkpoint 5 scans for it, since
        // the object changed again by the served height.
        let history = [
            (1, 2, ObjectStatus::Live),
            (5, 6, ObjectStatus::Deleted),
            (7, 8, ObjectStatus::Live),
        ];
        let removal_reads = |id| {
            [
                selector_request(id, None, None, Some(5)),
                selector_request(id, Some(6), None, Some(5)),
                selector_request(id, None, Some(7), Some(5)),
            ]
        };
        // A tombstone row that cannot be decoded.
        let unreadable = ObjectID::random();
        insert_history(&mock, unreadable, &history).await;
        mock.insert_row(
            objects::NAME,
            objects::encode_key(&ObjectKey(unreadable, SequenceNumber::from_u64(6))),
            [(objects::col::STATUS, vec![1u8, 2].into())],
        )
        .await;
        // No `objects` rows at all: only the version index records the history.
        let missing = ObjectID::random();
        for &(checkpoint, version, status) in &history {
            mock.insert_row(
                object_versions_by_checkpoint::NAME,
                object_versions_by_checkpoint::encode_key(&missing, checkpoint),
                object_versions_by_checkpoint::encode(SequenceNumber::from_u64(version), status),
            )
            .await;
        }
        // An object with the same history and every row in place.
        let recreated = ObjectID::random();
        insert_history(&mock, recreated, &history).await;
        let live = selector_request(recreated, None, None, Some(7));

        for req in removal_reads(unreadable) {
            let outcome =
                single_outcome(get_object(&kv_rpc, kv_rpc.admitted_request(req.clone())).await);
            assert!(
                matches!(outcome, Err(code) if code != tonic::Code::NotFound),
                "{req:?}: {outcome:?}"
            );
            let mut batch = BatchGetObjectsRequest::default();
            batch.requests = vec![req.clone(), live.clone()];
            assert!(
                batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
                    .await
                    .is_err(),
                "{req:?}"
            );
        }

        let mut cases = vec![(live, Ok(8))];
        for req in removal_reads(missing) {
            cases.push((req, Err(tonic::Code::Internal)));
        }
        for (req, expected) in &cases {
            let outcome =
                single_outcome(get_object(&kv_rpc, kv_rpc.admitted_request(req.clone())).await);
            assert_eq!(&outcome, expected, "GetObject {req:?}");
        }
        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = cases.iter().map(|(req, _)| req.clone()).collect();
        let items = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch))
            .await
            .unwrap()
            .objects;
        for (item, (req, expected)) in items.iter().zip_debug_eq(&cases) {
            assert_eq!(&batch_outcome(item), expected, "BatchGetObjects {req:?}");
        }

        let without_history =
            start_kv_rpc(addr, StagesConfig::default(), AlphaFeatures::default()).await;
        let removed = ObjectID::random();
        insert_history(
            &mock,
            removed,
            &[(1, 2, ObjectStatus::Live), (5, 6, ObjectStatus::Deleted)],
        )
        .await;
        for req in [
            selector_request(removed, None, None, None),
            selector_request(removed, Some(6), None, None),
        ] {
            let err = get_object(
                &without_history,
                without_history.admitted_request(req.clone()),
            )
            .await
            .unwrap_err();
            assert_eq!(
                removal_outcome(&err.into_status_proto()),
                Err(None),
                "{req:?}"
            );
            let mut batch = BatchGetObjectsRequest::default();
            batch.requests = vec![req.clone()];
            let items =
                batch_get_objects(&without_history, without_history.admitted_request(batch))
                    .await
                    .unwrap()
                    .objects;
            assert_eq!(removal_outcome(items[0].error()), Err(None), "{req:?}");
        }

        mock_server.abort();
    }

    /// A `version` read bounded by `at_checkpoint` whose version index read fails fails the
    /// request, alone and with its whole batch, as any failed read does, even though its
    /// `objects` row reads fine.
    #[tokio::test]
    async fn test_failed_version_index_read_fails_the_batch() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), OBJECT_AT_CHECKPOINT).await;

        // `broken` has its `objects` row at version 3, but its version index row at checkpoint 5
        // cannot be decoded. `healthy` is live at version 1 from checkpoint 1.
        let broken = ObjectID::random();
        let version = SequenceNumber::from_u64(3);
        mock.insert_row(
            objects::NAME,
            objects::encode_key(&ObjectKey(broken, version)),
            objects::encode(&Object::with_id_owner_version_for_testing(
                broken,
                version,
                Owner::AddressOwner(SuiAddress::ZERO),
            ))
            .unwrap()
            .to_vec(),
        )
        .await;
        mock.insert_row(
            object_versions_by_checkpoint::NAME,
            object_versions_by_checkpoint::encode_key(&broken, 5),
            [(
                object_versions_by_checkpoint::col::VERSION,
                vec![1u8, 2].into(),
            )],
        )
        .await;
        let healthy = ObjectID::random();
        insert_history(&mock, healthy, &[(1, 1, ObjectStatus::Live)]).await;

        let req = selector_request(broken, Some(3), None, Some(7));
        let outcome =
            single_outcome(get_object(&kv_rpc, kv_rpc.admitted_request(req.clone())).await);
        assert!(
            matches!(outcome, Err(code) if code != tonic::Code::NotFound),
            "{outcome:?}"
        );

        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = vec![selector_request(healthy, None, None, None), req];
        let result = batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch)).await;
        assert!(
            result.is_err(),
            "{:?}",
            result.ok().map(|response| response.objects)
        );

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
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), OBJECT_AT_CHECKPOINT).await;

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
    /// `OBJECT_REMOVED` detail describes, if it has one, as `(removedVersion, removalKind)`.
    type RemovalOutcome = Result<u64, Option<(u64, String)>>;

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
        )))
    }

    /// A read that resolves to a deletion or wrap answers `NOT_FOUND` with an `OBJECT_REMOVED`
    /// detail naming the removal's version and kind, from tombstones that record no removing
    /// transaction; a read of a never-existed object, or one bounded by `at_checkpoint` whose
    /// resolved version was written after the checkpoint, answers plain `NOT_FOUND`. Latest,
    /// exact-version, `max_version` and `at_checkpoint` reads, alone and combined, single and in
    /// one batch with live and never-existed items, with and without object contents.
    #[tokio::test]
    async fn test_removed_objects_report_their_removal() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), OBJECT_AT_CHECKPOINT).await;

        // Created at version 2 in checkpoint 1, mutated at 4 in checkpoint 3, wrapped at 6 in
        // checkpoint 5, unwrapped at 8 in checkpoint 7, and deleted at 9 in checkpoint 10.
        let id = ObjectID::random();
        let never_existed = ObjectID::random();
        let history = [
            (1, 2, ObjectStatus::Live),
            (3, 4, ObjectStatus::Live),
            (5, 6, ObjectStatus::Wrapped),
            (7, 8, ObjectStatus::Live),
            (10, 9, ObjectStatus::Deleted),
        ];
        insert_history(&mock, id, &history).await;

        // The outcome of reading `entry` of the history: its version if live, else its removal.
        let outcome = |entry: Option<&(u64, u64, ObjectStatus)>| {
            let Some(&(_, version, status)) = entry else {
                return Err(None);
            };
            let kind = match status {
                ObjectStatus::Live => return Ok(version),
                ObjectStatus::Deleted => "DELETED",
                ObjectStatus::Wrapped => "WRAPPED",
            };
            Err(Some((version, kind.to_owned())))
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
        // Both kinds are covered.
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

        // A latest or exact-version read lands on the tombstone row, so a removal costs it no
        // extra read. An `at_checkpoint` read learns the removal from the version index and reads
        // only the tombstone row, which must agree with the index.
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
            let kv_rpc = start_kv_rpc_at_heights(
                addr,
                StagesConfig::default(),
                OBJECT_AT_CHECKPOINT,
                |_| served,
            )
            .await;
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
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), OBJECT_AT_CHECKPOINT).await;

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
        for alpha_features in [AlphaFeatures::default(), OBJECT_AT_CHECKPOINT] {
            let enabled = alpha_features.object_at_checkpoint;
            let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), alpha_features).await;
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
            for (alpha_features, served) in
                [(AlphaFeatures::default(), 10), (OBJECT_AT_CHECKPOINT, 7)]
            {
                let enabled = alpha_features.object_at_checkpoint;
                let mock = MockBigtableServer::new();
                let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
                let kv_rpc = start_kv_rpc_at_heights(
                    addr,
                    StagesConfig::default(),
                    alpha_features,
                    |pipeline| {
                        if pipeline == lagging { 7 } else { 10 }
                    },
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
        let kv_rpc = start_kv_rpc(addr, stages, AlphaFeatures::default()).await;

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

    /// A lookup's `objects` row is read as soon as its own scan ends, without waiting for the
    /// rest of its scan group: when one group's `at_checkpoint` scans end one at a time, their
    /// rows go out in more than one `objects` read, each row once, and every request is answered.
    #[tokio::test]
    async fn test_batch_scan_answers_reach_objects_reads_as_each_scan_ends() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        // One group of four scans.
        let stages = StagesConfig {
            objects: Some(StageConfig {
                max_seeks_per_read: Some(4),
                ..Default::default()
            }),
            ..Default::default()
        };
        // One permit per request, and 25ms per row read, so the scans end 25ms apart.
        let kv_rpc =
            start_kv_rpc_with_request_concurrency(addr, stages, OBJECT_AT_CHECKPOINT, 1).await;
        mock.set_read_rows_latency(Duration::from_millis(25)).await;

        // Four objects, each live at version 1 from checkpoint 1.
        let stored: Vec<Object> = (0..4)
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
        batch.read_mask = Some(mask(&["object_id", "version", "digest"]));

        mock.clear_read_rows_calls().await;
        let response = tokio::time::timeout(
            Duration::from_secs(10),
            batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch)),
        )
        .await
        .expect("the batch stalled")
        .unwrap();
        assert_eq!(response.objects.len(), stored.len());
        for (item, object) in response.objects.iter().zip_debug_eq(&stored) {
            assert_eq!(
                item.object().object_id,
                Some(object.id().to_canonical_string(true)),
                "{item:?}"
            );
            assert_eq!(
                item.object().digest,
                Some(object.digest().to_string()),
                "{item:?}"
            );
        }

        let reads: Vec<_> = mock
            .read_rows_calls()
            .await
            .into_iter()
            .filter(|call| call.table == objects::NAME)
            .map(|call| call.row_keys)
            .collect();
        assert!(
            reads.len() > 1,
            "the group's rows waited for its slowest scan: {reads:?}"
        );
        let mut read_keys: Vec<_> = reads.into_iter().flatten().collect();
        read_keys.sort();
        let mut stored_keys: Vec<_> = stored
            .iter()
            .map(|object| objects::encode_key(&ObjectKey(object.id(), object.version())))
            .collect();
        stored_keys.sort();
        assert_eq!(read_keys, stored_keys, "each row is read exactly once");

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
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), OBJECT_AT_CHECKPOINT).await;

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

    /// A batch completes on a single permit with more scan groups than the `objects` stage runs
    /// at once, its lookups split between scans and plain version reads: a scan that holds the
    /// permit runs to its end and returns it, whether or not the `objects` reads it feeds are
    /// taking lookups. Every request gets its own lookup's answer, found or `NOT_FOUND`.
    #[tokio::test]
    async fn test_batch_scans_do_not_starve_object_reads_of_permits() {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        // One permit per request, scan groups of two, and one group and one `objects` read of
        // two keys at a time.
        let stages = StagesConfig {
            objects: Some(StageConfig {
                max_seeks_per_read: Some(2),
                concurrency: Some(1),
                ..Default::default()
            }),
            ..Default::default()
        };
        let kv_rpc =
            start_kv_rpc_with_request_concurrency(addr, stages, OBJECT_AT_CHECKPOINT, 1).await;

        // Eight objects. The even ones are live at version 1 from checkpoint 1 and at version 2
        // from checkpoint 7; the odd ones only at version 1, from checkpoint 7. One object's
        // lookups have different answers, so an answer that reaches another lookup's request
        // fails the test.
        let ids: Vec<ObjectID> = (0..8).map(|_| ObjectID::random()).collect();
        for (index, &id) in ids.iter().enumerate() {
            let history: &[(u64, u64, ObjectStatus)] = if index % 2 == 0 {
                &[(1, 1, ObjectStatus::Live), (7, 2, ObjectStatus::Live)]
            } else {
                &[(7, 1, ObjectStatus::Live)]
            };
            insert_history(&mock, id, history).await;
        }

        // Each object at version 1, latest, at checkpoint 5, and at the missing version 3: 16
        // scans in eight groups, and 16 plain version reads.
        let cases: Vec<(ObjectID, GetObjectRequest, Result<u64, tonic::Code>)> = ids
            .iter()
            .enumerate()
            .flat_map(|(index, &id)| {
                let (latest, at_checkpoint_5) = if index % 2 == 0 {
                    (Ok(2), Ok(1))
                } else {
                    (Ok(1), Err(tonic::Code::NotFound))
                };
                [
                    (selector_request(id, Some(1), None, None), Ok(1)),
                    (selector_request(id, None, None, None), latest),
                    (selector_request(id, None, None, Some(5)), at_checkpoint_5),
                    (
                        selector_request(id, Some(3), None, None),
                        Err(tonic::Code::NotFound),
                    ),
                ]
                .map(|(req, expected)| (id, req, expected))
            })
            .collect();
        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = cases.iter().map(|(_, req, _)| req.clone()).collect();
        batch.read_mask = Some(mask(&["object_id", "version", "digest"]));

        let response = tokio::time::timeout(
            Duration::from_secs(10),
            batch_get_objects(&kv_rpc, kv_rpc.admitted_request(batch)),
        )
        .await
        .expect("the batch stalled on its own limiter")
        .unwrap();
        assert_eq!(response.objects.len(), cases.len());
        for (item, (id, req, expected)) in response.objects.iter().zip_debug_eq(&cases) {
            assert_eq!(batch_outcome(item), *expected, "{req:?}: {item:?}");
            if expected.is_ok() {
                assert_eq!(
                    item.object().object_id,
                    Some(id.to_canonical_string(true)),
                    "{req:?}: {item:?}"
                );
            }
        }

        mock_server.abort();
    }

    #[tokio::test]
    async fn test_batch_get_objects_limit_exceeded() {
        let mock = MockBigtableServer::new();
        let (addr, server) = mock.start().await.expect("start mock BigTable");
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), AlphaFeatures::default()).await;

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
