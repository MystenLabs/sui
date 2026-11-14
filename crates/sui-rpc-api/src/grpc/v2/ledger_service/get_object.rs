// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ErrorReason;
use crate::RpcError;
use crate::RpcService;
use crate::error::ObjectNotFoundError;
use crate::grpc::v2::render::RenderContext;
use crate::ledger_history::query_options::below_serving_floor;
use crate::reader::AtCheckpointUpperBound;
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
use sui_types::base_types::SequenceNumber;
use sui_types::object::Object as StoredObject;

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
    /// `BELOW_EARLIEST_AVAILABLE` rather than `NOT_FOUND`, unless the version is above the
    /// object's latest live version or `at_checkpoint` decides the answer without it (see
    /// `resolve_missing_versions` in this module).
    Version {
        version: u64,
        at_checkpoint: Option<u64>,
    },
    /// The newest version at or below `max_version`, unless the object was deleted or wrapped at
    /// that version. With `at_checkpoint`, versions count up to the height the server answers
    /// `at_checkpoint` reads at, and the read fails if the selected version was written after the
    /// checkpoint, so it agrees with a `version` read of the selected version at that checkpoint.
    /// Without it, the read is pinned to that height and answers as if `at_checkpoint` were set
    /// to it: kv-rpc's served height, since versions above it may be in its tables before their
    /// deletions and wraps are, and a fullnode's highest indexed checkpoint. A fullnode without
    /// rpc indexes, or whose indexes are empty or still backfilling history after a restore,
    /// counts every version it holds. A fullnode whose object lower bound is above 0
    /// answers a read that finds no live version to select with `OUT_OF_RANGE` /
    /// `BELOW_EARLIEST_AVAILABLE` rather than `NOT_FOUND`, since the versions it would select
    /// from may have been pruned.
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

    /// Whether the read needs versions that later transactions replaced, which a node without
    /// object version history no longer has: every read with `at_checkpoint`, and every
    /// `max_version` read.
    pub fn needs_object_version_history(&self) -> bool {
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
/// `max_version` is rejected. `at_checkpoint` combines with either.
pub fn validate_get_object_requests(
    requests: &[GetObjectRequest],
    read_mask: Option<FieldMask>,
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
                    return Err(FieldViolation::new("max_version")
                        .with_description("version and max_version are mutually exclusive")
                        .with_reason(ErrorReason::FieldInvalid)
                        .nested_at("requests", idx)
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
        validate_get_object_requests(std::slice::from_ref(&request), read_mask)?;
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

    let (requests, read_mask) = validate_get_object_requests(&requests, read_mask)?;

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

/// Before the reads, once per request: refuse the whole request if some item needs object
/// version history and this node keeps none, and load the upper bound the reads at a checkpoint
/// use (see [`upper_bound_for_requests`]). Other reads still work on nodes without rpc indexes or
/// object version history.
fn load_at_checkpoint_upper_bound(
    service: &RpcService,
    requests: &[(Address, ObjectSelector)],
) -> Result<Option<AtCheckpointUpperBound>, RpcError> {
    if requests
        .iter()
        .any(|(_, selector)| selector.needs_object_version_history())
    {
        service.reader.require_object_version_history()?;
    }
    upper_bound_for_requests(
        requests,
        || service.reader.at_checkpoint_upper_bound(),
        || {
            let Some(indexes) = service.reader.inner().indexes() else {
                return Ok(None);
            };
            max_version_upper_bound(
                || Ok(indexes.is_history_backfill_pending()?),
                || Ok(indexes.get_highest_indexed_checkpoint_seq_number()?),
            )
        },
    )
}

/// The upper bound for `requests`' reads at a checkpoint (see [`pinned_checkpoint`]). If some
/// item sets `at_checkpoint`, `at_checkpoint_upper_bound`
/// (`StateReader::at_checkpoint_upper_bound`), whose failure fails the whole request. Otherwise,
/// if some item reads `max_version` without `at_checkpoint`, `max_version_upper_bound`, which is
/// `None` on a node with no rpc indexes (see [`max_version_upper_bound`]). Otherwise `None`.
fn upper_bound_for_requests(
    requests: &[(Address, ObjectSelector)],
    at_checkpoint_upper_bound: impl FnOnce() -> Result<AtCheckpointUpperBound, RpcError>,
    max_version_upper_bound: impl FnOnce() -> Result<Option<AtCheckpointUpperBound>, RpcError>,
) -> Result<Option<AtCheckpointUpperBound>, RpcError> {
    if requests
        .iter()
        .any(|(_, selector)| selector.at_checkpoint().is_some())
    {
        return at_checkpoint_upper_bound().map(Some);
    }
    let reads_unpinned_max_version = requests.iter().any(|(_, selector)| {
        matches!(
            selector,
            ObjectSelector::MaxVersion {
                at_checkpoint: None,
                ..
            }
        )
    });
    if reads_unpinned_max_version {
        max_version_upper_bound()
    } else {
        Ok(None)
    }
}

/// The upper bound a `max_version` read without `at_checkpoint` is pinned to on a node with rpc
/// indexes: their highest indexed checkpoint. `None` while the indexes are still backfilling
/// history after a restore or have not committed a checkpoint yet; the read then counts every
/// version the node holds, as on a node without rpc indexes, rather than fail.
fn max_version_upper_bound(
    history_backfill_pending: impl FnOnce() -> Result<bool, RpcError>,
    highest_indexed: impl FnOnce() -> Result<Option<u64>, RpcError>,
) -> Result<Option<AtCheckpointUpperBound>, RpcError> {
    if history_backfill_pending()? {
        return Ok(None);
    }
    Ok(highest_indexed()?.map(AtCheckpointUpperBound::new))
}

/// The checkpoint `selector` reads at: its `at_checkpoint`, or, for `max_version` without it,
/// the highest indexed checkpoint of `upper_bound` when the request loaded one (see
/// [`upper_bound_for_requests`]). Such a read answers as if it had set `at_checkpoint` to that
/// checkpoint. Without an upper bound it reads at no checkpoint.
fn pinned_checkpoint(
    selector: ObjectSelector,
    upper_bound: Option<&AtCheckpointUpperBound>,
) -> Option<u64> {
    match selector {
        ObjectSelector::MaxVersion {
            at_checkpoint: None,
            ..
        } => upper_bound.map(AtCheckpointUpperBound::highest_indexed),
        _ => selector.at_checkpoint(),
    }
}

/// After the reads: answer every item whose read found a version missing (`Ok(None)` from
/// [`get_object_impl`]), with this node's object lower bound
/// ([`sui_types::storage::RpcStateReader::get_lowest_available_checkpoint_objects`]) and its
/// objects' latest live versions. See [`missing_version_error`].
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
        |object_id| {
            reader
                .get_object(&ObjectID::from(object_id))
                .map(|object| object.version())
        },
    )
}

/// [`resolve_missing_versions`] with the object lower bound and the objects' latest live
/// versions read through `objects_lower_bound` and `latest_live_version`. `objects_lower_bound`
/// is called only if some read found a version missing.
fn answer_missing_versions(
    requests: &[(Address, ObjectSelector)],
    reads: Vec<Result<Option<Object>, RpcError>>,
    objects_lower_bound: impl FnOnce() -> Result<u64, RpcError>,
    latest_live_version: impl Fn(Address) -> Option<SequenceNumber>,
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
                || latest_live_version(object_id),
            )),
        })
        .collect())
}

