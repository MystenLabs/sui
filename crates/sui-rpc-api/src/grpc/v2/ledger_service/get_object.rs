// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::list_object_versions::removal_kind;
use super::list_object_versions::stored_version;
use super::list_object_versions::stored_version_of_indexed;
use crate::ErrorReason;
use crate::ObjectRemoval;
use crate::RpcError;
use crate::RpcService;
use crate::error::ObjectNotFoundError;
use crate::grpc::v2::render::RenderContext;
use crate::ledger_history::query_options::below_serving_floor;
use crate::reader::AtCheckpointUpperBound;
use crate::reader::StateReader;
use mysten_common::ZipDebugEqIteratorExt;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsResponse;
use sui_rpc::proto::sui::rpc::v2::GetObjectRequest;
use sui_rpc::proto::sui::rpc::v2::GetObjectResponse;
use sui_rpc::proto::sui::rpc::v2::GetObjectResult;
use sui_rpc::proto::sui::rpc::v2::Object;
use sui_sdk_types::Address;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SequenceNumber;
use sui_types::object::Object as StoredObject;
use sui_types::storage::ObjectOrTombstone;
use sui_types::storage::RpcStateReader;

pub const MAX_BATCH_REQUESTS: usize = 1000;
pub const READ_MASK_DEFAULT: &str = crate::read_mask_defaults::OBJECT;

/// Which state of an object a `GetObjectRequest` reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjectSelector {
    /// The latest live version (none of `version`, `max_version` and `at_checkpoint` set).
    Latest,
    /// Exactly `version`. With `at_checkpoint`, only if that version was written at or before
    /// the checkpoint. A fullnode whose object lower bound is above 0 (it pruned object versions
    /// or was restored from a snapshot) answers a version it does not hold with `OUT_OF_RANGE` /
    /// `BELOW_EARLIEST_AVAILABLE` rather than `NOT_FOUND`, unless it holds the tombstone of a
    /// deletion or wrap at that version, the version is above the object's latest version, or
    /// `at_checkpoint` decides the answer without it (see `resolve_missing_versions` in this
    /// module). With `at_checkpoint`, it answers the same way when the version index records no
    /// version of the object as of the checkpoint, since pruning removes the index history of an
    /// object deleted or wrapped in a pruned checkpoint.
    Version {
        version: u64,
        at_checkpoint: Option<u64>,
    },
    /// The newest version at or below `max_version`, unless the object was deleted or wrapped at
    /// that version. With `at_checkpoint`, versions count up to the height the server answers
    /// `at_checkpoint` reads at, and the read fails if the selected version was written after the
    /// checkpoint, so it agrees with a `version` read of the selected version at that checkpoint.
    /// Without it, a fullnode reads its live state, counting every version it holds, including
    /// those not yet committed from its execution cache to its store, and kv-rpc reads at its
    /// served height, answering as if `at_checkpoint` were set to it, since versions above it may
    /// be in its tables before their deletions and wraps are. A fullnode whose object lower bound
    /// is above 0 answers a read that finds no live version to select with `OUT_OF_RANGE` /
    /// `BELOW_EARLIEST_AVAILABLE` rather than `NOT_FOUND`, since the versions it would select
    /// from may have been pruned, unless the read resolves to a deletion or wrap that is the
    /// object's latest version, its version at the end of `at_checkpoint`, or one whose
    /// transaction's checkpoint is at or above that bound. A read with `at_checkpoint` is refused
    /// the same way when the version index records no version of the object as of the
    /// checkpoint. A fullnode serves it only with rpc indexes whose history is not still
    /// backfilling after a restore.
    MaxVersion {
        max_version: u64,
        at_checkpoint: Option<u64>,
    },
    /// The object's state at the end of this checkpoint (`at_checkpoint` alone).
    AtCheckpoint(u64),
}

impl ObjectSelector {
    /// The request's `at_checkpoint`: the checkpoint an [`Self::AtCheckpoint`] read is pinned
    /// to, or the checkpoint that bounds a [`Self::Version`] or [`Self::MaxVersion`] read to
    /// versions written at or before it. Either way, the read fails if the checkpoint is outside
    /// the range the server can answer for.
    pub fn at_checkpoint(&self) -> Option<u64> {
        match *self {
            Self::Latest => None,
            Self::Version { at_checkpoint, .. } | Self::MaxVersion { at_checkpoint, .. } => {
                at_checkpoint
            }
            Self::AtCheckpoint(checkpoint) => Some(checkpoint),
        }
    }

    /// Whether the read picks its version by a bound rather than by name: every read with
    /// `at_checkpoint`, and every `max_version` read. These reads need the rpc indexes' history on
    /// a fullnode, and kv-rpc's object-at-checkpoint reads.
    pub fn selects_version_by_bound(&self) -> bool {
        matches!(self, Self::MaxVersion { .. }) || self.at_checkpoint().is_some()
    }

    /// The `NotFound` error for `object_id` when this selector finds no object.
    pub fn not_found(&self, object_id: Address) -> ObjectNotFoundError {
        let error = match *self {
            Self::Latest | Self::AtCheckpoint(_) => ObjectNotFoundError::new(object_id),
            Self::Version { version, .. } => {
                ObjectNotFoundError::new_with_version(object_id, version)
            }
            Self::MaxVersion { max_version, .. } => {
                ObjectNotFoundError::new_with_max_version(object_id, max_version)
            }
        };
        match self.at_checkpoint() {
            Some(checkpoint) => error.at_checkpoint(checkpoint),
            None => error,
        }
    }
}

type ValidationResult = Result<(Vec<(Address, ObjectSelector)>, FieldMaskTree), RpcError>;

/// Validate each request's `object_id` and selector fields, and the read mask shared by all of
/// them; each request's own `read_mask` is ignored. A request that sets both `version` and
/// `max_version` is rejected. `at_checkpoint` combines with either. `batch` says whether
/// `requests` are the items of a `BatchGetObjects` request: the `max_version` violation then
/// names the item (`requests[i].max_version`), and for a `GetObject` request the field alone.
pub fn validate_get_object_requests(
    requests: &[GetObjectRequest],
    read_mask: Option<FieldMask>,
    batch: bool,
) -> ValidationResult {
    let read_mask = {
        let read_mask = read_mask.unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
        read_mask.validate::<Object>().map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        FieldMaskTree::from(read_mask)
    };
    let requests = requests
        .iter()
        .enumerate()
        .map(|(idx, request)| {
            let object_id = request
                .object_id
                .as_ref()
                .ok_or_else(|| {
                    FieldViolation::new("object_id")
                        .with_reason(ErrorReason::FieldMissing)
                        .nested_at("requests", idx)
                })?
                .parse()
                .map_err(|e| {
                    FieldViolation::new("object_id")
                        .with_description(format!("invalid object_id: {e}"))
                        .with_reason(ErrorReason::FieldInvalid)
                        .nested_at("requests", idx)
                })?;
            let selector = match (request.version, request.max_version, request.at_checkpoint) {
                (Some(_), Some(_), _) => {
                    let violation = FieldViolation::new("max_version")
                        .with_description("version and max_version are mutually exclusive")
                        .with_reason(ErrorReason::FieldInvalid);
                    return Err(if batch {
                        violation.nested_at("requests", idx)
                    } else {
                        violation
                    }
                    .into());
                }
                (None, None, None) => ObjectSelector::Latest,
                (None, None, Some(checkpoint)) => ObjectSelector::AtCheckpoint(checkpoint),
                (Some(version), None, at_checkpoint) => ObjectSelector::Version {
                    version,
                    at_checkpoint,
                },
                (None, Some(max_version), at_checkpoint) => ObjectSelector::MaxVersion {
                    max_version,
                    at_checkpoint,
                },
            };
            Ok((object_id, selector))
        })
        .collect::<Result<_, RpcError>>()?;
    Ok((requests, read_mask))
}

#[tracing::instrument(skip(service))]
pub fn get_object(
    service: &RpcService,
    mut request: GetObjectRequest,
) -> Result<GetObjectResponse, RpcError> {
    let read_mask = request.read_mask.take();
    let (requests, read_mask) =
        validate_get_object_requests(std::slice::from_ref(&request), read_mask, false)?;
    let upper_bound = load_at_checkpoint_upper_bound(service, &requests)?;
    let (object_id, selector) = requests[0];
    let render_context = RenderContext::empty(service.reader.inner());
    let read = get_object_impl(
        service,
        object_id,
        selector,
        upper_bound.as_ref(),
        &read_mask,
        &render_context,
    );
    let mut results = resolve_missing_versions(service, &requests, vec![read])?;
    apply_at_checkpoint_lower_bound(service, &requests, upper_bound.as_ref(), &mut results)?;
    // One request, so one result.
    results.swap_remove(0).map(GetObjectResponse::new)
}

#[tracing::instrument(skip(service))]
pub fn batch_get_objects(
    service: &RpcService,
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

    let (requests, read_mask) = validate_get_object_requests(&requests, read_mask, true)?;

    let upper_bound = load_at_checkpoint_upper_bound(service, &requests)?;
    let render_context = RenderContext::empty(service.reader.inner());
    let reads: Vec<_> = requests
        .iter()
        .map(|&(object_id, selector)| {
            get_object_impl(
                service,
                object_id,
                selector,
                upper_bound.as_ref(),
                &read_mask,
                &render_context,
            )
        })
        .collect();
    let mut results = resolve_missing_versions(service, &requests, reads)?;
    apply_at_checkpoint_lower_bound(service, &requests, upper_bound.as_ref(), &mut results)?;
    let objects = results
        .into_iter()
        .map(|result| match result {
            Ok(object) => GetObjectResult::new_object(object),
            Err(error) => GetObjectResult::new_error(error.into_status_proto()),
        })
        .collect();
    Ok(BatchGetObjectsResponse::new(objects))
}

/// Before the reads, once per request, refuse the whole request if this node cannot serve one
/// of its items, and load what the reads check:
///
/// - If some item sets `at_checkpoint`, the upper bound the reads at a checkpoint check
///   (`StateReader::at_checkpoint_upper_bound`), which needs the rpc indexes' history.
/// - Otherwise, a `max_version` item needs the rpc indexes' history
///   (`StateReader::require_rpc_index_history`): on a pruned node its read checks the version
///   it finds against them.
///
/// Other reads still work on nodes without rpc indexes.
fn load_at_checkpoint_upper_bound(
    service: &RpcService,
    requests: &[(Address, ObjectSelector)],
) -> Result<Option<AtCheckpointUpperBound>, RpcError> {
    let any = |predicate: fn(&ObjectSelector) -> bool| {
        requests.iter().any(|(_, selector)| predicate(selector))
    };
    if any(|selector| selector.at_checkpoint().is_some()) {
        return service.reader.at_checkpoint_upper_bound().map(Some);
    }
    if any(|selector| matches!(selector, ObjectSelector::MaxVersion { .. })) {
        service.reader.require_rpc_index_history()?;
    }
    Ok(None)
}

