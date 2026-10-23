// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Range;
use std::ops::RangeInclusive;

use crate::ErrorReason;
use crate::Result;
use crate::RpcError;
use bytes::Bytes;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::ListObjectVersionsRequest;
use sui_rpc::proto::sui::rpc::v2::ListObjectVersionsResponse;
use sui_rpc::proto::sui::rpc::v2::Object;
use sui_rpc::proto::sui::rpc::v2::ObjectVersion;
use sui_rpc::proto::sui::rpc::v2::Ordering;
use sui_rpc::proto::sui::rpc::v2::QueryOptions;
use sui_sdk_types::Address;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;

/// Versions per response when the request sets no `options.limit`.
const DEFAULT_LIMIT: u32 = 50;
/// Larger `options.limit` values are coerced down to this.
const MAX_LIMIT: u32 = 1000;
const READ_MASK_DEFAULT: &str = crate::read_mask_defaults::OBJECT;

/// The position of one listed version, set as `ObjectVersion.cursor` and accepted back as
/// `options.after` and `options.before`. Public so other implementors of `LedgerService` mint and
/// accept the same cursors. BCS-encoded as an enum so that a listing in another order can add a
/// variant without changing the encoding of existing cursors.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ObjectVersionCursor {
    /// `version` of the object `object_id`, listed in ascending version order.
    Version { object_id: ObjectID, version: u64 },
}

impl ObjectVersionCursor {
    pub fn encode(&self) -> Bytes {
        bcs::to_bytes(self).unwrap().into()
    }
}

/// What a `ListObjectVersions` request lists: the object, the fields to render, and the request's
/// version range and cursors intersected into one version range. Public so other implementors of
/// `LedgerService` parse requests and mint cursors the same way.
#[derive(Clone, Debug)]
pub struct ObjectVersionsQuery {
    pub object_id: ObjectID,
    /// The fields of each listed object to render.
    pub read_mask: FieldMaskTree,
    /// The versions that `start_version`, `end_version`, `options.after` and `options.before`
    /// allow. An end of `u64::MAX` leaves the range unbounded above, since no version reaches it.
    pub versions: Range<u64>,
    /// List only the versions written at or before this checkpoint. When unset, servers bound
    /// the listing by the highest checkpoint they have indexed instead.
    pub at_checkpoint: Option<u64>,
    /// Whether to list versions from highest to lowest.
    pub descending: bool,
    /// The most versions one response lists.
    pub limit: usize,
}

impl ObjectVersionsQuery {
    /// Parse and validate `request`. A malformed cursor, or one for another object, is
    /// `INVALID_ARGUMENT`.
    pub fn new(request: &ListObjectVersionsRequest) -> Result<Self> {
        let object_id = request
            .object_id
            .as_ref()
            .ok_or_else(|| {
                FieldViolation::new("object_id")
                    .with_description("missing object_id")
                    .with_reason(ErrorReason::FieldMissing)
            })?
            .parse::<Address>()
            .map_err(|e| {
                FieldViolation::new("object_id")
                    .with_description(format!("invalid object_id: {e}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?
            .into();

        let read_mask = request
            .read_mask
            .clone()
            .unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
        read_mask.validate::<Object>().map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;

        let options = request.options.as_ref();
        let mut versions =
            request.start_version.unwrap_or(0)..request.end_version.unwrap_or(u64::MAX);
        if let Some(after) = options.and_then(|options| options.after.as_deref()) {
            let after = cursor_version("options.after", after, object_id)?;
            versions.start = versions.start.max(after.saturating_add(1));
        }
        if let Some(before) = options.and_then(|options| options.before.as_deref()) {
            versions.end = versions
                .end
                .min(cursor_version("options.before", before, object_id)?);
        }

        Ok(Self {
            object_id,
            read_mask: FieldMaskTree::from(read_mask),
            versions,
            at_checkpoint: request.at_checkpoint,
            descending: descending(options)?,
            limit: limit(options.and_then(|options| options.limit)),
        })
    }

    /// The versions to read as one inclusive range: [`Self::versions`], limited to versions at
    /// or below `newest_visible`, the object's newest version written by the checkpoint that
    /// bounds the listing. `None` if no version is left to list.
    pub fn version_range(
        &self,
        newest_visible: SequenceNumber,
    ) -> Option<RangeInclusive<SequenceNumber>> {
        let last = self
            .versions
            .end
            .checked_sub(1)?
            .min(newest_visible.value());
        (self.versions.start <= last)
            .then(|| SequenceNumber::from_u64(self.versions.start)..=SequenceNumber::from_u64(last))
    }

    /// One listed version: `object`, rendered from `version` of the listed object, with its
    /// cursor.
    pub fn listed_version(&self, object: Object, version: SequenceNumber) -> ObjectVersion {
        let cursor = ObjectVersionCursor::Version {
            object_id: self.object_id,
            version: version.value(),
        };
        ObjectVersion::default()
            .with_object(object)
            .with_cursor(cursor.encode())
    }
}

/// Fullnodes prune superseded object versions, so a listing would silently omit them; servers
/// that keep object version history, such as archival kv-rpc, serve `ListObjectVersions`.
pub(super) fn list_object_versions(
    _request: ListObjectVersionsRequest,
) -> Result<ListObjectVersionsResponse> {
    Err(RpcError::new(
        tonic::Code::Unimplemented,
        "fullnodes prune object version history; ListObjectVersions is served by archival \
         kv-rpc",
    ))
}

/// The version that the cursor in request field `field` names of the object `object_id`.
fn cursor_version(field: &str, cursor: &[u8], object_id: ObjectID) -> Result<u64> {
    let cursor: ObjectVersionCursor = bcs::from_bytes(cursor).map_err(|e| {
        FieldViolation::new(field)
            .with_description(format!("invalid cursor encoding: {e}"))
            .with_reason(ErrorReason::FieldInvalid)
    })?;
    match cursor {
        ObjectVersionCursor::Version {
            object_id: cursor_object_id,
            version,
        } if cursor_object_id == object_id => Ok(version),
        ObjectVersionCursor::Version { .. } => Err(FieldViolation::new(field)
            .with_description("cursor object ID does not match request object ID")
            .with_reason(ErrorReason::FieldInvalid)
            .into()),
    }
}

/// Whether `options` asks for versions from highest to lowest.
fn descending(options: Option<&QueryOptions>) -> Result<bool> {
    match options
        .and_then(|options| options.ordering)
        .map(Ordering::try_from)
    {
        None | Some(Ok(Ordering::Ascending)) => Ok(false),
        Some(Ok(Ordering::Descending)) => Ok(true),
        Some(_) => Err(FieldViolation::new("options.ordering")
            .with_description("invalid ordering")
            .with_reason(ErrorReason::FieldInvalid)
            .into()),
    }
}

/// The most versions one response lists, for a requested `options.limit`.
fn limit(requested: Option<u32>) -> usize {
    requested.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as usize
}