/// The answer to a read that found the version it needs missing from the object store.
///
/// - `NOT_FOUND` if `objects_lower_bound` is 0: the node was neither pruned nor restored from a
///   snapshot, so it holds every version ever written, and the missing one never existed.
/// - `NOT_FOUND` for an exact `version` above the object's latest live version
///   (`latest_live_version`): it does not exist yet.
/// - Otherwise `OUT_OF_RANGE` / `BELOW_EARLIEST_AVAILABLE` with `objects_lower_bound` as the
///   lowest available checkpoint, since the version may have been pruned. That includes a
///   `max_version` read that finds no live version at or below `max_version`, and an object with
///   no version left. The object store does not tell a deletion or wrap from a pruned version,
///   so a read of an object deleted or wrapped at or below the version read, with no live
///   version above it, fails the same way.
fn missing_version_error(
    object_id: Address,
    selector: ObjectSelector,
    objects_lower_bound: u64,
    latest_live_version: impl FnOnce() -> Option<SequenceNumber>,
) -> RpcError {
    let not_yet_written = || match selector {
        ObjectSelector::Version { version, .. } => {
            latest_live_version().is_some_and(|latest| SequenceNumber::from_u64(version) > latest)
        }
        _ => false,
    };
    if objects_lower_bound == 0 || not_yet_written() {
        selector.not_found(object_id).into()
    } else {
        below_serving_floor(objects_lower_bound)
    }
}