/// After the reads: answer every item whose read found a version missing (`Ok(None)` from
/// [`get_object_impl`]), with this node's object lower bound
/// ([`sui_types::storage::RpcStateReader::get_lowest_available_checkpoint_objects`]) and its
/// objects' latest versions. See [`missing_version_error`].
///
/// The lower bound is read at most once per request, after the reads: pruning can raise it while
/// the reads run, and a version pruned meanwhile then fails as pruned rather than as one that
/// never existed.
fn resolve_missing_versions(
    service: &RpcService,
    requests: &[(Address, ObjectSelector)],
    reads: Vec<Result<Option<Object>, RpcError>>,
) -> Result<Vec<Result<Object, RpcError>>, RpcError> {
    let reader = service.reader.inner();
    answer_missing_versions(
        requests,
        reads,
        || Ok(reader.get_lowest_available_checkpoint_objects()?),
        |object_id| latest_version(reader.as_ref(), ObjectID::from(object_id)),
    )
}

/// The object's latest version, live or a deletion or wrap, read through the execution cache, so
/// it sees changes not committed to the store yet. The live object is read first: that read
/// caches the absence of an object with no version, so the newest-entry read after it, and both
/// reads of a later request for the same object, find it in the cache. A store that keeps no
/// tombstones reads none, and gives the latest live version.
fn latest_version(reader: &dyn RpcStateReader, id: ObjectID) -> Option<LatestVersion> {
    if let Some(object) = reader.get_object(&id) {
        return Some(LatestVersion {
            version: object.version(),
            removed: false,
        });
    }
    let item = reader.get_latest_object_or_tombstone(&id).ok()??;
    Some(LatestVersion {
        version: stored_version(&item),
        removed: matches!(item, ObjectOrTombstone::Tombstone(_)),
    })
}

/// An object's newest version in the object store, live or a deletion or wrap.
#[derive(Clone, Copy, Debug)]
struct LatestVersion {
    version: SequenceNumber,
    /// Whether the object was deleted or wrapped at `version`.
    removed: bool,
}

/// [`resolve_missing_versions`] with the object lower bound and the objects' latest versions read
/// through `objects_lower_bound` and `latest_version`. `objects_lower_bound` is called only if
/// some read found a version missing.
fn answer_missing_versions(
    requests: &[(Address, ObjectSelector)],
    reads: Vec<Result<Option<Object>, RpcError>>,
    objects_lower_bound: impl FnOnce() -> Result<u64, RpcError>,
    latest_version: impl Fn(Address) -> Option<LatestVersion>,
) -> Result<Vec<Result<Object, RpcError>>, RpcError> {
    let objects_lower_bound = if reads.iter().any(|read| matches!(read, Ok(None))) {
        objects_lower_bound()?
    } else {
        0
    };
    Ok(requests
        .iter()
        .zip_debug_eq(reads)
        .map(|(&(object_id, selector), read)| match read? {
            Some(object) => Ok(object),
            None => Err(missing_version_error(
                object_id,
                selector,
                objects_lower_bound,
                || latest_version(object_id),
            )),
        })
        .collect())
}

/// The answer to a read that found the version it needs missing from the object store, and no
/// deletion or wrap there that it could describe (see [`get_object_impl`]).
///
/// - `NOT_FOUND` if `objects_lower_bound` is 0: the node was neither pruned nor restored from a
///   snapshot, so it holds every version ever written, and the missing one never existed.
/// - `NOT_FOUND` for a `max_version` read without `at_checkpoint`, if the object's latest version
///   (`latest_version`) is a deletion or wrap at or below `max_version`: that removal is the
///   newest version at or below `max_version`. The read gets here, without the removal described,
///   only when the store's tombstone lookup refused that tombstone as possibly below the object
///   lower bound; a failed lookup fails the read instead (see [`read_object`]). The latest version
///   is read through the execution cache, so it includes versions not committed to the store yet.
/// - Otherwise `OUT_OF_RANGE` / `BELOW_EARLIEST_AVAILABLE` with `objects_lower_bound` as the
///   lowest available checkpoint, since the version may have been pruned. That includes a
///   `max_version` read that finds no live version at or below `max_version`, an object with no
///   version left, and a `version` or `max_version` read with `at_checkpoint` of an object the
///   version index records no version of as of the checkpoint (see [`VersionRead::NoVersionYet`]).
///   The object store's live reads do not tell a deletion or wrap from a pruned version, so a
///   read that resolves to a removal this node keeps no tombstone of, or one the store's
///   tombstone lookup refuses as possibly below the object lower bound, fails the same way. (An
///   exact `version` above the object's latest version is `NOT_FOUND` before it gets here; see
///   [`exact_version_after_miss`].)
fn missing_version_error(
    object_id: Address,
    selector: ObjectSelector,
    objects_lower_bound: u64,
    latest_version: impl FnOnce() -> Option<LatestVersion>,
) -> RpcError {
    let answered_by_latest_version = || match selector {
        ObjectSelector::MaxVersion {
            max_version,
            at_checkpoint: None,
        } => latest_version().is_some_and(|latest| {
            latest.removed && latest.version <= SequenceNumber::from_u64(max_version)
        }),
        _ => false,
    };
    if objects_lower_bound == 0 || answered_by_latest_version() {
        selector.not_found(object_id).into()
    } else {
        below_serving_floor(objects_lower_bound)
    }
}

/// After the reads: answer as pruned every item whose `at_checkpoint` is below the lower bound
/// (see `StateReader::at_checkpoint_lower_bound`), replacing whatever its read returned, since
/// that read may have seen pruned data.
fn apply_at_checkpoint_lower_bound(
    service: &RpcService,
    requests: &[(Address, ObjectSelector)],
    upper_bound: Option<&AtCheckpointUpperBound>,
    results: &mut [Result<Object, RpcError>],
) -> Result<(), RpcError> {
    // The request loads an upper bound exactly when some item sets `at_checkpoint`.
    if upper_bound.is_none() {
        return Ok(());
    }
    let lower_bound = service.reader.at_checkpoint_lower_bound()?;
    for (&(_, selector), result) in requests.iter().zip_debug_eq(results.iter_mut()) {
        if let Some(checkpoint) = selector.at_checkpoint()
            && let Err(error) = lower_bound.check(checkpoint)
        {
            *result = Err(error);
        }
    }
    Ok(())
}

/// What a read found for the version its selector picks.
#[derive(Debug)]
enum VersionRead<T> {
    /// The selected version: the object, live, or read with tombstones (see [`select`]), the
    /// tombstone of the deletion or wrap at that version.
    Selected(T),
    /// The answer is that the object does not exist (`NOT_FOUND`): the selected version was
    /// written after the request's `at_checkpoint`, or deleted or wrapped the object.
    NotFound,
    /// The version index records no version of the object as of the request's `at_checkpoint`:
    /// the object did not exist yet, or it was deleted or wrapped in a checkpoint this node
    /// pruned, along with the object's index history. Either way the object does not exist at
    /// the checkpoint, so `at_checkpoint` alone is `NOT_FOUND`, but a `version` or `max_version`
    /// read may select a version that existed and was pruned, so it is missing data (see
    /// [`read_object`]). There is no deletion or wrap to describe from the index.
    NoVersionYet,
    /// The object store has no live version where the read needs one. The version may never
    /// have existed, may be a deletion or wrap, or may have been pruned; see
    /// [`missing_version_error`].
    Missing,
}

impl<T> VersionRead<T> {
    /// The item, or [`Self::NotFound`] if there is none.
    fn or_not_found(item: Option<T>) -> Self {
        item.map_or(Self::NotFound, Self::Selected)
    }

    /// The item, or [`Self::Missing`] if there is none.
    fn or_missing(item: Option<T>) -> Self {
        item.map_or(Self::Missing, Self::Selected)
    }
}

/// What a read of one object answers.
#[derive(Debug)]
enum ObjectRead {
    /// The live object the selector picks.
    Live(StoredObject),
    /// `NOT_FOUND`, describing the deletion or wrap the read resolved to when it found it. A read
    /// that found no live version answers with the removal its selection over tombstones picked;
    /// on a pruned node, the store's tombstone lookup returns no tombstone that may have a pruned
    /// version after it (see
    /// [`sui_types::storage::RpcStateReader::get_object_or_tombstone_lt_or_eq_version`]).
    NotFound(Option<ObjectRemoval>),
    /// The object store has no live version where the read needs one, and the read found no
    /// deletion or wrap there to answer with, or a `version` or `max_version` read with
    /// `at_checkpoint` found no version index record ([`VersionRead::NoVersionYet`]); see
    /// [`missing_version_error`].
    Missing,
}

/// Read one object. `upper_bound` is set whenever some item of the request sets `at_checkpoint`;
/// the reader checks it before any read at the checkpoint, so a
/// checkpoint above it fails the read whatever the object's state. A read that resolves to a
/// deletion or wrap answers `NotFound`, describing the removal if [`read_object`] finds it.
/// `Ok(None)` if a `version` or `max_version` read found the version it needs missing, and no
/// removal there that it can answer with (see [`ObjectRead::Missing`]);
/// [`resolve_missing_versions`] answers it after the reads.
#[tracing::instrument(skip(service, render_context))]
fn get_object_impl(
    service: &RpcService,
    object_id: Address,
    selector: ObjectSelector,
    upper_bound: Option<&AtCheckpointUpperBound>,
    read_mask: &FieldMaskTree,
    render_context: &RenderContext,
) -> Result<Option<Object>, RpcError> {
    let reader = &service.reader;
    let id = ObjectID::from(object_id);
    // A node without object version history serves only each object's latest version, and
    // decides from its newest entry, live or a tombstone: the compactor keeps that entry, so the
    // answer does not change as compaction proceeds. A version above that entry has not been
    // written yet and is `NotFound`, as on any node; an object with no entry at all is `NotFound`
    // too. A deletion or wrap at exactly that version is `NotFound` with the removal described.
    // Any older version is refused (`OutOfRange`, `VERSION_NOT_RETAINED`) even while it is still
    // stored.
    if let ObjectSelector::Version {
        version,
        at_checkpoint: None,
    } = selector
        && let Err(refusal) = reader.require_object_version_history()
    {
        // The live object first: a store that keeps no tombstones reads no newest entry.
        let newest = match reader.inner().get_object(&id) {
            Some(object) => Some(ObjectOrTombstone::Object(object)),
            None => ObjectsOrTombstones(reader).latest(id)?,
        };
        return match newest {
            Some(ObjectOrTombstone::Object(object)) if object.version().value() == version => Ok(
                Some(service.render_object_to_proto(&object, read_mask, render_context)),
            ),
            Some(item) if item.stored_version().value() < version => {
                Err(selector.not_found(object_id).into())
            }
            Some(ObjectOrTombstone::Tombstone(tombstone @ (_, removed_at, _)))
                if removed_at.value() == version =>
            {
                let removal = tombstone_removal(tombstone);
                Err(not_found_describing(selector.not_found(object_id), removal).into())
            }
            Some(_) => Err(refusal),
            None => Err(selector.not_found(object_id).into()),
        };
    }
    let read = || {
        read_object(
            &LiveObjects(reader),
            &ObjectsOrTombstones(reader),
            id,
            selector,
            upper_bound,
        )
    };
    let read = match selector {
        ObjectSelector::Version {
            version,
            at_checkpoint: None,
        } => match LiveObjects(reader).by_key(id, version.into())? {
            Some(object) => ObjectRead::Live(object),
            None => exact_version_after_miss(
                version.into(),
                || latest_version(reader.inner().as_ref(), id),
                read,
            )?,
        },
        _ => read()?,
    };
    match read {
        ObjectRead::Live(object) => Ok(Some(service.render_object_to_proto(
            &object,
            read_mask,
            render_context,
        ))),
        ObjectRead::NotFound(removal) => {
            Err(not_found_describing(selector.not_found(object_id), removal).into())
        }
        ObjectRead::Missing => Ok(None),
    }
}

