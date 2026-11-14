// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::RangeInclusive;

use crate::AtCheckpointUpperBound;
use crate::ErrorReason;
use crate::Result;
use crate::read_mask_defaults;
use bytes::Bytes;
use futures::StreamExt;
use futures::stream::BoxStream;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::ListObjectVersionsRequest;
use sui_rpc::proto::sui::rpc::v2::ListObjectVersionsResponse;
use sui_rpc::proto::sui::rpc::v2::Object;
use sui_rpc::proto::sui::rpc::v2::ObjectRemoval;
use sui_rpc::proto::sui::rpc::v2::Ordering;
use sui_rpc::proto::sui::rpc::v2::QueryEnd;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2::QueryOptions;
use sui_rpc::proto::sui::rpc::v2::object_removal;
use sui_sdk_types::Address;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::digests::TransactionDigest;

/// A stream of `ListObjectVersions` response frames.
pub type ObjectVersionFrames = BoxStream<'static, Result<ListObjectVersionsResponse>>;

/// The position of one listed item, set as `ListObjectVersionsResponse.cursor` and accepted back
/// as `options.after` and `options.before`. Public so other implementors of `LedgerService` mint
/// and accept the same cursors. BCS-encoded as an enum so that a listing in another order can add
/// a variant without changing the encoding of existing cursors.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ObjectVersionCursor {
    /// The item at `version` of the object `object_id`, live or a removal, listed in ascending
    /// version order.
    Version { object_id: ObjectID, version: u64 },
}

impl ObjectVersionCursor {
    pub fn encode(&self) -> Bytes {
        bcs::to_bytes(self).unwrap().into()
    }
}

/// What a `ListObjectVersions` request lists: the object, the fields to render, the order, the
/// limit, and the bounds the request sets. Public so other implementors of `LedgerService` parse
/// requests, bound listings, end them, and mint cursors the same way.
#[derive(Clone, Debug)]
pub struct ObjectVersionsQuery {
    pub object_id: ObjectID,
    /// The fields of each listed live version to render.
    pub read_mask: FieldMaskTree,
    /// The fields of each listed removal to render.
    pub removal_read_mask: FieldMaskTree,
    /// List only the versions written at or before this checkpoint. When unset, servers bound
    /// the listing by the highest checkpoint they have indexed instead.
    pub at_checkpoint: Option<u64>,
    /// Whether to list versions from highest to lowest.
    pub descending: bool,
    /// The most items, live versions and removals, one stream lists.
    pub limit: usize,
    /// `start_version`, the lowest version to list.
    start_version: Option<u64>,
    /// `end_version`, the version above the highest to list.
    end_version: Option<u64>,
    /// The version the `options.after` cursor names.
    after: Option<u64>,
    /// The version the `options.before` cursor names.
    before: Option<u64>,
}

/// The versions a listing reads, and how it ends once it has listed all of them below its limit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectVersionsPlan {
    /// The versions to read, as one inclusive range. `None` if no version is left to list.
    pub versions: Option<RangeInclusive<SequenceNumber>>,
    /// The reason for the final frame of a listing that lists every item in [`Self::versions`]
    /// without reaching the limit.
    pub end: QueryEndReason,
}

/// One item of a listing, as a backend reads it, for [`ObjectVersionsQuery::frames`] to send.
#[derive(Clone, Debug)]
#[allow(
    clippy::large_enum_variant,
    reason = "most items are live versions, each moved once into its frame; boxing them would \
              add an allocation per item to spare the rare removal"
)]
pub enum ObjectVersionItem {
    /// A live version, rendered with [`ObjectVersionsQuery::read_mask`]. `version` is the version
    /// it was rendered from, since the mask may leave `object.version` unset.
    Live {
        object: Object,
        version: SequenceNumber,
    },
    /// A version at which the object was deleted or wrapped.
    Removal {
        version: SequenceNumber,
        kind: ObjectRemovalKind,
        /// The transaction that deleted or wrapped the object: set whenever
        /// [`ObjectVersionsQuery::lists_removal_transactions`], `None` otherwise.
        transaction: Option<TransactionDigest>,
    },
}

impl ObjectVersionItem {
    /// The version this item is at, which its cursor names.
    pub fn version(&self) -> SequenceNumber {
        match self {
            Self::Live { version, .. } | Self::Removal { version, .. } => *version,
        }
    }
}

/// How an object was removed from the live set at a version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectRemovalKind {
    /// Deleted, including unwrapped and deleted by the same transaction. An object ID can be
    /// used again at a higher version, as when a dynamic field is removed and re-added under the
    /// same name, so a deletion need not be the object's last version.
    Deleted,
    /// Wrapped in another object; it can be unwrapped at a higher version.
    Wrapped,
}