/// After the reads: answer as pruned every item that reads at a checkpoint (see
/// [`pinned_checkpoint`]) below the lower bound (see `StateReader::at_checkpoint_lower_bound`),
/// replacing whatever its read returned, since that read may have seen pruned data.
fn apply_at_checkpoint_lower_bound(
    service: &RpcService,
    requests: &[(Address, ObjectSelector)],
    upper_bound: Option<&AtCheckpointUpperBound>,
    results: &mut [Result<Object, RpcError>],
) -> Result<(), RpcError> {
    // The request loads an upper bound exactly when some item reads at a checkpoint.
    if upper_bound.is_none() {
        return Ok(());
    }
    let lower_bound = service.reader.at_checkpoint_lower_bound()?;
    for (&(_, selector), result) in requests.iter().zip_debug_eq(results.iter_mut()) {
        if let Some(checkpoint) = pinned_checkpoint(selector, upper_bound)
            && let Err(error) = lower_bound.check(checkpoint)
        {
            *result = Err(error);
        }
    }
    Ok(())
}

/// What a read found for the version its selector picks.
#[derive(Debug)]
enum VersionRead {
    /// The selected version, live.
    Live(StoredObject),
    /// The answer is that the object does not exist (`NOT_FOUND`): the selected version was
    /// written after the checkpoint the read is pinned to, or deleted or wrapped the object, or
    /// the object had no version yet.
    NotFound,
    /// The object store has no live version where the read needs one. The version may never
    /// have existed, may be a deletion or wrap, or may have been pruned; see
    /// [`missing_version_error`].
    Missing,
}

impl VersionRead {
    /// The object, or [`Self::NotFound`] if there is none.
    fn or_not_found(object: Option<StoredObject>) -> Self {
        object.map_or(Self::NotFound, Self::Live)
    }

    /// The object, or [`Self::Missing`] if there is none.
    fn or_missing(object: Option<StoredObject>) -> Self {
        object.map_or(Self::Missing, Self::Live)
    }
}

/// Read one object. `upper_bound` is set whenever `selector` reads at a checkpoint (see
/// [`pinned_checkpoint`]); the reader checks it before any read at the checkpoint, so a
/// checkpoint above it fails the read whatever the object's state. `Ok(None)` if a `version` or
/// `max_version` read found the version it needs missing (see [`VersionRead::Missing`]);
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
    // A node without object version history serves only each object's latest version. A version
    // above it has not been written yet and is `NotFound`, as on any node. Any other version, and
    // any version of an object with no live version, is refused (`Unimplemented`,
    // `NOT_SERVED_HERE`, cause `no_object_history`) even while it is still stored, so that answer
    // does not change as compaction reaches it.
    if let ObjectSelector::Version {
        version,
        at_checkpoint: None,
    } = selector
        && let Err(refusal) = reader.require_object_version_history()
    {
        return match reader.inner().get_object(&id) {
            Some(object) if object.version().value() == version => Ok(Some(
                service.render_object_to_proto(&object, read_mask, render_context),
            )),
            Some(object) if object.version().value() < version => {
                Err(selector.not_found(object_id).into())
            }
            _ => Err(refusal),
        };
    }
    let read_checkpoint = pinned_checkpoint(selector, upper_bound);
    let upper_bound = || {
        upper_bound.ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                "at_checkpoint upper bound was not loaded",
            )
        })
    };
    let read = match selector {
        ObjectSelector::Latest => VersionRead::or_not_found(reader.inner().get_object(&id)),
        ObjectSelector::Version {
            version,
            at_checkpoint: None,
        } => VersionRead::or_missing(reader.inner().get_object_by_key(&id, version.into())),
        ObjectSelector::Version {
            version,
            at_checkpoint: Some(checkpoint),
        } => version_at_checkpoint(
            version.into(),
            reader.get_object_version_at_checkpoint(id, checkpoint, upper_bound()?)?,
            |version| reader.inner().get_object_by_key(&id, version),
        ),
        ObjectSelector::MaxVersion { max_version, .. } => read_max_version(
            max_version.into(),
            read_checkpoint
                .map(|checkpoint| upper_bound().map(|upper_bound| (checkpoint, upper_bound)))
                .transpose()?,
            |checkpoint, upper_bound| {
                reader.get_object_version_at_checkpoint(id, checkpoint, upper_bound)
            },
            |version| reader.inner().get_object_by_key(&id, version),
            |version| Ok(reader.inner().get_object_lt_or_eq_version(&id, version)?),
        )?,
        ObjectSelector::AtCheckpoint(checkpoint) => VersionRead::or_not_found(
            reader.get_object_at_checkpoint(id, checkpoint, upper_bound()?)?,
        ),
    };

    match read {
        VersionRead::Live(object) => Ok(Some(service.render_object_to_proto(
            &object,
            read_mask,
            render_context,
        ))),
        VersionRead::NotFound => Err(selector.not_found(object_id).into()),
        VersionRead::Missing => Ok(None),
    }
}