/// An exact `version` read without `at_checkpoint` whose read of the live object found the
/// version missing. A version above the object's latest version (`latest_version`, live or a
/// deletion or wrap) has not been written yet, and is `NOT_FOUND` on any node. The latest version
/// is read only after the miss, so a version at or below it may have been written between the
/// two reads: `read_again` reads it again, live and then with tombstones to describe a deletion
/// or wrap at it, and it is [`ObjectRead::Missing`] only if that read finds neither. A version
/// that misses is looked for with tombstones once, after the latest version.
fn exact_version_after_miss(
    version: SequenceNumber,
    latest_version: impl FnOnce() -> Option<LatestVersion>,
    read_again: impl FnOnce() -> Result<ObjectRead, RpcError>,
) -> Result<ObjectRead, RpcError> {
    match latest_version() {
        Some(latest) if version > latest.version => Ok(ObjectRead::NotFound(None)),
        Some(_) => read_again(),
        None => Ok(ObjectRead::Missing),
    }
}

/// `not_found`, describing `removal` if there is one.
fn not_found_describing(
    not_found: ObjectNotFoundError,
    removal: Option<ObjectRemoval>,
) -> ObjectNotFoundError {
    match removal {
        Some(removal) => not_found.removed(removal),
        None => not_found,
    }
}

/// Read object `id` by `selector`: select through `live`, and if that finds no live object,
/// look for the deletion or wrap the read resolved to with [`find_removal`] over
/// `with_tombstones`, or, where it found the version it needs missing, answer with
/// [`resolve_missing`].
///
/// The selection over live objects answers the read unless it finds the version missing;
/// otherwise the one with tombstones only describes a removal. It is skipped where the version
/// index records no version of the object as of the read's `at_checkpoint`
/// ([`VersionRead::NoVersionYet`]), so an object that never existed costs that one index read:
/// `NOT_FOUND` for `at_checkpoint` alone, and missing data ([`ObjectRead::Missing`]) for a
/// `version` or `max_version` read. A `max_version` read without `at_checkpoint` selects once,
/// with tombstones: the store's live and tombstone lookups at or below a version select the same
/// entry, so the one with tombstones answers the read whether that entry is live or a removal.
fn read_object(
    live: &impl ObjectReads<Item = StoredObject>,
    with_tombstones: &impl ObjectReads<Item = ObjectOrTombstone>,
    id: ObjectID,
    selector: ObjectSelector,
    upper_bound: Option<&AtCheckpointUpperBound>,
) -> Result<ObjectRead, RpcError> {
    if let ObjectSelector::MaxVersion {
        at_checkpoint: None,
        ..
    } = selector
    {
        return Ok(match select(with_tombstones, id, selector, upper_bound)? {
            VersionRead::Selected(ObjectOrTombstone::Object(object)) => ObjectRead::Live(object),
            VersionRead::Selected(ObjectOrTombstone::Tombstone(tombstone)) => {
                match tombstone_removal(tombstone) {
                    Some(removal) => ObjectRead::NotFound(Some(removal)),
                    None => ObjectRead::Missing,
                }
            }
            // This selector reads `Selected` or `Missing`.
            _ => ObjectRead::Missing,
        });
    }
    let read = select(live, id, selector, upper_bound)?;
    let removal = || find_removal(with_tombstones, id, selector, upper_bound);
    Ok(match read {
        VersionRead::Selected(object) => ObjectRead::Live(object),
        VersionRead::NoVersionYet => match selector {
            ObjectSelector::AtCheckpoint(_) => ObjectRead::NotFound(None),
            _ => ObjectRead::Missing,
        },
        VersionRead::NotFound => ObjectRead::NotFound(removal()),
        VersionRead::Missing => resolve_missing(with_tombstones, id, selector, upper_bound),
    })
}

/// The answer to a read whose selection over live objects found the version it needs missing
/// ([`VersionRead::Missing`]), from the same selection over the object store's tombstones as well
/// as its live objects (`reads`). The live selection cannot tell a deletion or wrap from a pruned
/// version; this one can. A removal it selects answers `NOT_FOUND`, described. So does its
/// [`VersionRead::NotFound`], as for a `max_version` read with `at_checkpoint` K whose newest
/// version at or below `max_version` is a removal written after K. The store's tombstone lookup
/// refuses an entry that may have a pruned version after it. [`ObjectRead::Missing`] if the
/// selection resolves to nothing, to a live object (written after the live read), or a read
/// fails.
fn resolve_missing(
    reads: &impl ObjectReads<Item = ObjectOrTombstone>,
    id: ObjectID,
    selector: ObjectSelector,
    upper_bound: Option<&AtCheckpointUpperBound>,
) -> ObjectRead {
    match select(reads, id, selector, upper_bound) {
        Ok(VersionRead::Selected(ObjectOrTombstone::Tombstone(tombstone))) => {
            match tombstone_removal(tombstone) {
                Some(removal) => ObjectRead::NotFound(Some(removal)),
                None => ObjectRead::Missing,
            }
        }
        Ok(VersionRead::NotFound) => ObjectRead::NotFound(None),
        _ => ObjectRead::Missing,
    }
}

/// The deletion or wrap `selector` resolved to, for a read that found no live object: the same
/// selection over the object store's tombstones as well as its live objects (`reads`). Best
/// effort: `None` if the store keeps no tombstones, the selection finds none, or a read fails,
/// which leaves the `NotFound` answer without detail.
fn find_removal(
    reads: &impl ObjectReads<Item = ObjectOrTombstone>,
    id: ObjectID,
    selector: ObjectSelector,
    upper_bound: Option<&AtCheckpointUpperBound>,
) -> Option<ObjectRemoval> {
    let Ok(VersionRead::Selected(ObjectOrTombstone::Tombstone(tombstone))) =
        select(reads, id, selector, upper_bound)
    else {
        return None;
    };
    tombstone_removal(tombstone)
}

/// The deletion or wrap the tombstone `(_, version, digest)` records. `None` if the digest marks
/// no deletion or wrap. The removing transaction is not looked up: `ListObjectVersions` finds it
/// when the request's `removal_read_mask` names `transaction`.
fn tombstone_removal((_, version, digest): ObjectRef) -> Option<ObjectRemoval> {
    Some(ObjectRemoval {
        version: version.value(),
        kind: removal_kind(&digest).ok()?,
    })
}

/// The version `selector` reads of object `id`, through `reads`: what [`get_object_impl`] renders
/// when `reads` reads live objects, or with [`ObjectsOrTombstones`], the tombstone in its place
/// when that version deleted or wrapped the object. The versions the version index records come
/// through [`ObjectReads::version_at_checkpoint`]. A read with `at_checkpoint` checks
/// `upper_bound` before any read at the checkpoint.
fn select<R: ObjectReads>(
    reads: &R,
    id: ObjectID,
    selector: ObjectSelector,
    upper_bound: Option<&AtCheckpointUpperBound>,
) -> Result<VersionRead<R::Item>, RpcError> {
    let upper_bound = || {
        upper_bound.ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                "at_checkpoint upper bound was not loaded",
            )
        })
    };
    let version_at = |checkpoint, upper_bound: &AtCheckpointUpperBound| {
        reads.version_at_checkpoint(id, checkpoint, upper_bound)
    };
    match selector {
        ObjectSelector::Latest => Ok(VersionRead::or_not_found(reads.latest(id)?)),
        ObjectSelector::Version {
            version,
            at_checkpoint: None,
        } => Ok(VersionRead::or_missing(reads.by_key(id, version.into())?)),
        ObjectSelector::Version {
            version,
            at_checkpoint: Some(checkpoint),
        } => version_at_checkpoint(
            version.into(),
            version_at(checkpoint, upper_bound()?)?,
            |version| reads.by_key(id, version),
        ),
        ObjectSelector::MaxVersion {
            max_version,
            at_checkpoint: None,
        } => Ok(VersionRead::or_missing(
            reads.lt_or_eq(id, max_version.into())?,
        )),
        ObjectSelector::MaxVersion {
            max_version,
            at_checkpoint: Some(checkpoint),
        } => {
            let upper_bound = upper_bound()?;
            max_version_at_checkpoint(
                max_version.into(),
                version_at(checkpoint, upper_bound)?,
                || version_at(upper_bound.highest_indexed(), upper_bound),
                |version| reads.by_key(id, version),
                |version| reads.lt_or_eq(id, version),
            )
        }
        ObjectSelector::AtCheckpoint(checkpoint) => {
            // A deletion or wrapping tombstone has no live object at its version.
            match version_at(checkpoint, upper_bound()?)? {
                Some(version) => Ok(VersionRead::or_not_found(reads.by_key(id, version)?)),
                None => Ok(VersionRead::NoVersionYet),
            }
        }
    }
}

/// The object store reads [`select`] makes.
trait ObjectReads {
    type Item: StoredVersion;

    /// The object's latest version.
    fn latest(&self, id: ObjectID) -> Result<Option<Self::Item>, RpcError>;

    /// Exactly `version` of the object.
    fn by_key(&self, id: ObjectID, version: SequenceNumber)
    -> Result<Option<Self::Item>, RpcError>;

    /// The object's newest version at or below `version`.
    fn lt_or_eq(
        &self,
        id: ObjectID,
        version: SequenceNumber,
    ) -> Result<Option<Self::Item>, RpcError>;

    /// The object's version at the end of `checkpoint`, as the version index records it (see
    /// [`crate::reader::StateReader::get_object_version_at_checkpoint`]), mapped to the version in
    /// the object store of the item it records, as far as these reads tell the two apart.
    fn version_at_checkpoint(
        &self,
        id: ObjectID,
        checkpoint: u64,
        upper_bound: &AtCheckpointUpperBound,
    ) -> Result<Option<SequenceNumber>, RpcError>;
}

/// Reads of live objects: a version at which the object was deleted or wrapped reads as `None`.
struct LiveObjects<'a>(&'a StateReader);

impl ObjectReads for LiveObjects<'_> {
    type Item = StoredObject;

    fn latest(&self, id: ObjectID) -> Result<Option<StoredObject>, RpcError> {
        Ok(self.0.inner().get_object(&id))
    }

    fn by_key(
        &self,
        id: ObjectID,
        version: SequenceNumber,
    ) -> Result<Option<StoredObject>, RpcError> {
        Ok(self.0.inner().get_object_by_key(&id, version))
    }

    fn lt_or_eq(
        &self,
        id: ObjectID,
        version: SequenceNumber,
    ) -> Result<Option<StoredObject>, RpcError> {
        Ok(self.0.inner().get_object_lt_or_eq_version(&id, version)?)
    }

    /// The index records every live version at that version. A removal it records below its
    /// tombstone (see [`newest_version_written_by`]) reads as no live object at either version.
    fn version_at_checkpoint(
        &self,
        id: ObjectID,
        checkpoint: u64,
        upper_bound: &AtCheckpointUpperBound,
    ) -> Result<Option<SequenceNumber>, RpcError> {
        self.0
            .get_object_version_at_checkpoint(id, checkpoint, upper_bound)
    }
}