impl From<ObjectRemovalKind> for object_removal::Kind {
    fn from(kind: ObjectRemovalKind) -> Self {
        match kind {
            ObjectRemovalKind::Deleted => Self::Deleted,
            ObjectRemovalKind::Wrapped => Self::Wrapped,
        }
    }
}

impl ObjectVersionsQuery {
    /// Parse and validate `request`, with `options.limit` defaulting to `default_limit` and
    /// coerced into `1..=max_limit`. A malformed cursor, one for another object, or an
    /// `end_version` below `start_version` is `INVALID_ARGUMENT`.
    pub fn new(
        request: &ListObjectVersionsRequest,
        default_limit: u32,
        max_limit: u32,
    ) -> Result<Self> {
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

        let read_mask = read_mask_defaults::validate_read_mask::<Object>(
            request.read_mask.clone(),
            read_mask_defaults::OBJECT,
        )?;
        let removal_read_mask = read_mask_defaults::validate_field_mask::<ObjectRemoval>(
            "removal_read_mask",
            request.removal_read_mask.clone(),
            read_mask_defaults::OBJECT_REMOVAL,
        )?;

        let options = request.options.as_ref();
        let after = options
            .and_then(|options| options.after.as_deref())
            .map(|after| cursor_version("options.after", after, object_id))
            .transpose()?;
        let before = options
            .and_then(|options| options.before.as_deref())
            .map(|before| cursor_version("options.before", before, object_id))
            .transpose()?;
        if let (Some(start), Some(end)) = (request.start_version, request.end_version)
            && end < start
        {
            return Err(FieldViolation::new("end_version")
                .with_description("end_version must be greater than or equal to start_version")
                .with_reason(ErrorReason::FieldInvalid)
                .into());
        }

        Ok(Self {
            object_id,
            read_mask,
            removal_read_mask,
            at_checkpoint: request.at_checkpoint,
            descending: descending(options)?,
            limit: options
                .and_then(|options| options.limit)
                .unwrap_or(default_limit)
                .clamp(1, max_limit) as usize,
            start_version: request.start_version,
            end_version: request.end_version,
            after,
            before,
        })
    }

    /// Whether [`Self::removal_read_mask`] selects each removal's `transaction`. A backend that
    /// lists removals sets [`ObjectVersionItem::Removal`]'s `transaction` exactly when it does,
    /// failing the listing when it cannot.
    pub fn lists_removal_transactions(&self) -> bool {
        self.removal_read_mask
            .contains(ObjectRemoval::TRANSACTION_FIELD.name)
    }

    /// The checkpoint that bounds the listing: `at_checkpoint`, or when it is unset,
    /// `upper_bound`'s highest indexed checkpoint. An `at_checkpoint` above that is `OUT_OF_RANGE`
    /// ([`AtCheckpointUpperBound::check`]), whatever the rest of the request.
    pub fn bounding_checkpoint(&self, upper_bound: &AtCheckpointUpperBound) -> Result<u64> {
        match self.at_checkpoint {
            Some(checkpoint) => {
                upper_bound.check(checkpoint)?;
                Ok(checkpoint)
            }
            None => Ok(upper_bound.highest_indexed()),
        }
    }