/// Serve `max_version`. Pinned to a checkpoint (see [`pinned_checkpoint`]), with the request's
/// upper bound, through [`max_version_at_checkpoint`]; otherwise the newest version at or below
/// `max_version` among every version this node holds, or [`VersionRead::Missing`] if none is
/// live.
fn read_max_version(
    max_version: SequenceNumber,
    pinned: Option<(u64, &AtCheckpointUpperBound)>,
    get_object_version_at_checkpoint: impl Fn(
        u64,
        &AtCheckpointUpperBound,
    ) -> Result<Option<SequenceNumber>, RpcError>,
    get_object_by_key: impl Fn(SequenceNumber) -> Option<StoredObject>,
    get_object_lt_or_eq_version: impl FnOnce(SequenceNumber) -> Result<Option<StoredObject>, RpcError>,
) -> Result<VersionRead, RpcError> {
    let Some((checkpoint, upper_bound)) = pinned else {
        return Ok(VersionRead::or_missing(get_object_lt_or_eq_version(
            max_version,
        )?));
    };
    let version_at = |checkpoint| get_object_version_at_checkpoint(checkpoint, upper_bound);
    max_version_at_checkpoint(
        max_version,
        version_at(checkpoint)?,
        || version_at(upper_bound.highest_indexed()),
        get_object_by_key,
        get_object_lt_or_eq_version,
    )
}

/// Serve exactly `version` with `at_checkpoint`. `version_at_checkpoint` is the object's version
/// at the end of the checkpoint (see
/// [`crate::reader::StateReader::get_object_version_at_checkpoint`]); a version above it was
/// written after the checkpoint, and so was every version if there is none.
///
/// The read is refused unless the `at_checkpoint` lower bound accepts the checkpoint, and then
/// the checkpoint's end state is intact. So if `version_at_checkpoint` has no live object, it is
/// a deletion or wrap, or the version the rpc index records for one (see
/// [`newest_version_written_by`]), and the answer is `NOT_FOUND`. A version below it that has no
/// live object may have been pruned.
fn version_at_checkpoint(
    version: SequenceNumber,
    version_at_checkpoint: Option<SequenceNumber>,
    get_object_by_key: impl FnOnce(SequenceNumber) -> Option<StoredObject>,
) -> VersionRead {
    match version_at_checkpoint {
        Some(version_at_checkpoint) if version < version_at_checkpoint => {
            VersionRead::or_missing(get_object_by_key(version))
        }
        Some(version_at_checkpoint) if version == version_at_checkpoint => {
            VersionRead::or_not_found(get_object_by_key(version))
        }
        _ => VersionRead::NotFound,
    }
}