/// Reads of live objects and tombstones: a version at which the object was deleted or wrapped
/// reads as its tombstone. Only as complete as the store's
/// [`sui_types::storage::RpcStateReader::get_object_or_tombstone_lt_or_eq_version`] and
/// [`sui_types::storage::RpcStateReader::get_object_or_tombstone_by_key`].
struct ObjectsOrTombstones<'a>(&'a StateReader);

impl ObjectReads for ObjectsOrTombstones<'_> {
    type Item = ObjectOrTombstone;

    fn latest(&self, id: ObjectID) -> Result<Option<ObjectOrTombstone>, RpcError> {
        Ok(self.0.inner().get_latest_object_or_tombstone(&id)?)
    }

    fn by_key(
        &self,
        id: ObjectID,
        version: SequenceNumber,
    ) -> Result<Option<ObjectOrTombstone>, RpcError> {
        Ok(self
            .0
            .inner()
            .get_object_or_tombstone_by_key(&id, version)?)
    }

    fn lt_or_eq(
        &self,
        id: ObjectID,
        version: SequenceNumber,
    ) -> Result<Option<ObjectOrTombstone>, RpcError> {
        Ok(self
            .0
            .inner()
            .get_object_or_tombstone_lt_or_eq_version(&id, version)?)
    }

    fn version_at_checkpoint(
        &self,
        id: ObjectID,
        checkpoint: u64,
        upper_bound: &AtCheckpointUpperBound,
    ) -> Result<Option<SequenceNumber>, RpcError> {
        let indexed = self
            .0
            .get_object_version_at_checkpoint(id, checkpoint, upper_bound)?;
        stored_version_of_indexed(self.0, id, indexed)
    }
}

/// A version read from the object store, live or a tombstone.
trait StoredVersion {
    fn stored_version(&self) -> SequenceNumber;
}

impl StoredVersion for StoredObject {
    fn stored_version(&self) -> SequenceNumber {
        self.version()
    }
}

impl StoredVersion for ObjectOrTombstone {
    fn stored_version(&self) -> SequenceNumber {
        stored_version(self)
    }
}

/// Serve exactly `version` with `at_checkpoint`. `version_at_checkpoint` is the object's version
/// at the end of the checkpoint (see
/// [`crate::reader::StateReader::get_object_version_at_checkpoint`]); a version above it was
/// written after the checkpoint. If there is none, [`VersionRead::NoVersionYet`]: the object
/// did not exist yet, or this node pruned its index history.
///
/// The read is refused unless the `at_checkpoint` lower bound accepts the checkpoint, and then
/// the checkpoint's end state is intact. So if `version_at_checkpoint` has no live object, it is
/// a deletion or wrap, or the version the rpc index records for one (see
/// [`newest_version_written_by`]), and the answer is `NOT_FOUND`. A version below it that has no
/// live object may have been pruned.
fn version_at_checkpoint<T>(
    version: SequenceNumber,
    version_at_checkpoint: Option<SequenceNumber>,
    get_object_by_key: impl FnOnce(SequenceNumber) -> Result<Option<T>, RpcError>,
) -> Result<VersionRead<T>, RpcError> {
    Ok(match version_at_checkpoint {
        Some(version_at_checkpoint) if version < version_at_checkpoint => {
            VersionRead::or_missing(get_object_by_key(version)?)
        }
        Some(version_at_checkpoint) if version == version_at_checkpoint => {
            VersionRead::or_not_found(get_object_by_key(version)?)
        }
        Some(_) => VersionRead::NotFound,
        None => VersionRead::NoVersionYet,
    })
}

/// Serve `max_version` with `at_checkpoint`: select the object's newest version at or below
/// `max_version` written up to the highest indexed checkpoint H, and answer
/// [`VersionRead::NotFound`] if it was written after the checkpoint, or if it deleted or wrapped
/// the object, and [`VersionRead::NoVersionYet`] if the version index records no version of the
/// object as of the checkpoint. `version_at_checkpoint` is the object's version at the end of the
/// checkpoint (see
/// [`crate::reader::StateReader::get_object_version_at_checkpoint`]), and `version_at_height`
/// reads its version at the end of H, needed only when `max_version` is at or above
/// `version_at_checkpoint`. A version above the object's version at the end of a checkpoint was
/// written after it. [`VersionRead::Missing`] if the object store has no live version at or below
/// `max_version` where the selection needs one, unless the object is live at
/// `version_at_checkpoint`: the read is refused unless the `at_checkpoint` lower bound accepts the
/// checkpoint, and then this node holds that version, so a scan from `max_version` at or above it
/// that finds no live version stopped at a deletion or wrap above it, written after the
/// checkpoint. Read with tombstones (see [`select`]), a removal written by the checkpoint selects
/// its tombstone.
fn max_version_at_checkpoint<T: StoredVersion>(
    max_version: SequenceNumber,
    version_at_checkpoint: Option<SequenceNumber>,
    version_at_height: impl FnOnce() -> Result<Option<SequenceNumber>, RpcError>,
    get_object_by_key: impl Fn(SequenceNumber) -> Result<Option<T>, RpcError>,
    get_object_lt_or_eq_version: impl FnOnce(SequenceNumber) -> Result<Option<T>, RpcError>,
) -> Result<VersionRead<T>, RpcError> {
    let Some(version_at_checkpoint) = version_at_checkpoint else {
        // The object did not exist yet, or this node pruned its index history.
        return Ok(VersionRead::NoVersionYet);
    };
    if max_version < version_at_checkpoint {
        // Every version at or below `max_version` was written by the checkpoint.
        return Ok(VersionRead::or_missing(get_object_lt_or_eq_version(
            max_version,
        )?));
    }
    let selected = newest_version_written_by(
        max_version,
        version_at_height()?,
        &get_object_by_key,
        get_object_lt_or_eq_version,
    )?;
    // A live version is never between the version index's record of a removal and the removal's
    // tombstone (see `newest_version_written_by`), so comparing it with `version_at_checkpoint`
    // tells whether it was written after the checkpoint even when that is such a record.
    Ok(match selected {
        VersionRead::Selected(item) if item.stored_version() > version_at_checkpoint => {
            VersionRead::NotFound
        }
        VersionRead::Missing if get_object_by_key(version_at_checkpoint)?.is_some() => {
            VersionRead::NotFound
        }
        selected => selected,
    })
}