    /// What the listing reads and how it ends, given `newest_visible`, the object's version at
    /// the end of [`Self::bounding_checkpoint`], live or not (the newest version written by
    /// then), or `None` if the object did not exist yet.
    ///
    /// A cursor comes from a listing on any server, such as another replica that has indexed
    /// further, so it can name a version past this server's tip: above `newest_visible`, or any
    /// version if it is `None`. When `at_checkpoint` is unset, the ledger List APIs' rules apply
    /// to such a cursor:
    /// - Descending, it is [`AtCheckpointUpperBound::unindexed_cursor_error`], whatever the
    ///   version range: listing from this server's view would start below versions it lacks.
    /// - Ascending, the listing is served from this server's view, which holds nothing above
    ///   the tip. If its window reaches past the tip (see `LEDGER_TIP` below), an `after` past
    ///   the tip lists nothing and ends with `LEDGER_TIP`, where the client resumes from the
    ///   cursor it holds, or with `CURSOR_BOUND` if `before` is at or below the version after
    ///   `after`, leaving nothing between them. A window that ends at or below the tip ends by
    ///   the rules below. A `before` past the tip leaves the tip as the listing's end.
    ///
    /// A listing with `at_checkpoint` stops at that indexed checkpoint, so every cursor is safe
    /// to page from.
    ///
    /// The listing ends where its range does, at the bound the ledger List APIs would name:
    /// - The far cursor in listing order ends it with `CURSOR_BOUND` when the version it names
    ///   is inside the request's window (`start_version`, `end_version` and the versions visible
    ///   at the checkpoint) or at its edge: `before` at or below the window's end ascending,
    ///   `after` at or above its start descending.
    /// - Otherwise an ascending listing without `at_checkpoint` whose window reaches past the
    ///   object's newest version (`end_version` unset, or above both the version after it and
    ///   `start_version`) ends with `LEDGER_TIP`, since versions written later extend it, even
    ///   when its `after` cursor leaves nothing to list: a client following an object's new
    ///   versions resumes from its newest version's cursor.
    /// - Every other listing ends at the edge of the window the request set, with
    ///   `CHECKPOINT_BOUND`, including one whose starting cursor (`after` ascending, `before`
    ///   descending) sits at or past that edge.
    pub fn plan(
        &self,
        upper_bound: &AtCheckpointUpperBound,
        newest_visible: Option<SequenceNumber>,
    ) -> Result<ObjectVersionsPlan> {
        // Exclusive ends throughout: the version after the newest visible one bounds the window.
        let visible_end = newest_visible.map_or(0, |newest| newest.value().saturating_add(1));
        if self.at_checkpoint.is_none()
            && self.descending
            && [self.after, self.before]
                .into_iter()
                .flatten()
                .any(|cursor| cursor >= visible_end)
        {
            return Err(upper_bound.unindexed_cursor_error());
        }
        let reaches_tip = self.at_checkpoint.is_none()
            && self
                .end_version
                .is_none_or(|end| end > visible_end && end > self.start_version.unwrap_or(0));
        if reaches_tip
            && !self.descending
            && let Some(after) = self.after.filter(|&after| after >= visible_end)
        {
            let resume = after.saturating_add(1);
            let end = if self.before.is_some_and(|before| before <= resume) {
                QueryEndReason::CursorBound
            } else {
                QueryEndReason::LedgerTip
            };
            return Ok(ObjectVersionsPlan {
                versions: None,
                end,
            });
        }
        let mut end = if reaches_tip && !self.descending {
            QueryEndReason::LedgerTip
        } else {
            QueryEndReason::CheckpointBound
        };
        let mut low = self.start_version.unwrap_or(0);
        let mut high = self
            .end_version
            .map_or(visible_end, |end| end.min(visible_end));
        if low < high {
            if let Some(after) = self.after
                && after >= low
            {
                low = after.saturating_add(1);
                if self.descending {
                    end = QueryEndReason::CursorBound;
                }
            }
            if let Some(before) = self.before
                && before <= high
            {
                high = before;
                if !self.descending {
                    end = QueryEndReason::CursorBound;
                }
            }
        }

        Ok(ObjectVersionsPlan {
            versions: (low < high)
                .then(|| SequenceNumber::from_u64(low)..=SequenceNumber::from_u64(high - 1)),
            end,
        })
    }

    /// The frames of a listing whose items `listed` yields in listing order. Each item goes out
    /// on its own frame with its cursor: a live version as `object`, a removal as `removal` with
    /// the fields [`Self::removal_read_mask`] selects. The frame with the [`Self::limit`]th item
    /// ends the stream with `ITEM_LIMIT`, whatever `end` says, and `listed` is not polled again. A
    /// `listed` that ends first is followed by a final frame ending with `end`'s reason, or by
    /// `end`'s error.
    pub fn frames(
        &self,
        listed: BoxStream<'static, Result<ObjectVersionItem>>,
        end: Result<QueryEndReason>,
    ) -> ObjectVersionFrames {
        let object_id = self.object_id;
        let removal_read_mask = self.removal_read_mask.clone();
        let limit = self.limit;
        async_stream::try_stream! {
            let mut listed = listed;
            let mut produced = 0;
            while let Some(item) = listed.next().await {
                let item = item?;
                produced += 1;
                let cursor = ObjectVersionCursor::Version {
                    object_id,
                    version: item.version().value(),
                };
                let mut frame = ListObjectVersionsResponse::default().with_cursor(cursor.encode());
                match item {
                    ObjectVersionItem::Live { object, .. } => frame.set_object(object),
                    ObjectVersionItem::Removal {
                        version,
                        kind,
                        transaction,
                    } => {
                        let mut removal = ObjectRemoval::default();
                        if removal_read_mask.contains(ObjectRemoval::VERSION_FIELD.name) {
                            removal.set_version(version.value());
                        }
                        if removal_read_mask.contains(ObjectRemoval::KIND_FIELD.name) {
                            removal.set_kind(object_removal::Kind::from(kind));
                        }
                        if removal_read_mask.contains(ObjectRemoval::TRANSACTION_FIELD.name) {
                            removal.transaction = transaction.map(|digest| digest.to_string());
                        }
                        frame.set_removal(removal);
                    }
                }
                if produced == limit {
                    frame.end = Some(query_end(QueryEndReason::ItemLimit));
                    yield frame;
                    return;
                }
                yield frame;
            }
            let reason = end?;
            yield ListObjectVersionsResponse::default().with_end(query_end(reason));
        }
        .boxed()
    }
}

fn query_end(reason: QueryEndReason) -> QueryEnd {
    let mut end = QueryEnd::default();
    end.reason = Some(reason as i32);
    end
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