/// Serve `max_version` pinned to a checkpoint: select the object's newest version at or below
/// `max_version` written up to the highest indexed checkpoint H, and answer
/// [`VersionRead::NotFound`] if it was written after the checkpoint, or if it deleted or wrapped
/// the object, or if the object had no version as of the checkpoint. `version_at_checkpoint` is
/// the object's version at the end of the checkpoint (see
/// [`crate::reader::StateReader::get_object_version_at_checkpoint`]), and `version_at_height`
/// reads its version at the end of H, needed only when `max_version` is at or above
/// `version_at_checkpoint`. A version above the object's version at the end of a checkpoint was
/// written after it. [`VersionRead::Missing`] if the object store has no live version at or below
/// `max_version` where the selection needs one, unless the object is live at
/// `version_at_checkpoint`: the read is refused unless the `at_checkpoint` lower bound accepts the
/// checkpoint, and then this node holds that version, so a scan from `max_version` at or above it
/// that finds no live version stopped at a deletion or wrap above it, written after the
/// checkpoint.
fn max_version_at_checkpoint(
    max_version: SequenceNumber,
    version_at_checkpoint: Option<SequenceNumber>,
    version_at_height: impl FnOnce() -> Result<Option<SequenceNumber>, RpcError>,
    get_object_by_key: impl Fn(SequenceNumber) -> Option<StoredObject>,
    get_object_lt_or_eq_version: impl FnOnce(SequenceNumber) -> Result<Option<StoredObject>, RpcError>,
) -> Result<VersionRead, RpcError> {
    let Some(version_at_checkpoint) = version_at_checkpoint else {
        // Every version of the object was written after the checkpoint.
        return Ok(VersionRead::NotFound);
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
        VersionRead::Live(object) if object.version() > version_at_checkpoint => {
            VersionRead::NotFound
        }
        VersionRead::Missing if get_object_by_key(version_at_checkpoint).is_some() => {
            VersionRead::NotFound
        }
        selected => selected,
    })
}

/// The newest version of an object at or below `max_version` among those written at or before a
/// checkpoint, or [`VersionRead::NotFound`] if the object did not exist yet or that version
/// deleted or wrapped it. `version_at_checkpoint` is the object's version at the end of the
/// checkpoint; every version above it was written after the checkpoint.
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
/// which was written after the checkpoint. A scan that finds no live version either stopped at
/// the tombstone or found the version before the removal pruned, so the read is
/// [`VersionRead::Missing`].
fn newest_version_written_by(
    max_version: SequenceNumber,
    version_at_checkpoint: Option<SequenceNumber>,
    get_object_by_key: impl FnOnce(SequenceNumber) -> Option<StoredObject>,
    get_object_lt_or_eq_version: impl FnOnce(SequenceNumber) -> Result<Option<StoredObject>, RpcError>,
) -> Result<VersionRead, RpcError> {
    let Some(version_at_checkpoint) = version_at_checkpoint else {
        return Ok(VersionRead::NotFound);
    };
    if max_version < version_at_checkpoint {
        return Ok(VersionRead::or_missing(get_object_lt_or_eq_version(
            max_version,
        )?));
    }
    if let Some(object) = get_object_by_key(version_at_checkpoint) {
        return Ok(VersionRead::Live(object));
    }
    Ok(match get_object_lt_or_eq_version(max_version)? {
        Some(object) if object.version() < version_at_checkpoint => VersionRead::Live(object),
        Some(_) => VersionRead::NotFound,
        None => VersionRead::Missing,
    })
}

#[cfg(test)]
mod tests {
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

    use sui_rpc::proto::sui::rpc::v2::Object;
    use sui_sdk_types::Address;