/// The newest version of an object at or below `max_version` among those written at or before a
/// checkpoint, [`VersionRead::NoVersionYet`] if the version index records no version of it, or
/// [`VersionRead::NotFound`] if that version deleted or wrapped it. `version_at_checkpoint` is
/// the object's version at the end of the checkpoint; every version above it was written after
/// the checkpoint.
///
/// Below `version_at_checkpoint`, `max_version` selects the newest version at or below it, and
/// the read is [`VersionRead::Missing`] if the object store has no live one. At or above it, the
/// object's state at `version_at_checkpoint` answers: the object if it is live there. If it is
/// not, the object was deleted or wrapped by the checkpoint, but the version index can record
/// that removal below the version of its tombstone in the object store: for a transaction with V1
/// effects, the index records `TransactionEffectsAPI::lamport_version`, which V1 effects recompute
/// from the modified objects alone, while the tombstone sits at the version the transaction
/// executed at, which also counts its read-only inputs. No version lies between the two, so the
/// newest version at or below `max_version` answers if it is below `version_at_checkpoint` (the
/// removal came after `max_version`), and nothing does if the scan finds a version above it,
/// which was written after the checkpoint. A scan of live objects that finds no live version
/// either stopped at the tombstone or found the version before the removal pruned, so the read is
/// [`VersionRead::Missing`].
fn newest_version_written_by<T: StoredVersion>(
    max_version: SequenceNumber,
    version_at_checkpoint: Option<SequenceNumber>,
    get_object_by_key: impl FnOnce(SequenceNumber) -> Result<Option<T>, RpcError>,
    get_object_lt_or_eq_version: impl FnOnce(SequenceNumber) -> Result<Option<T>, RpcError>,
) -> Result<VersionRead<T>, RpcError> {
    let Some(version_at_checkpoint) = version_at_checkpoint else {
        return Ok(VersionRead::NoVersionYet);
    };
    if max_version < version_at_checkpoint {
        return Ok(VersionRead::or_missing(get_object_lt_or_eq_version(
            max_version,
        )?));
    }
    if let Some(item) = get_object_by_key(version_at_checkpoint)? {
        return Ok(VersionRead::Selected(item));
    }
    Ok(match get_object_lt_or_eq_version(max_version)? {
        Some(item) if item.stored_version() < version_at_checkpoint => VersionRead::Selected(item),
        Some(_) => VersionRead::NotFound,
        None => VersionRead::Missing,
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::BTreeMap;

    use sui_types::base_types::ObjectDigest;
    use sui_types::base_types::ObjectID;
    use sui_types::base_types::SequenceNumber;
    use sui_types::base_types::SuiAddress;
    use sui_types::digests::TransactionDigest;
    use sui_types::effects::TransactionEffects;
    use sui_types::effects::TransactionEffectsAPI;
    use sui_types::execution_status::ExecutionStatus;
    use sui_types::gas::GasCostSummary;
    use sui_types::object::Object as StoredObject;
    use sui_types::object::Owner;
    use sui_types::storage::ObjectOrTombstone;

    use sui_rpc::proto::sui::rpc::v2::Object;
    use sui_sdk_types::Address;

    use super::LatestVersion;
    use super::ObjectRead;
    use super::ObjectReads;
    use super::ObjectSelector;
    use super::VersionRead;
    use super::answer_missing_versions;
    use super::max_version_at_checkpoint;
    use super::read_object;
    use super::version_at_checkpoint;
    use crate::RpcError;
    use crate::ledger_history::object_versions::ObjectRemovalKind;
    use crate::ledger_history::query_options::assert_below_serving_floor_error;
    use crate::reader::AtCheckpointUpperBound;

    /// V1 effects of a transaction that deletes or wraps `object`, modified at
    /// `object_modified_at`, paying with `gas` modified at `gas_modified_at`, and executed at
    /// `executed_at`, which also counts read-only inputs the effects do not list.
    fn v1_removal(
        object: ObjectID,
        object_modified_at: u64,
        gas: ObjectID,
        gas_modified_at: u64,
        executed_at: u64,
        wrap: bool,
    ) -> TransactionEffects {
        let executed_at = SequenceNumber::from_u64(executed_at);
        let removed = if wrap {
            (object, executed_at, ObjectDigest::OBJECT_DIGEST_WRAPPED)
        } else {
            (object, executed_at, ObjectDigest::OBJECT_DIGEST_DELETED)
        };
        let (deleted, wrapped) = if wrap {
            (vec![], vec![removed])
        } else {
            (vec![removed], vec![])
        };
        TransactionEffects::new_from_execution_v1(
            ExecutionStatus::Success,
            0,
            GasCostSummary::default(),
            vec![
                (object, SequenceNumber::from_u64(object_modified_at)),
                (gas, SequenceNumber::from_u64(gas_modified_at)),
            ],
            vec![],
            TransactionDigest::random(),
            vec![],
            vec![],
            vec![],
            deleted,
            vec![],
            wrapped,
            (
                (gas, executed_at, ObjectDigest::random()),
                Owner::AddressOwner(SuiAddress::ZERO),
            ),
            None,
            vec![],
        )
    }

    /// One of the object's versions: the checkpoint that wrote it, whether the object is live at
    /// it, and the version the fullnode's rpc-store index records for it.
    struct Row {
        version: u64,
        checkpoint: u64,
        live: bool,
        indexed_as: u64,
    }

    /// `max_version` with `at_checkpoint` against a model of the chain rule
    /// (`find_object_lt_or_eq_version` bounded by the checkpoints), with each removal at its
    /// tombstone version, over a lifecycle whose wrap and deletion have V1 effects. The
    /// fullnode's rpc-store index records a removal at `TransactionEffects::lamport_version`,
    /// while the object store holds its tombstone at the removed object's ref version; for V1
    /// effects the first is recomputed from the modified objects alone, so it is lower. The
    /// object store also holds versions written above the highest indexed checkpoint, which
    /// must not count.
    ///
    /// For V1 removals this differs from Postgres `obj_versions`, which records the
    /// removal at the recomputed lamport version: for a `max_version` at or above that version
    /// and below the tombstone, Postgres answers that the object does not exist, while this read
    /// returns the version before the removal if that version was written by `at_checkpoint`
    /// (and nothing otherwise), as the chain does.
    #[test]
    fn max_version_at_checkpoint_follows_chain_rule() {
        let id = ObjectID::random();
        let gas = ObjectID::random();
        let wrap = v1_removal(id, 5, gas, 7, 1001, true);
        let delete = v1_removal(id, 2500, gas, 2400, 3001, false);
        let removal = |effects: &TransactionEffects| {
            let [(_, tombstone)] = effects.all_tombstones()[..] else {
                panic!("expected one tombstone");
            };
            let indexed_as = effects.lamport_version();
            assert!(indexed_as < tombstone, "{indexed_as} < {tombstone}");
            (tombstone.value(), indexed_as.value())
        };
        let (wrapped_at, wrap_indexed_as) = removal(&wrap);
        let (deleted_at, delete_indexed_as) = removal(&delete);
        assert_eq!((wrapped_at, wrap_indexed_as), (1001, 8));
        assert_eq!((deleted_at, delete_indexed_as), (3001, 2501));

        let live = |version, checkpoint| Row {
            version,
            checkpoint,
            live: true,
            indexed_as: version,
        };
        let removed = |(version, indexed_as), checkpoint| Row {
            version,
            checkpoint,
            live: false,
            indexed_as,
        };
        let lifecycle = [
            live(3, 1),
            live(5, 2),
            removed((wrapped_at, wrap_indexed_as), 3),
            live(2000, 4),
            live(2500, 6),
            removed((deleted_at, delete_indexed_as), 7),
        ];
        let objects: BTreeMap<SequenceNumber, Option<StoredObject>> = lifecycle
            .iter()
            .map(|row| {
                let version = SequenceNumber::from_u64(row.version);
                let object = row.live.then(|| {
                    StoredObject::with_id_owner_version_for_testing(
                        id,
                        version,
                        Owner::AddressOwner(SuiAddress::ZERO),
                    )
                });
                (version, object)
            })
            .collect();
        let indexed_version_at = |checkpoint: u64| {
            lifecycle
                .iter()
                .rfind(|row| row.checkpoint <= checkpoint)
                .map(|row| SequenceNumber::from_u64(row.indexed_as))
        };
        let read = |max_version: u64, checkpoint: u64, highest_indexed: u64| {
            let read = max_version_at_checkpoint(
                SequenceNumber::from_u64(max_version),
                indexed_version_at(checkpoint),
                || Ok(indexed_version_at(highest_indexed)),
                |version| Ok(objects.get(&version).cloned().flatten()),
                |version| {
                    Ok(objects
                        .range(..=version)
                        .next_back()
                        .and_then(|(_, object)| object.clone()))
                },
            )
            .unwrap();
            match read {
                VersionRead::Selected(object) => Ok(object.version().value()),
                VersionRead::NotFound => Err("not found"),
                VersionRead::NoVersionYet | VersionRead::Missing => Err("missing"),
            }
        };
        // The newest version at or below `max_version` written up to `highest_indexed`, or "not
        // found" if it was written after `checkpoint`, or "missing" if there is no such version
        // or the object had no version as of `checkpoint`, which the read cannot tell from index
        // history this node pruned. Removals sit at their tombstone versions, not at
        // `indexed_as`. A selected removal is "not found" by the chain rule, but the object
        // store's reads hide tombstones, so the read can only tell it apart from a pruned version
        // when it was written after `checkpoint` and the object is live at the end of
        // `checkpoint`; otherwise either answer (`None`) is accepted, never an object.
        let expected = |max_version: u64, checkpoint: u64, highest_indexed: u64| {
            let Some(at_checkpoint) = lifecycle.iter().rfind(|row| row.checkpoint <= checkpoint)
            else {
                return Some(Err("missing"));
            };
            let Some(selected) = lifecycle
                .iter()
                .rfind(|row| row.version <= max_version && row.checkpoint <= highest_indexed)
            else {
                return Some(Err("missing"));
            };
            if selected.live && selected.checkpoint <= checkpoint {
                Some(Ok(selected.version))
            } else if selected.live || (selected.checkpoint > checkpoint && at_checkpoint.live) {
                Some(Err("not found"))
            } else {
                None
            }
        };

        let mut max_versions: Vec<u64> = (0..=12).collect();
        for row in &lifecycle {
            max_versions.extend([
                row.version - 1,
                row.version,
                row.version + 1,
                row.indexed_as,
            ]);
        }
        max_versions.extend([1500, u64::MAX]);
        for highest_indexed in 0..=8 {
            for checkpoint in 0..=highest_indexed {
                for &max_version in &max_versions {
                    let actual = read(max_version, checkpoint, highest_indexed);
                    let context = format!(
                        "max_version {max_version} at_checkpoint {checkpoint} \
                         highest indexed {highest_indexed}"
                    );
                    match expected(max_version, checkpoint, highest_indexed) {
                        Some(expected) => assert_eq!(actual, expected, "{context}"),
                        None => assert!(
                            matches!(actual, Err("not found" | "missing")),
                            "{context}: {actual:?}"
                        ),
                    }
                }
            }
        }

        // A newer version at or below `max_version` written after the checkpoint is selected,
        // so the read finds nothing rather than an older version.
        assert_eq!(read(5, 1, 8), Err("not found"));
        assert_eq!(read(5, 2, 8), Ok(5));
        // A version above the highest indexed checkpoint does not count.
        assert_eq!(read(2500, 4, 5), Ok(2000));
        assert_eq!(read(2500, 4, 6), Err("not found"));
        // Between a V1 removal's indexed version and its tombstone, the version before it is
        // the newest (Postgres answers that the object does not exist), and at or above the
        // tombstone nothing is, which the read reports as missing: it cannot tell the wrap's
        // tombstone from a pruned version.
        assert_eq!(read(1000, 3, 8), Ok(5));
        assert_eq!(read(1001, 3, 8), Err("missing"));
        // A deletion written after the checkpoint, of the version live at its end, is not found
        // even though the read sees no live version at or below `max_version`.
        assert_eq!(read(deleted_at, 6, 8), Err("not found"));
        assert_eq!(read(u64::MAX, 6, 8), Err("not found"));
    }

    fn assert_not_found(result: Result<Object, RpcError>) {
        let status = result.unwrap_err().into_status_proto();
        assert_eq!(status.code, tonic::Code::NotFound as i32, "{status:?}");
    }

    fn version(version: u64) -> ObjectSelector {
        ObjectSelector::Version {
            version,
            at_checkpoint: None,
        }
    }

    fn max_version(max_version: u64) -> ObjectSelector {
        ObjectSelector::MaxVersion {
            max_version,
            at_checkpoint: None,
        }
    }

    /// Answer one read that found its version missing, on a node with `objects_lower_bound`
    /// whose object's latest version is `latest`, live.
    fn answer_missing(
        selector: ObjectSelector,
        objects_lower_bound: u64,
        latest: Option<u64>,
    ) -> Result<Object, RpcError> {
        answer_missing_with_removal(selector, objects_lower_bound, latest, false)
    }

    /// [`answer_missing`] when the object's latest version is a deletion or wrap if `removed`.
    fn answer_missing_with_removal(
        selector: ObjectSelector,
        objects_lower_bound: u64,
        latest: Option<u64>,
        removed: bool,
    ) -> Result<Object, RpcError> {
        let id = Address::ZERO;
        let [result] = answer_missing_versions(
            &[(id, selector)],
            vec![Ok(None)],
            || Ok(objects_lower_bound),
            |_| {
                latest.map(|version| LatestVersion {
                    version: SequenceNumber::from_u64(version),
                    removed,
                })
            },
        )
        .unwrap()
        .try_into()
        .unwrap();
        result
    }

    /// A missing version is `NOT_FOUND` on a node that holds every version. Otherwise a node whose
    /// object lower bound is above 0 may have pruned it, and refuses the read with that lower
    /// bound.
    #[test]
    fn missing_versions_are_refused_on_pruned_nodes() {
        const FLOOR: u64 = 42;
        // A version below the latest live one, read exactly or as a `max_version`.
        assert_below_serving_floor_error(
            answer_missing(version(5), FLOOR, Some(10)).unwrap_err(),
            FLOOR,
        );
        assert_below_serving_floor_error(
            answer_missing(max_version(5), FLOOR, Some(10)).unwrap_err(),
            FLOOR,
        );
        // An object with no live version left, or none ever.
        assert_below_serving_floor_error(
            answer_missing(version(5), FLOOR, None).unwrap_err(),
            FLOOR,
        );
        assert_below_serving_floor_error(
            answer_missing(max_version(u64::MAX), FLOOR, None).unwrap_err(),
            FLOOR,
        );
        // With `at_checkpoint`, by the same rule.
        let at_checkpoint = ObjectSelector::Version {
            version: 5,
            at_checkpoint: Some(100),
        };
        assert_below_serving_floor_error(
            answer_missing(at_checkpoint, FLOOR, Some(10)).unwrap_err(),
            FLOOR,
        );

        // An exact version is refused whatever the latest version: one above it is `NOT_FOUND`
        // before the read is left to `missing_version_error` (see `exact_version_after_miss`).
        assert_below_serving_floor_error(
            answer_missing(version(10), FLOOR, Some(10)).unwrap_err(),
            FLOOR,
        );

        // A node that was neither pruned nor restored holds every version.
        for selector in [version(5), version(11), max_version(5), at_checkpoint] {
            for latest in [None, Some(10)] {
                assert_not_found(answer_missing(selector, 0, latest));
            }
        }
    }

    /// An exact version above the object's latest version, live or a removal, is `NOT_FOUND`.
    /// The latest version is read after the miss, so a version written between the two reads is
    /// read again and served, rather than left missing (and refused as pruned).
    #[test]
    fn exact_version_misses_check_the_latest_version_after_the_miss() {
        let id = ObjectID::random();
        let latest = |version: u64, removed: bool| LatestVersion {
            version: SequenceNumber::from_u64(version),
            removed,
        };
        let answer = |version: u64, latest: Option<LatestVersion>, found_again: bool| {
            let read_again = std::cell::Cell::new(false);
            let read = super::exact_version_after_miss(
                SequenceNumber::from_u64(version),
                || latest,
                || {
                    read_again.set(true);
                    Ok(if found_again {
                        ObjectRead::Live(StoredObject::with_id_owner_version_for_testing(
                            id,
                            SequenceNumber::from_u64(version),
                            Owner::AddressOwner(SuiAddress::ZERO),
                        ))
                    } else {
                        ObjectRead::Missing
                    })
                },
            )
            .unwrap();
            let answer = match read {
                ObjectRead::Live(object) => Ok(object.version().value()),
                ObjectRead::NotFound(None) => Err("not found"),
                ObjectRead::Missing => Err("missing"),
                other => panic!("unexpected {other:?}"),
            };
            (answer, read_again.get())
        };
        // Not written yet, above a live version or a removal: no second read.
        assert_eq!(
            answer(11, Some(latest(10, false)), true),
            (Err("not found"), false)
        );
        assert_eq!(
            answer(11, Some(latest(10, true)), true),
            (Err("not found"), false)
        );
        // At or below the latest version and still absent: possibly pruned.
        assert_eq!(
            answer(5, Some(latest(10, false)), false),
            (Err("missing"), true)
        );
        // No version at all.
        assert_eq!(answer(5, None, true), (Err("missing"), false));
        // Version 11 is written between the miss and the read of the latest version.
        assert_eq!(answer(11, Some(latest(11, false)), true), (Ok(11), true));
    }

    /// On a pruned node, a `max_version` read without `at_checkpoint` that finds no live version
    /// and resolves to the object's latest version, a deletion or wrap at or below it, is
    /// `NOT_FOUND` (the latest version includes changes not committed yet). A `max_version` read
    /// with `at_checkpoint`, or one below the removal, is still refused: the version it selects
    /// may be an older one, pruned.
    #[test]
    fn missing_versions_resolving_to_the_latest_removal_are_not_found() {
        const FLOOR: u64 = 42;
        let removed_at_10 =
            |selector: ObjectSelector| answer_missing_with_removal(selector, FLOOR, Some(10), true);
        assert_not_found(removed_at_10(max_version(10)));
        assert_not_found(removed_at_10(max_version(u64::MAX)));
        assert_below_serving_floor_error(removed_at_10(max_version(9)).unwrap_err(), FLOOR);
        // A live latest version at or below `max_version` decides nothing.
        assert_below_serving_floor_error(
            answer_missing_with_removal(max_version(12), FLOOR, Some(10), false).unwrap_err(),
            FLOOR,
        );
        // With `at_checkpoint`, the read counts only versions written by the checkpoint, and the
        // removal may have been written after it.
        let at_checkpoint = ObjectSelector::MaxVersion {
            max_version: 12,
            at_checkpoint: Some(5),
        };
        assert_below_serving_floor_error(removed_at_10(at_checkpoint).unwrap_err(), FLOOR);
    }

    /// In a batch, a missing version fails only its own item, the lower bound is read once, and
    /// only if some read found a version missing.
    #[test]
    fn missing_versions_fail_only_their_own_batch_items() {
        let missing = Address::new([1; 32]);
        let found = Address::new([2; 32]);
        let requests = [
            (missing, version(5)),
            (found, version(3)),
            (found, max_version(9)),
        ];
        let mut lower_bound_reads = 0;
        let results = answer_missing_versions(
            &requests,
            vec![
                Ok(None),
                Ok(Some(Object::default())),
                Ok(Some(Object::default())),
            ],
            || {
                lower_bound_reads += 1;
                Ok(7)
            },
            |object_id| {
                assert_eq!(object_id, missing);
                Some(LatestVersion {
                    version: SequenceNumber::from_u64(10),
                    removed: false,
                })
            },
        )
        .unwrap();
        assert_eq!(lower_bound_reads, 1);
        let [refused, served, served_max] = results.try_into().unwrap();
        assert_below_serving_floor_error(refused.unwrap_err(), 7);
        assert_eq!(served.unwrap(), Object::default());
        assert_eq!(served_max.unwrap(), Object::default());

        let results = answer_missing_versions(
            &requests[1..],
            vec![
                Ok(Some(Object::default())),
                Err(requests[2].1.not_found(found).into()),
            ],
            || panic!("no read found a version missing"),
            |_| panic!("no read found a version missing"),
        )
        .unwrap();
        let [served, not_found] = results.try_into().unwrap();
        assert_eq!(served.unwrap(), Object::default());
        assert_not_found(not_found);
    }

    /// With `at_checkpoint`, a version written after the checkpoint and a deletion or wrap at the
    /// version the checkpoint ends with are `NOT_FOUND` whatever the node pruned. A version below
    /// that one with no live object may have been pruned, and so may the index history of an
    /// object the version index records no version of as of the checkpoint: the fullnode pruner
    /// retracts every index row of an object deleted or wrapped in a pruned checkpoint.
    #[test]
    fn at_checkpoint_visibility_misses_stay_not_found() {
        let id = ObjectID::random();
        let object = |version: u64| {
            StoredObject::with_id_owner_version_for_testing(
                id,
                SequenceNumber::from_u64(version),
                Owner::AddressOwner(SuiAddress::ZERO),
            )
        };
        let v = SequenceNumber::from_u64;
        let exact =
            |version: u64, version_at: Option<u64>, held: &[u64]| match version_at_checkpoint(
                v(version),
                version_at.map(v),
                |version| {
                    Ok(held
                        .contains(&version.value())
                        .then(|| object(version.value())))
                },
            )
            .unwrap()
            {
                VersionRead::Selected(object) => Ok(object.version().value()),
                VersionRead::NotFound => Err("not found"),
                VersionRead::NoVersionYet | VersionRead::Missing => Err("missing"),
            };
        // The object is live at 5 at the end of the checkpoint; this node pruned 3.
        assert_eq!(exact(5, Some(5), &[5, 7]), Ok(5));
        assert_eq!(exact(7, Some(5), &[5, 7]), Err("not found"));
        assert_eq!(exact(3, Some(5), &[5, 7]), Err("missing"));
        // The checkpoint ends with a deletion at 6.
        assert_eq!(exact(6, Some(6), &[5]), Err("not found"));
        // No index row as of the checkpoint: the object may not exist yet, or it was created at 2
        // and deleted at 3 in checkpoints this node pruned, index rows included.
        assert_eq!(exact(9, None, &[5, 7]), Err("missing"));
        assert_eq!(exact(2, None, &[]), Err("missing"));

        // Versions 3, 5 and 7 written at checkpoints 1, 2 and 3, and a deletion at 9 written at
        // checkpoint 4; this node pruned 3 and 5 and holds the deletion's tombstone.
        let version_at = |checkpoint: u64| match checkpoint {
            0 => None,
            1 => Some(v(3)),
            2 => Some(v(5)),
            3 => Some(v(7)),
            _ => Some(v(9)),
        };
        let max = |max_version: u64, checkpoint: u64| match max_version_at_checkpoint(
            v(max_version),
            version_at(checkpoint),
            || Ok(version_at(4)),
            |version| Ok((version == v(7)).then(|| object(7))),
            |version| Ok((v(7)..v(9)).contains(&version).then(|| object(7))),
        )
        .unwrap()
        {
            VersionRead::Selected(object) => Ok(object.version().value()),
            VersionRead::NotFound => Err("not found"),
            VersionRead::NoVersionYet | VersionRead::Missing => Err("missing"),
        };
        // 7 is selected but was written after checkpoint 2.
        assert_eq!(max(8, 2), Err("not found"));
        // No index row as of checkpoint 0, which the read cannot tell from pruned index history.
        assert_eq!(max(8, 0), Err("missing"));
        assert_eq!(max(8, 3), Ok(7));
        // The deletion is selected but was written after checkpoint 3, where 7 is live.
        assert_eq!(max(9, 3), Err("not found"));
        assert_eq!(max(u64::MAX, 3), Err("not found"));
        // Selecting 5 needs a version this node pruned.
        assert_eq!(max(5, 3), Err("missing"));
        assert_eq!(max(5, 2), Err("missing"));
        // At checkpoint 4 the deletion is selected, which the object store's reads do not tell
        // apart from a pruned version.
        assert_eq!(max(9, 4), Err("missing"));
    }

    /// `max_version` with `at_checkpoint`, read with tombstones over `lifecycle` as `select`
    /// reads it, with the index recording each version where the object store holds it. The
    /// selected item's version, and whether it is live.
    fn read_with_tombstones(
        lifecycle: &[Row],
        max_version: u64,
        checkpoint: u64,
        highest_indexed: u64,
    ) -> Option<(u64, bool)> {
        let id = ObjectID::random();
        let items: BTreeMap<SequenceNumber, ObjectOrTombstone> = lifecycle
            .iter()
            .map(|row| {
                let version = SequenceNumber::from_u64(row.version);
                let item = if row.live {
                    ObjectOrTombstone::Object(StoredObject::with_id_owner_version_for_testing(
                        id,
                        version,
                        Owner::AddressOwner(SuiAddress::ZERO),
                    ))
                } else {
                    ObjectOrTombstone::Tombstone((id, version, ObjectDigest::OBJECT_DIGEST_DELETED))
                };
                (version, item)
            })
            .collect();
        let version_at = |checkpoint: u64| {
            let row = lifecycle.iter().rfind(|row| row.checkpoint <= checkpoint)?;
            Some(SequenceNumber::from_u64(row.version))
        };
        let read = max_version_at_checkpoint(
            SequenceNumber::from_u64(max_version),
            version_at(checkpoint),
            || Ok(version_at(highest_indexed)),
            |version| Ok(items.get(&version).cloned()),
            |version| {
                Ok(items
                    .range(..=version)
                    .next_back()
                    .map(|(_, item)| item.clone()))
            },
        )
        .unwrap();
        match read {
            VersionRead::Selected(ObjectOrTombstone::Object(object)) => {
                Some((object.version().value(), true))
            }
            VersionRead::Selected(ObjectOrTombstone::Tombstone((_, version, _))) => {
                Some((version.value(), false))
            }
            VersionRead::NotFound | VersionRead::NoVersionYet | VersionRead::Missing => None,
        }
    }

    /// Read with tombstones, `max_version` with `at_checkpoint` selects the removal the read
    /// resolves to, only when that removal was written by the checkpoint, and otherwise the same
    /// live version or nothing as the read of live objects.
    #[test]
    fn max_version_at_checkpoint_selects_removals_written_by_the_checkpoint() {
        let row = |version, checkpoint, live| Row {
            version,
            checkpoint,
            live,
            indexed_as: version,
        };
        // Created, mutated, wrapped, unwrapped, mutated, deleted.
        let lifecycle = [
            row(3, 1, true),
            row(5, 2, true),
            row(6, 3, false),
            row(8, 4, true),
            row(9, 6, true),
            row(10, 7, false),
        ];
        // The newest version at or below `max_version` written up to `highest_indexed`, live or
        // removed, unless it was written after `checkpoint`.
        let expected = |max_version: u64, checkpoint: u64, highest_indexed: u64| {
            lifecycle
                .iter()
                .rfind(|row| row.version <= max_version && row.checkpoint <= highest_indexed)
                .filter(|row| row.checkpoint <= checkpoint)
                .map(|row| (row.version, row.live))
        };
        for highest_indexed in 0..=8 {
            for checkpoint in 0..=highest_indexed {
                for max_version in (0..=12).chain([u64::MAX]) {
                    assert_eq!(
                        read_with_tombstones(&lifecycle, max_version, checkpoint, highest_indexed),
                        expected(max_version, checkpoint, highest_indexed),
                        "max_version {max_version} at_checkpoint {checkpoint} \
                         highest indexed {highest_indexed}",
                    );
                }
            }
        }
        // The wrap at version 6 answers at checkpoint 3, but not at checkpoint 2, before it.
        assert_eq!(read_with_tombstones(&lifecycle, 7, 3, 8), Some((6, false)));
        assert_eq!(read_with_tombstones(&lifecycle, 7, 2, 8), None);
    }

    /// How an object changed at one of its versions.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Change {
        Live,
        Wrapped,
        Deleted,
    }

    /// An object's history as `(version, checkpoint that wrote it, change)`, with the version
    /// index recording each removal at its tombstone's version, and the reads made of it counted.
    struct HistoryReads<'a> {
        id: ObjectID,
        history: &'a [(u64, u64, Change)],
        reads: Cell<usize>,
    }

    impl HistoryReads<'_> {
        fn item(&self, &(version, _, change): &(u64, u64, Change)) -> ObjectOrTombstone {
            let version = SequenceNumber::from_u64(version);
            match change {
                Change::Live => {
                    ObjectOrTombstone::Object(StoredObject::with_id_owner_version_for_testing(
                        self.id,
                        version,
                        Owner::AddressOwner(SuiAddress::ZERO),
                    ))
                }
                Change::Wrapped => ObjectOrTombstone::Tombstone((
                    self.id,
                    version,
                    ObjectDigest::OBJECT_DIGEST_WRAPPED,
                )),
                Change::Deleted => ObjectOrTombstone::Tombstone((
                    self.id,
                    version,
                    ObjectDigest::OBJECT_DIGEST_DELETED,
                )),
            }
        }

        /// The change `pick` finds, counted as one read.
        fn read(
            &self,
            pick: impl Fn(&(u64, u64, Change)) -> bool,
            newest: bool,
        ) -> Option<ObjectOrTombstone> {
            self.reads.set(self.reads.get() + 1);
            let found = if newest {
                self.history.iter().rfind(|change| pick(change))
            } else {
                self.history.iter().find(|change| pick(change))
            };
            found.map(|change| self.item(change))
        }
    }

    /// What a read of a [`HistoryReads`] item returns: the item itself with tombstones, or the live
    /// object, where a removal reads as `None`.
    trait FromItem: Sized {
        fn from_item(item: ObjectOrTombstone) -> Option<Self>;
    }

    impl FromItem for ObjectOrTombstone {
        fn from_item(item: ObjectOrTombstone) -> Option<Self> {
            Some(item)
        }
    }

    impl FromItem for StoredObject {
        fn from_item(item: ObjectOrTombstone) -> Option<Self> {
            match item {
                ObjectOrTombstone::Object(object) => Some(object),
                ObjectOrTombstone::Tombstone(_) => None,
            }
        }
    }

    /// [`ObjectReads`] over a [`HistoryReads`], of live objects (`T` = `StoredObject`) or with
    /// tombstones (`T` = `ObjectOrTombstone`).
    struct Reads<'a, T> {
        history: HistoryReads<'a>,
        item: std::marker::PhantomData<T>,
    }

    impl<'a, T> Reads<'a, T> {
        fn new(id: ObjectID, history: &'a [(u64, u64, Change)]) -> Self {
            Self {
                history: HistoryReads {
                    id,
                    history,
                    reads: Cell::new(0),
                },
                item: std::marker::PhantomData,
            }
        }

        fn count(&self) -> usize {
            self.history.reads.get()
        }
    }

    impl<T: FromItem + super::StoredVersion> ObjectReads for Reads<'_, T> {
        type Item = T;

        fn latest(&self, _id: ObjectID) -> Result<Option<T>, RpcError> {
            Ok(self.history.read(|_| true, true).and_then(T::from_item))
        }

        fn by_key(&self, _id: ObjectID, version: SequenceNumber) -> Result<Option<T>, RpcError> {
            Ok(self
                .history
                .read(|&(v, _, _)| v == version.value(), false)
                .and_then(T::from_item))
        }

        fn lt_or_eq(&self, _id: ObjectID, version: SequenceNumber) -> Result<Option<T>, RpcError> {
            Ok(self
                .history
                .read(|&(v, _, _)| v <= version.value(), true)
                .and_then(T::from_item))
        }

        fn version_at_checkpoint(
            &self,
            _id: ObjectID,
            checkpoint: u64,
            upper_bound: &AtCheckpointUpperBound,
        ) -> Result<Option<SequenceNumber>, RpcError> {
            upper_bound.check(checkpoint)?;
            Ok(self
                .history
                .read(|&(_, written_at, _)| written_at <= checkpoint, true)
                .map(|item| super::stored_version(&item)))
        }
    }

    /// What [`read_object`] answered, with the reads it made: through live objects and with
    /// tombstones.
    #[derive(Debug, PartialEq, Eq)]
    enum Answer {
        Live(u64),
        NotFound,
        Removed(u64, ObjectRemovalKind),
        Missing,
    }

    fn read_history(
        history: &[(u64, u64, Change)],
        selector: ObjectSelector,
        upper_bound: Option<&AtCheckpointUpperBound>,
    ) -> (Answer, [usize; 2]) {
        let id = ObjectID::random();
        let live = Reads::<StoredObject>::new(id, history);
        let with_tombstones = Reads::<ObjectOrTombstone>::new(id, history);
        let read = read_object(&live, &with_tombstones, id, selector, upper_bound).unwrap();
        let answer = match read {
            ObjectRead::Live(object) => Answer::Live(object.version().value()),
            ObjectRead::NotFound(None) => Answer::NotFound,
            ObjectRead::NotFound(Some(removal)) => Answer::Removed(removal.version, removal.kind),
            ObjectRead::Missing => Answer::Missing,
        };
        (answer, [live.count(), with_tombstones.count()])
    }

    /// An `at_checkpoint` or exact-version read of an object that never existed costs one read;
    /// an `at_checkpoint` read learns it from the version index and never looks for a removal.
    /// The index cannot tell such an object from one whose index history this node pruned, so
    /// only `at_checkpoint` alone answers `NOT_FOUND`, and a `version` or `max_version` read with
    /// it is missing data. Without `at_checkpoint` there is no index to ask, so an exact version
    /// looks for a tombstone at that version, and a `max_version` makes its one read with
    /// tombstones.
    #[test]
    fn reads_of_objects_that_never_existed_look_for_no_removal() {
        let indexed = AtCheckpointUpperBound::new(5);
        assert_eq!(
            read_history(&[], ObjectSelector::AtCheckpoint(3), Some(&indexed)),
            (Answer::NotFound, [1, 0])
        );
        for selector in [
            ObjectSelector::Version {
                version: 3,
                at_checkpoint: Some(3),
            },
            ObjectSelector::MaxVersion {
                max_version: 3,
                at_checkpoint: Some(3),
            },
        ] {
            assert_eq!(
                read_history(&[], selector, Some(&indexed)),
                (Answer::Missing, [1, 0]),
                "{selector:?}"
            );
        }
        assert_eq!(
            read_history(&[], version(3), None),
            (Answer::Missing, [1, 1])
        );
        assert_eq!(
            read_history(&[], max_version(3), None),
            (Answer::Missing, [0, 1])
        );
    }

    /// A read that resolves to a deletion or wrap answers `NOT_FOUND` describing it, including
    /// the reads whose live selection finds the version missing: an exact version, a version below
    /// the checkpoint's with `at_checkpoint`, and a `max_version` with or without
    /// `at_checkpoint`. A read that resolves to a version written after its checkpoint describes
    /// no removal.
    #[test]
    fn reads_resolving_to_removals_describe_them() {
        use Change::*;
        // Created at 3, wrapped at 4, unwrapped at 5 and deleted at 6, at checkpoints 1 to 4.
        let history = [(3, 1, Live), (4, 2, Wrapped), (5, 3, Live), (6, 4, Deleted)];
        let indexed = AtCheckpointUpperBound::new(4);
        let wrapped = Answer::Removed(4, ObjectRemovalKind::Wrapped);
        let deleted = Answer::Removed(6, ObjectRemovalKind::Deleted);
        let read = |selector, upper_bound| read_history(&history, selector, upper_bound).0;
        let with_at_checkpoint = |version, at_checkpoint| ObjectSelector::Version {
            version,
            at_checkpoint: Some(at_checkpoint),
        };
        let max_at_checkpoint = |max_version, at_checkpoint| ObjectSelector::MaxVersion {
            max_version,
            at_checkpoint: Some(at_checkpoint),
        };

        assert_eq!(read(ObjectSelector::Latest, None), deleted);
        assert_eq!(read(version(4), None), wrapped);
        assert_eq!(read(version(6), None), deleted);
        assert_eq!(read(version(5), None), Answer::Live(5));
        assert_eq!(read(with_at_checkpoint(4, 3), Some(&indexed)), wrapped);
        assert_eq!(read(with_at_checkpoint(6, 4), Some(&indexed)), deleted);
        assert_eq!(
            read(ObjectSelector::AtCheckpoint(2), Some(&indexed)),
            wrapped
        );
        assert_eq!(
            read(ObjectSelector::AtCheckpoint(0), Some(&indexed)),
            Answer::NotFound
        );
        // `max_version` with `at_checkpoint`.
        assert_eq!(read(max_at_checkpoint(4, 2), Some(&indexed)), wrapped);
        assert_eq!(read(max_at_checkpoint(9, 4), Some(&indexed)), deleted);
        // Version 5 is selected but was written after checkpoint 2.
        assert_eq!(
            read(max_at_checkpoint(5, 2), Some(&indexed)),
            Answer::NotFound
        );
        assert_eq!(
            read(max_at_checkpoint(3, 2), Some(&indexed)),
            Answer::Live(3)
        );

        // Without `at_checkpoint`, the newest entry at or below `max_version`, live or a tombstone,
        // decides, in one read with tombstones.
        assert_eq!(
            read_history(&history, max_version(4), None),
            (wrapped, [0, 1])
        );
        assert_eq!(
            read_history(&history, max_version(9), None),
            (deleted, [0, 1])
        );
        assert_eq!(
            read_history(&history, max_version(5), None),
            (Answer::Live(5), [0, 1])
        );
    }

    /// With the versions before them pruned, `read_object` still resolves `max_version` reads
    /// with `at_checkpoint` K to a removal that is the object's version at the end of K: one
    /// wrapped by K and unwrapped after it, read between the wrap and the unwrap, and one deleted
    /// at K, read at or above the deletion.
    #[test]
    fn removals_ending_the_checkpoint_resolve_with_earlier_versions_pruned() {
        use Change::*;
        // Created at 3 (checkpoint 1, pruned), wrapped at 4, unwrapped at 5 and deleted at 6, at
        // checkpoints 2 to 4.
        let history = [(4, 2, Wrapped), (5, 3, Live), (6, 4, Deleted)];
        let indexed = AtCheckpointUpperBound::new(4);
        let read = |max_version, at_checkpoint| {
            read_history(
                &history,
                ObjectSelector::MaxVersion {
                    max_version,
                    at_checkpoint: Some(at_checkpoint),
                },
                Some(&indexed),
            )
            .0
        };
        let wrapped = Answer::Removed(4, ObjectRemovalKind::Wrapped);
        let deleted = Answer::Removed(6, ObjectRemovalKind::Deleted);
        assert_eq!(read(4, 2), wrapped);
        assert_eq!(read(6, 4), deleted);
        assert_eq!(read(u64::MAX, 4), deleted);
        // The unwrap is selected but was written after checkpoint 2.
        assert_eq!(read(5, 2), Answer::NotFound);
    }

    /// A `max_version` read with `at_checkpoint` K of an object removed at K answers `NOT_FOUND`,
    /// with no removal described, when the newest version at or below `max_version` is a later
    /// removal: that removal was written after K. The live selection finds no live version there,
    /// and the selection with tombstones decides the answer.
    #[test]
    fn removed_at_the_checkpoint_and_removed_again_after_it_is_not_found() {
        use Change::*;
        // Created at 3, deleted at 4 (checkpoint 2), re-created at 5, deleted at 6 and re-created
        // at 7, at checkpoints 1 to 5.
        let history = [
            (3, 1, Live),
            (4, 2, Deleted),
            (5, 3, Live),
            (6, 4, Deleted),
            (7, 5, Live),
        ];
        let read = |history: &[(u64, u64, Change)], max_version, highest_indexed| {
            read_history(
                history,
                ObjectSelector::MaxVersion {
                    max_version,
                    at_checkpoint: Some(2),
                },
                Some(&AtCheckpointUpperBound::new(highest_indexed)),
            )
            .0
        };
        // The deletion at 6 is the newest version at or below 6, below the version at the height.
        assert_eq!(read(&history, 6, 5), Answer::NotFound);
        // The deletion at 6 is the object's version at the height.
        assert_eq!(read(&history[..4], 6, 4), Answer::NotFound);
        assert_eq!(read(&history[..4], u64::MAX, 4), Answer::NotFound);
        // The deletion at K itself still answers, described.
        assert_eq!(
            read(&history, 4, 5),
            Answer::Removed(4, ObjectRemovalKind::Deleted)
        );
    }
}