    use super::ObjectSelector;
    use super::VersionRead;
    use super::answer_missing_versions;
    use super::max_version_at_checkpoint;
    use super::max_version_upper_bound;
    use super::pinned_checkpoint;
    use super::read_max_version;
    use super::upper_bound_for_requests;
    use super::version_at_checkpoint;
    use crate::RpcError;
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
                |version| objects.get(&version).cloned().flatten(),
                |version| {
                    Ok(objects
                        .range(..=version)
                        .next_back()
                        .and_then(|(_, object)| object.clone()))
                },
            )
            .unwrap();
            match read {
                VersionRead::Live(object) => Ok(object.version().value()),
                VersionRead::NotFound => Err("not found"),
                VersionRead::Missing => Err("missing"),
            }
        };
        // The newest version at or below `max_version` written up to `highest_indexed`, or "not
        // found" if it was written after `checkpoint` or the object had no version as of
        // `checkpoint`, or "missing" if there is no such version. Removals sit at their tombstone
        // versions, not at `indexed_as`. A selected removal is "not found" by the chain rule, but
        // the object store's reads hide tombstones, so the read can only tell it apart from a
        // pruned version when it was written after `checkpoint` and the object is live at the
        // end of `checkpoint`; otherwise either answer (`None`) is accepted, never an object.
        let expected = |max_version: u64, checkpoint: u64, highest_indexed: u64| {
            let Some(at_checkpoint) = lifecycle.iter().rfind(|row| row.checkpoint <= checkpoint)
            else {
                return Some(Err("not found"));
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
    /// whose object's latest live version is `latest_live`.
    fn answer_missing(
        selector: ObjectSelector,
        objects_lower_bound: u64,
        latest_live: Option<u64>,
    ) -> Result<Object, RpcError> {
        let id = Address::ZERO;
        let [result] = answer_missing_versions(
            &[(id, selector)],
            vec![Ok(None)],
            || Ok(objects_lower_bound),
            |_| latest_live.map(SequenceNumber::from_u64),
        )
        .unwrap()
        .try_into()
        .unwrap();
        result
    }

    /// A missing version is `NOT_FOUND` on a node that holds every version, and when it is an
    /// exact version above the object's latest live version. Otherwise a node whose object lower
    /// bound is above 0 may have pruned it, and refuses the read with that lower bound.
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

        // An exact version above the latest live one does not exist yet.
        assert_not_found(answer_missing(version(11), FLOOR, Some(10)));
        assert_below_serving_floor_error(
            answer_missing(version(10), FLOOR, Some(10)).unwrap_err(),
            FLOOR,
        );

        // A node that was neither pruned nor restored holds every version.
        for selector in [version(5), version(11), max_version(5), at_checkpoint] {
            for latest_live in [None, Some(10)] {
                assert_not_found(answer_missing(selector, 0, latest_live));
            }
        }
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
                Some(SequenceNumber::from_u64(10))
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

    /// With `at_checkpoint`, a version written after the checkpoint, a deletion or wrap at the
    /// version the checkpoint ends with, and an object with no version as of the checkpoint are
    /// `NOT_FOUND` whatever the node pruned. Only a version below that one with no live object may
    /// have been pruned.
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
                    held.contains(&version.value())
                        .then(|| object(version.value()))
                },
            ) {
                VersionRead::Live(object) => Ok(object.version().value()),
                VersionRead::NotFound => Err("not found"),
                VersionRead::Missing => Err("missing"),
            };
        // The object is live at 5 at the end of the checkpoint; this node pruned 3.
        assert_eq!(exact(5, Some(5), &[5, 7]), Ok(5));
        assert_eq!(exact(7, Some(5), &[5, 7]), Err("not found"));
        assert_eq!(exact(9, None, &[5, 7]), Err("not found"));
        assert_eq!(exact(3, Some(5), &[5, 7]), Err("missing"));
        // The checkpoint ends with a deletion at 6.
        assert_eq!(exact(6, Some(6), &[5]), Err("not found"));

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
            |version| (version == v(7)).then(|| object(7)),
            |version| Ok((v(7)..v(9)).contains(&version).then(|| object(7))),
        )
        .unwrap()
        {
            VersionRead::Live(object) => Ok(object.version().value()),
            VersionRead::NotFound => Err("not found"),
            VersionRead::Missing => Err("missing"),
        };
        // 7 is selected but was written after checkpoint 2, and the object had no version at 0.
        assert_eq!(max(8, 2), Err("not found"));
        assert_eq!(max(8, 0), Err("not found"));
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

    /// Without `at_checkpoint`, `max_version` on a node with rpc indexes reads at the highest
    /// indexed checkpoint, so a version written above it does not count. On a node without rpc
    /// indexes it counts every version the node holds.
    #[test]
    fn max_version_without_at_checkpoint_is_pinned_to_the_highest_indexed_checkpoint() {
        let id = ObjectID::random();
        let v = SequenceNumber::from_u64;
        // Versions 3 and 5 written at checkpoints 1 and 2, and 7 written at checkpoint 3, which
        // the rpc indexes have not reached.
        let held = [3, 5, 7];
        let object = |version: SequenceNumber| {
            StoredObject::with_id_owner_version_for_testing(
                id,
                version,
                Owner::AddressOwner(SuiAddress::ZERO),
            )
        };
        let unpinned = |max_version: u64| ObjectSelector::MaxVersion {
            max_version,
            at_checkpoint: None,
        };
        let read = |max_version: u64, upper_bound: Option<&AtCheckpointUpperBound>| {
            let pinned = pinned_checkpoint(unpinned(max_version), upper_bound)
                .map(|checkpoint| (checkpoint, upper_bound.unwrap()));
            let read = read_max_version(
                v(max_version),
                pinned,
                |checkpoint, upper_bound: &AtCheckpointUpperBound| {
                    upper_bound.check(checkpoint)?;
                    Ok(match checkpoint {
                        0 => None,
                        1 => Some(v(3)),
                        _ => Some(v(5)),
                    })
                },
                |version| held.contains(&version.value()).then(|| object(version)),
                |version| {
                    Ok(held
                        .iter()
                        .rfind(|&&held| held <= version.value())
                        .map(|&held| object(v(held))))
                },
            )
            .unwrap();
            match read {
                VersionRead::Live(object) => Ok(object.version().value()),
                VersionRead::NotFound => Err("not found"),
                VersionRead::Missing => Err("missing"),
            }
        };

        let indexed = AtCheckpointUpperBound::new(2);
        assert_eq!(pinned_checkpoint(unpinned(8), Some(&indexed)), Some(2));
        assert_eq!(read(8, Some(&indexed)), Ok(5));
        assert_eq!(read(7, Some(&indexed)), Ok(5));
        assert_eq!(read(4, Some(&indexed)), Ok(3));
        assert_eq!(read(2, Some(&indexed)), Err("missing"));

        assert_eq!(pinned_checkpoint(unpinned(8), None), None);
        assert_eq!(read(8, None), Ok(7));
        assert_eq!(read(4, None), Ok(3));

        // An explicit `at_checkpoint` keeps its own checkpoint.
        let pinned_at_1 = ObjectSelector::MaxVersion {
            max_version: 8,
            at_checkpoint: Some(1),
        };
        assert_eq!(pinned_checkpoint(pinned_at_1, Some(&indexed)), Some(1));
    }

    /// While the rpc indexes are backfilling history after a restore, or have not committed a
    /// checkpoint yet, a `max_version` read without `at_checkpoint` counts every version the node
    /// holds rather than fail, alone or in a batch with other reads. A read with `at_checkpoint`
    /// still fails the whole request.
    #[test]
    fn unpinned_max_version_is_served_while_indexes_cannot_pin_it() {
        type UpperBound = Result<Option<AtCheckpointUpperBound>, RpcError>;
        let id = ObjectID::random();
        let backfill_pending = || max_version_upper_bound(|| Ok(true), || panic!("not read"));
        let empty = || max_version_upper_bound(|| Ok(false), || Ok(None));
        let indexed = || max_version_upper_bound(|| Ok(false), || Ok(Some(4)));
        let highest_indexed =
            |bound: UpperBound| bound.unwrap().map(|bound| bound.highest_indexed());
        assert_eq!(highest_indexed(backfill_pending()), None);
        assert_eq!(highest_indexed(empty()), None);
        assert_eq!(highest_indexed(indexed()), Some(4));

        let backfilling = || {
            Err(crate::not_ready(
                crate::NotReadyCause::RestoreBackfill,
                "rpc index history is still backfilling after a restore",
            ))
        };
        let plain_max_version = ObjectSelector::MaxVersion {
            max_version: 8,
            at_checkpoint: None,
        };
        let batch = [
            (Address::ZERO, plain_max_version),
            (Address::ZERO, ObjectSelector::Latest),
            (Address::ZERO, version(3)),
        ];
        let unpinnable: [fn() -> UpperBound; 2] = [backfill_pending, empty];
        for unpinnable in unpinnable {
            let bound = upper_bound_for_requests(&batch, backfilling, unpinnable).unwrap();
            assert!(bound.is_none());
            for (_, selector) in batch {
                assert_eq!(pinned_checkpoint(selector, bound.as_ref()), None);
            }
            // The read counts every version the node holds, without the rpc indexes.
            let read = read_max_version(
                SequenceNumber::from_u64(8),
                None,
                |_, _| panic!("no index read"),
                |_| panic!("no exact read"),
                |version| {
                    assert_eq!(version, SequenceNumber::from_u64(8));
                    Ok(Some(StoredObject::with_id_owner_version_for_testing(
                        id,
                        SequenceNumber::from_u64(7),
                        Owner::AddressOwner(SuiAddress::ZERO),
                    )))
                },
            )
            .unwrap();
            assert!(matches!(read, VersionRead::Live(object) if object.version().value() == 7));
        }
        let bound = upper_bound_for_requests(&batch, backfilling, indexed).unwrap();
        assert_eq!(
            pinned_checkpoint(plain_max_version, bound.as_ref()),
            Some(4)
        );

        // A read with `at_checkpoint` fails the whole request, and pins the others to the bound
        // it loads when that succeeds.
        let with_at_checkpoint = [
            (Address::ZERO, plain_max_version),
            (
                Address::ZERO,
                ObjectSelector::Version {
                    version: 3,
                    at_checkpoint: Some(2),
                },
            ),
        ];
        let error =
            upper_bound_for_requests(&with_at_checkpoint, backfilling, || panic!("not read"))
                .unwrap_err()
                .into_status_proto();
        assert_eq!(error.code, tonic::Code::Unavailable as i32, "{error:?}");
        let bound = upper_bound_for_requests(
            &with_at_checkpoint,
            || Ok(AtCheckpointUpperBound::new(6)),
            || panic!("not read"),
        )
        .unwrap();
        assert_eq!(
            pinned_checkpoint(plain_max_version, bound.as_ref()),
            Some(6)
        );
    }
}