#[cfg(test)]
mod version_without_history_tests {
    use sui_rpc::proto::sui::rpc::v2::get_object_result;
    use sui_types::base_types::{ObjectDigest, ObjectID, SequenceNumber, SuiAddress};
    use sui_types::object::Owner;

    use super::*;
    use crate::reader::{NotServedHereCause, assert_not_served_here, assert_version_not_retained};
    use crate::stub_store::StubStore;

    /// A store holding versions 1 and 2 of one object, and that object's id.
    fn store_with_two_versions(retains_object_version_history: bool) -> (RpcService, Address) {
        let id = ObjectID::new([1; ObjectID::LENGTH]);
        let owner = Owner::AddressOwner(SuiAddress::ZERO);
        let objects = [1, 2]
            .map(|version| {
                sui_types::object::Object::with_id_owner_version_for_testing(
                    id,
                    SequenceNumber::from_u64(version),
                    owner.clone(),
                )
            })
            .to_vec();
        let service = StubStore {
            objects,
            retains_object_version_history,
            ..StubStore::new()
        }
        .into_service();
        (service, id.into())
    }

    /// A store without object version history holding `live` versions of one object and its
    /// tombstone at `removed_at`, and that object's id.
    fn store_with_removal(live: &[u64], removed_at: u64) -> (RpcService, Address) {
        let id = ObjectID::new([1; ObjectID::LENGTH]);
        let owner = Owner::AddressOwner(SuiAddress::ZERO);
        let objects = live
            .iter()
            .map(|&version| {
                sui_types::object::Object::with_id_owner_version_for_testing(
                    id,
                    SequenceNumber::from_u64(version),
                    owner.clone(),
                )
            })
            .collect();
        let service = StubStore {
            objects,
            tombstones: vec![(
                id,
                SequenceNumber::from_u64(removed_at),
                ObjectDigest::OBJECT_DIGEST_WRAPPED,
            )],
            retains_object_version_history: false,
            ..StubStore::new()
        }
        .into_service();
        (service, id.into())
    }

    /// The `removedVersion` of `status`'s `OBJECT_REMOVED` detail, which must be `NOT_FOUND`.
    fn removed_version(status: &crate::proto::google::rpc::Status) -> Option<String> {
        assert_eq!(status.code, tonic::Code::NotFound as i32, "{status:?}");
        status
            .details
            .iter()
            .filter_map(|detail| detail.to_msg::<crate::proto::google::rpc::ErrorInfo>().ok())
            .find(|info| info.reason == ErrorReason::ObjectRemoved.as_str_name())
            .and_then(|info| info.metadata.get("removedVersion").cloned())
    }

    fn request(object_id: Address, version: Option<u64>) -> GetObjectRequest {
        let mut request = GetObjectRequest::default();
        request.object_id = Some(object_id.to_string());
        request.version = version;
        request.read_mask = Some(FieldMask::from_str("object_id,version"));
        request
    }

    fn version_of(response: Result<GetObjectResponse, RpcError>) -> u64 {
        response.unwrap().object.unwrap().version.unwrap()
    }

    /// A node without object version history serves an object's latest version, by `version` or
    /// without one, answers `NotFound` for a version not written yet and for an object it holds
    /// no version of, and refuses every older version, including one it still stores.
    #[test]
    fn without_history_only_the_latest_version_is_served() {
        let (service, id) = store_with_two_versions(false);

        assert_eq!(version_of(get_object(&service, request(id, None))), 2);
        assert_eq!(version_of(get_object(&service, request(id, Some(2)))), 2);
        assert_version_not_retained(get_object(&service, request(id, Some(1))).unwrap_err());
        let error = get_object(&service, request(id, Some(3))).unwrap_err();
        assert_eq!(error.into_status_proto().code, tonic::Code::NotFound as i32);
        let missing = Address::new([2; 32]);
        let error = get_object(&service, request(missing, Some(1))).unwrap_err();
        assert_eq!(error.into_status_proto().code, tonic::Code::NotFound as i32);

        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = vec![
            request(id, Some(1)),
            request(id, Some(2)),
            request(id, Some(3)),
            request(missing, Some(1)),
        ];
        batch.read_mask = Some(FieldMask::from_str("object_id,version"));
        let response = batch_get_objects(&service, batch).unwrap();
        let [older, latest, above_latest, never_existed] = response.objects.as_slice() else {
            panic!("expected four results in {response:?}");
        };
        let status_of = |result: &GetObjectResult| match &result.result {
            Some(get_object_result::Result::Error(status)) => status.clone(),
            _ => panic!("expected an error in {response:?}"),
        };
        assert_eq!(
            status_of(older).code,
            tonic::Code::OutOfRange as i32,
            "{response:?}"
        );
        assert!(
            matches!(
                &latest.result,
                Some(get_object_result::Result::Object(object)) if object.version == Some(2)
            ),
            "{response:?}"
        );
        assert_eq!(
            status_of(above_latest).code,
            tonic::Code::NotFound as i32,
            "{response:?}"
        );
        assert_eq!(
            status_of(never_existed).code,
            tonic::Code::NotFound as i32,
            "{response:?}"
        );
    }