#[cfg(test)]
mod version_without_history_tests {
    use sui_rpc::proto::sui::rpc::v2::get_object_result;
    use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
    use sui_types::object::Owner;

    use super::*;
    use crate::reader::{NotServedHereCause, assert_not_served_here};
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
    /// without one, answers `NotFound` for a version not written yet, and refuses every older
    /// version, including one it still stores.
    #[test]
    fn without_history_only_the_latest_version_is_served() {
        let (service, id) = store_with_two_versions(false);

        assert_eq!(version_of(get_object(&service, request(id, None))), 2);
        assert_eq!(version_of(get_object(&service, request(id, Some(2)))), 2);
        assert_not_served_here(
            get_object(&service, request(id, Some(1))).unwrap_err(),
            NotServedHereCause::NoObjectHistory,
        );
        let error = get_object(&service, request(id, Some(3))).unwrap_err();
        assert_eq!(error.into_status_proto().code, tonic::Code::NotFound as i32);
        let missing = Address::new([2; 32]);
        assert_not_served_here(
            get_object(&service, request(missing, Some(1))).unwrap_err(),
            NotServedHereCause::NoObjectHistory,
        );

        let mut batch = BatchGetObjectsRequest::default();
        batch.requests = vec![request(id, Some(1)), request(id, Some(2))];
        batch.read_mask = Some(FieldMask::from_str("object_id,version"));
        let response = batch_get_objects(&service, batch).unwrap();
        let [first, second] = response.objects.as_slice() else {
            panic!("expected two results in {response:?}");
        };
        assert!(
            matches!(
                &first.result,
                Some(get_object_result::Result::Error(status))
                    if status.code == tonic::Code::Unimplemented as i32
            ),
            "{response:?}"
        );
        assert!(
            matches!(
                &second.result,
                Some(get_object_result::Result::Object(object)) if object.version == Some(2)
            ),
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
}