    /// A node with object version history serves any stored version and answers `NotFound` for
    /// one it does not store.
    #[test]
    fn with_history_any_stored_version_is_served() {
        let (service, id) = store_with_two_versions(true);

        assert_eq!(version_of(get_object(&service, request(id, Some(1)))), 1);
        let error = get_object(&service, request(id, Some(3))).unwrap_err();
        assert_eq!(error.into_status_proto().code, tonic::Code::NotFound as i32);
    }

    /// A node with object version history but without rpc indexes refuses every `max_version`
    /// read, with or without `at_checkpoint`, alone or with other reads in a batch, which it
    /// refuses whole, and still serves `version` reads.
    #[test]
    fn without_rpc_indexes_max_version_reads_are_refused() {
        let (service, id) = store_with_two_versions(true);
        let max_version = |max_version, at_checkpoint| {
            let mut request = request(id, None);
            request.max_version = Some(max_version);
            request.at_checkpoint = at_checkpoint;
            request
        };
        for request in [
            max_version(1, None),
            max_version(2, None),
            max_version(u64::MAX, None),
            max_version(2, Some(0)),
        ] {
            assert_not_served_here(
                get_object(&service, request.clone()).unwrap_err(),
                NotServedHereCause::NoRpcIndexes,
            );
            let mut batch = BatchGetObjectsRequest::default();
            batch.requests = vec![self::request(id, Some(1)), request];
            batch.read_mask = Some(FieldMask::from_str("object_id,version"));
            assert_not_served_here(
                batch_get_objects(&service, batch).unwrap_err(),
                NotServedHereCause::NoRpcIndexes,
            );
        }
        assert_eq!(version_of(get_object(&service, request(id, Some(1)))), 1);
    }

    /// A node without object version history decides an exact `version` from the object's
    /// newest entry even when that entry is a tombstone: the removal version answers `NotFound`
    /// with the removal described, a version above it `NotFound`, and every older version is
    /// refused, through GetObject and BatchGetObjects alike.
    #[test]
    fn without_history_a_removed_object_is_answered_from_its_tombstone() {
        let (service, id) = store_with_removal(&[1, 2], 3);

        let status = get_object(&service, request(id, Some(3)))
            .unwrap_err()
            .into_status_proto();
        assert_eq!(removed_version(&status).as_deref(), Some("3"), "{status:?}");
        let status = get_object(&service, request(id, Some(4)))
            .unwrap_err()
            .into_status_proto();
        assert_eq!(status.code, tonic::Code::NotFound as i32, "{status:?}");
        assert_version_not_retained(get_object(&service, request(id, Some(2))).unwrap_err());

        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = vec![
            request(id, Some(3)),
            request(id, Some(4)),
            request(id, Some(2)),
        ];
        batch.read_mask = Some(FieldMask::from_str("object_id,version"));
        let response = batch_get_objects(&service, batch).unwrap();
        let statuses: Vec<_> = response
            .objects
            .iter()
            .map(|result| match &result.result {
                Some(get_object_result::Result::Error(status)) => status,
                _ => panic!("expected only errors in {response:?}"),
            })
            .collect();
        let [at_removal, above_removal, below_removal] = statuses.as_slice() else {
            panic!("expected three results in {response:?}");
        };
        assert_eq!(
            removed_version(at_removal).as_deref(),
            Some("3"),
            "{response:?}"
        );
        assert_eq!(
            above_removal.code,
            tonic::Code::NotFound as i32,
            "{response:?}"
        );
        assert_eq!(
            below_removal.code,
            tonic::Code::OutOfRange as i32,
            "{response:?}"
        );
    }
}
