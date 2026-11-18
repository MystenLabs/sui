// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::RangeInclusive;

use crate::AtCheckpointUpperBound;
use crate::ErrorReason;
use crate::Result;
use crate::ledger_history::query_options::above_latest_available;
use crate::ledger_history::query_options::validate_checkpoint_bounds;
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
use sui_types::object::OBJECT_START_VERSION;

/// A stream of `ListObjectVersions` response frames.
pub type ObjectVersionFrames = BoxStream<'static, Result<ListObjectVersionsResponse>>;

/// A position in one object's version history, set as `ListObjectVersionsResponse.cursor`
/// and accepted back as `options.after` and `options.before`. Public so other implementors of
/// `LedgerService` mint and accept the same cursors. BCS-encoded as an enum so that a listing in
/// another order can add a variant without changing the encoding of existing cursors.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ObjectVersionCursor {
    /// The item at `version` of the object `object_id`, live or a removal, minted by a listing
    /// bounded by `bounding_checkpoint` (see [`CheckpointWindow::bounding_checkpoint`]). The
    /// client holds the item, so as `after` it lists the versions above `version`, and as
    /// `before` those below it. A server that has indexed less than `bounding_checkpoint` can
    /// tell from it that the cursor comes from a server further ahead, even when the object
    /// leaves it nothing to compare `version` with.
    Version {
        object_id: ObjectID,
        version: u64,
        bounding_checkpoint: u64,
    },
    /// The position between `version` and the version after it, which names no item, minted at
    /// the edge of its window by a listing bounded by `bounding_checkpoint` (see
    /// [`ObjectVersionsQuery::end_cursor`]). As `after` it lists the versions above `version`,
    /// and as `before` those at or below it, so its two sides hold every version, including
    /// versions written after the listing that minted it: resuming from it in either order
    /// neither skips nor repeats one. Object versions start at [`OBJECT_START_VERSION`], so the
    /// boundary at version 0 lies below every version.
    Boundary {
        object_id: ObjectID,
        version: u64,
        bounding_checkpoint: u64,
    },
}

impl ObjectVersionCursor {
    pub fn encode(&self) -> Bytes {
        bcs::to_bytes(self).unwrap().into()
    }
}

/// A cursor a request carries, decoded. A listing that ends at it hands it back as it came (see
/// [`ObjectVersionsQuery::end_cursor`]).
#[derive(Clone, Copy, Debug)]
struct RequestCursor {
    /// The version it names: an item's, or the one below a boundary.
    version: u64,
    /// The checkpoint that bounded the listing that minted it.
    bounding_checkpoint: u64,
    /// Whether it is an [`ObjectVersionCursor::Boundary`] rather than an item's cursor.
    boundary: bool,
}

impl RequestCursor {
    /// The version above those this cursor lists as `before`: an item's own version, or the
    /// version after a boundary. As `after`, either kind lists the versions above `version`.
    fn end_before(&self) -> u64 {
        if self.boundary {
            self.version.saturating_add(1)
        } else {
            self.version
        }
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
    /// `start_checkpoint`: list only the versions written in this checkpoint or later.
    pub start_checkpoint: Option<u64>,
    /// `end_checkpoint`: list only the versions written before this checkpoint.
    pub end_checkpoint: Option<u64>,
    /// Whether to list versions from highest to lowest.
    pub descending: bool,
    /// The most items, live versions and removals, one stream lists.
    pub limit: usize,
    /// `start_version`, the lowest version to list.
    start_version: Option<u64>,
    /// `end_version`, the version above the highest to list.
    end_version: Option<u64>,
    /// The `options.after` cursor.
    after: Option<RequestCursor>,
    /// The `options.before` cursor.
    before: Option<RequestCursor>,
}

/// The checkpoints a listing covers, resolved against the highest checkpoint the server has
/// indexed (see [`ObjectVersionsQuery::checkpoint_window`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckpointWindow {
    /// The checkpoint that bounds the listing, which its cursors record: `end_checkpoint - 1`
    /// when `end_checkpoint` is set and that checkpoint is indexed, else the highest indexed
    /// checkpoint. The listing reads the versions written by the end of it.
    pub bounding_checkpoint: u64,
    /// `start_checkpoint - 1`, capped at [`Self::bounding_checkpoint`]: the listing starts above
    /// the object's version at the end of it. `None` when `start_checkpoint` is unset or 0, and
    /// the listing starts at version 0.
    pub below_start: Option<u64>,
    /// Whether the listing runs to the indexed tip: `end_checkpoint` is unset, or above the
    /// checkpoint after the highest indexed one, or, ascending, `start_checkpoint` is at or above
    /// that checkpoint.
    reaches_tip: bool,
    /// Whether the checkpoint range is empty: `end_checkpoint` is at or below
    /// `start_checkpoint`.
    empty: bool,
}

impl CheckpointWindow {
    /// Whether the checkpoint range is empty, so the listing lists nothing and reads no version
    /// at its checkpoints.
    pub fn is_empty(&self) -> bool {
        self.empty
    }
}

/// The versions a listing reads, and how it ends once it has listed all of them below its limit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectVersionsPlan {
    /// The versions to read, as one inclusive range. `None` if no version is left to list.
    pub versions: Option<RangeInclusive<SequenceNumber>>,
    /// The reason for the final frame of a listing that lists every item in [`Self::versions`]
    /// without reaching the limit.
    pub end: QueryEndReason,
    /// The window's edges, after the cursors: it lists the versions at or above `low` and below
    /// `high`.
    low: u64,
    high: u64,
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
    /// coerced into `1..=max_limit`. A malformed cursor, one for another object, an
    /// `end_version` below `start_version`, or an `end_checkpoint` below `start_checkpoint` is
    /// `INVALID_ARGUMENT`.
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
            .and_then(|options| options.after.as_ref())
            .map(|after| request_cursor("options.after", after, object_id))
            .transpose()?;
        let before = options
            .and_then(|options| options.before.as_ref())
            .map(|before| request_cursor("options.before", before, object_id))
            .transpose()?;
        if let (Some(start), Some(end)) = (request.start_version, request.end_version)
            && end < start
        {
            return Err(FieldViolation::new("end_version")
                .with_description("end_version must be greater than or equal to start_version")
                .with_reason(ErrorReason::FieldInvalid)
                .into());
        }
        validate_checkpoint_bounds(request.start_checkpoint, request.end_checkpoint)?;

        Ok(Self {
            object_id,
            read_mask,
            removal_read_mask,
            start_checkpoint: request.start_checkpoint,
            end_checkpoint: request.end_checkpoint,
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

    /// Fails with `OUT_OF_RANGE` / `ABOVE_LATEST_AVAILABLE` ([`AtCheckpointUpperBound::check_cursor`],
    /// naming the lowest of their checkpoints) when the listing runs to the tip (`end_checkpoint`
    /// unset, or above the checkpoint after the tip) and the request has cursors, all from
    /// listings bounded by a checkpoint above `upper_bound`'s highest indexed one: from servers
    /// that have indexed further than this one. A cursor from a listing bounded at or below it
    /// names a version this server has indexed.
    pub fn check_some_cursor_indexed(
        &self,
        window: &CheckpointWindow,
        upper_bound: &AtCheckpointUpperBound,
    ) -> Result<()> {
        let lowest = [&self.after, &self.before]
            .into_iter()
            .flatten()
            .map(|cursor| cursor.bounding_checkpoint)
            .min();
        match lowest {
            Some(lowest) if window.reaches_tip => upper_bound.check_cursor(lowest),
            _ => Ok(()),
        }
    }

    /// The checkpoints the listing covers, against `upper_bound`'s highest indexed checkpoint,
    /// the tip, as `ListTransactions` bounds them. A descending listing whose `end_checkpoint` is
    /// above the checkpoint after the tip, or whose `start_checkpoint` is above the tip with
    /// `end_checkpoint` unset, is `OUT_OF_RANGE` / `ABOVE_LATEST_AVAILABLE` (bound
    /// `end_checkpoint` or `start_checkpoint`): it would start above versions this server has
    /// not indexed. An ascending listing is never refused for its range: it runs through the tip
    /// and ends with `LEDGER_TIP`.
    pub fn checkpoint_window(
        &self,
        upper_bound: &AtCheckpointUpperBound,
    ) -> Result<CheckpointWindow> {
        let first_unavailable = upper_bound.highest_indexed().saturating_add(1);
        if self.descending {
            let past_tip = match self.end_checkpoint {
                Some(end) if end > first_unavailable => Some(("end_checkpoint", end)),
                Some(_) => None,
                None => self
                    .start_checkpoint
                    .filter(|start| *start >= first_unavailable)
                    .map(|start| ("start_checkpoint", start)),
            };
            if let Some((field, requested)) = past_tip {
                return Err(above_latest_available(field, requested, first_unavailable));
            }
        }
        let indexed_end = self.end_checkpoint.filter(|end| *end <= first_unavailable);
        let bounding_checkpoint =
            indexed_end.map_or(upper_bound.highest_indexed(), |end| end.saturating_sub(1));
        Ok(CheckpointWindow {
            bounding_checkpoint,
            below_start: self
                .start_checkpoint
                .and_then(|start| start.checked_sub(1))
                .map(|below| below.min(bounding_checkpoint)),
            // An ascending range that starts above the tip ends there, even an empty one, as
            // `ListTransactions` ends it.
            reaches_tip: indexed_end.is_none()
                || (!self.descending
                    && self
                        .start_checkpoint
                        .is_some_and(|start| start >= first_unavailable)),
            empty: self
                .end_checkpoint
                .is_some_and(|end| end <= self.start_checkpoint.unwrap_or(0)),
        })
    }

    /// What the listing reads and how it ends, given `newest_visible`, the object's version at
    /// the end of [`CheckpointWindow::bounding_checkpoint`], live or not (the newest version
    /// written by then), or `None` if the object did not exist yet, and `newest_before_start`,
    /// its version at the end of [`CheckpointWindow::below_start`] (`None` when that is unset).
    /// The checkpoint range lists the versions above `newest_before_start` up to
    /// `newest_visible`: an object's versions only increase from checkpoint to checkpoint, so
    /// the versions written in a checkpoint range are one version range. The cursors narrow it
    /// to the versions between their positions (see [`ObjectVersionCursor`]).
    ///
    /// A cursor comes from a listing on any server, such as another replica that has indexed
    /// further, so it can name a version past this server's tip: above `newest_visible`, or at or
    /// above [`OBJECT_START_VERSION`] if it is `None`. When the window runs to the tip
    /// (`end_checkpoint` unset, or above the checkpoint after the tip), the ledger List APIs'
    /// rules apply to such a cursor:
    /// - Descending, it is [`AtCheckpointUpperBound::unindexed_cursor_error`], whatever the
    ///   version range: listing from this server's view would start below versions it lacks. A
    ///   `before` at the version right after `newest_visible` leaves none between them, so it is
    ///   served. With `newest_visible` `None`, that version is [`OBJECT_START_VERSION`], the
    ///   first an object can have, so the boundary at version 0, which a listing of the object
    ///   that lists nothing mints, is served back as either cursor, unless every cursor of the
    ///   request comes from a listing bounded above the tip (see
    ///   [`Self::check_some_cursor_indexed`]).
    /// - Ascending, the listing is served from this server's view, which holds nothing above
    ///   the tip. If its window reaches past the tip (see `LEDGER_TIP` below), an `after` past
    ///   the tip lists nothing and ends with `LEDGER_TIP`, where the client resumes from the
    ///   cursor it holds, with `CURSOR_BOUND` if `before` is at or below the version after
    ///   `after`, leaving nothing between them, or with `CHECKPOINT_BOUND` if `end_version` is.
    ///   A window that ends at or below the tip ends by the rules below. A `before` past the tip
    ///   leaves the tip as the listing's end.
    ///
    /// A listing with an indexed `end_checkpoint` stops at it, so every cursor is safe to page
    /// from. An empty checkpoint range lists nothing. It ends with `CHECKPOINT_BOUND` when
    /// `end_checkpoint - 1` is indexed; ascending above the tip, it ends with `LEDGER_TIP`, as
    /// `ListTransactions` does.
    ///
    /// The listing ends where its range does, at the bound the ledger List APIs would name:
    /// - The far cursor in listing order ends it with `CURSOR_BOUND` when the version it names
    ///   is inside the request's window (`start_version`, `end_version` and the versions written
    ///   in the checkpoint range) or at its edge: `before` at or below the window's end
    ///   ascending, `after` at or above its start descending.
    /// - Otherwise an ascending listing whose window runs to the tip and reaches past the
    ///   object's newest version (`end_version` unset, or above both the version after it and
    ///   `start_version`) ends with `LEDGER_TIP`, since versions written later extend it, even
    ///   when its `after` cursor leaves nothing to list: a client following an object's new
    ///   versions resumes from its newest version's cursor.
    /// - Every other listing ends at the edge of the window the request set, with
    ///   `CHECKPOINT_BOUND`, including one whose starting cursor (`after` ascending, `before`
    ///   descending) sits at or past that edge.
    pub fn plan(
        &self,
        window: &CheckpointWindow,
        upper_bound: &AtCheckpointUpperBound,
        newest_visible: Option<SequenceNumber>,
        newest_before_start: Option<SequenceNumber>,
    ) -> Result<ObjectVersionsPlan> {
        // Exclusive ends throughout: the version after the newest visible one bounds the window.
        // An empty checkpoint range reads neither version, so its window is empty too.
        let visible_end = newest_visible.map_or(0, |newest| newest.value().saturating_add(1));
        let checkpoints_start =
            newest_before_start.map_or(0, |newest| newest.value().saturating_add(1));
        let after = self.after.as_ref().map(|after| after.version);
        let before = self.before.as_ref().map(RequestCursor::end_before);
        // `lowest_missing` is the lowest version a server further ahead may hold that this one
        // lacks: the one after the newest visible version, or, without one, the first version an
        // object can have. A `before` at it leaves no such version between it and the listing.
        // Without a visible version, the boundary at version 0 lies below it, so only the
        // cursors' checkpoints can show that they come from further ahead.
        if window.reaches_tip && self.descending {
            if newest_visible.is_none() {
                self.check_some_cursor_indexed(window, upper_bound)?;
            }
            let lowest_missing = newest_visible.map_or(OBJECT_START_VERSION.value(), |newest| {
                newest.value().saturating_add(1)
            });
            if after.is_some_and(|after| after >= lowest_missing)
                || before.is_some_and(|before| before > lowest_missing)
            {
                return Err(upper_bound.unindexed_cursor_error());
            }
        }
        let reaches_tip = window.reaches_tip
            && self
                .end_version
                .is_none_or(|end| end > visible_end && end > self.start_version.unwrap_or(0));
        if reaches_tip
            && !self.descending
            && let Some(after) = after.filter(|&after| after >= visible_end)
        {
            let resume = after.saturating_add(1);
            // The far cursor ends the listing when it is at least as tight as `end_version`.
            let end = if before.is_some_and(|before| {
                before <= resume && self.end_version.is_none_or(|end| before <= end)
            }) {
                QueryEndReason::CursorBound
            } else if self.end_version.is_some_and(|end| end <= resume) {
                QueryEndReason::CheckpointBound
            } else {
                QueryEndReason::LedgerTip
            };
            return Ok(ObjectVersionsPlan {
                versions: None,
                end,
                low: resume,
                high: visible_end,
            });
        }
        let mut end = if reaches_tip && !self.descending {
            QueryEndReason::LedgerTip
        } else {
            QueryEndReason::CheckpointBound
        };
        let mut low = self
            .start_version
            .map_or(checkpoints_start, |start| start.max(checkpoints_start));
        let mut high = self
            .end_version
            .map_or(visible_end, |end| end.min(visible_end));
        if low < high {
            if let Some(after) = after
                && after >= low
            {
                low = after.saturating_add(1);
                if self.descending {
                    end = QueryEndReason::CursorBound;
                }
            }
            if let Some(before) = before
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
            low,
            high,
        })
    }

    /// The cursor the last frame of a listing that `plan` plans resumes from, when the listing
    /// ends with its planned reason, bounded by `bounding_checkpoint`: a request in the same
    /// order resumes from it.
    /// - `CURSOR_BOUND`: the far cursor (`before` ascending, `after` descending), unchanged.
    /// - The starting cursor (`after` ascending, `before` descending), unchanged, when it sits at
    ///   or past the window's edge, so the listing reached nothing. That includes an ascending
    ///   `after` past the tip.
    /// - Otherwise the window's edge, which is what the listing reached, as an
    ///   [`ObjectVersionCursor::Boundary`]: ascending, the boundary below the window's end, above
    ///   which `after` lists what the window left; descending, the boundary below the window's
    ///   lowest version, at or below which `before` lists, or above `newest_visible` when the
    ///   window lies wholly above it. Versions written later land on the far side of the
    ///   boundary from what the listing reached, whichever order resumes from it.
    pub fn end_cursor(&self, plan: &ObjectVersionsPlan, bounding_checkpoint: u64) -> Bytes {
        let (far, starting) = if self.descending {
            (&self.after, &self.before)
        } else {
            (&self.before, &self.after)
        };
        if plan.end == QueryEndReason::CursorBound
            && let Some(far) = far
        {
            return self.echo(far);
        }
        let reached_nothing = (*starting).filter(|starting| {
            if self.descending {
                starting.end_before() <= plan.low
            } else {
                starting.version.saturating_add(1) >= plan.high
            }
        });
        if let Some(starting) = reached_nothing {
            return self.echo(&starting);
        }
        // A descending window wholly above the newest visible version ends at the boundary above
        // it, a cursor this server accepts back. The boundary at version 0 lies below every
        // version.
        let edge = if self.descending {
            plan.low.min(plan.high)
        } else {
            plan.high
        };
        self.boundary_cursor(edge.saturating_sub(1), bounding_checkpoint)
    }

    /// The cursor of the item at `version`, minted by a listing bounded by `bounding_checkpoint`.
    pub fn cursor(&self, version: u64, bounding_checkpoint: u64) -> Bytes {
        ObjectVersionCursor::Version {
            object_id: self.object_id,
            version,
            bounding_checkpoint,
        }
        .encode()
    }

    /// The cursor of the boundary between `version` and the version after it, minted by a
    /// listing bounded by `bounding_checkpoint`.
    pub fn boundary_cursor(&self, version: u64, bounding_checkpoint: u64) -> Bytes {
        ObjectVersionCursor::Boundary {
            object_id: self.object_id,
            version,
            bounding_checkpoint,
        }
        .encode()
    }

    /// `cursor`, encoded as the request carried it.
    fn echo(&self, cursor: &RequestCursor) -> Bytes {
        if cursor.boundary {
            self.boundary_cursor(cursor.version, cursor.bounding_checkpoint)
        } else {
            self.cursor(cursor.version, cursor.bounding_checkpoint)
        }
    }

    /// The frames of a listing bounded by `bounding_checkpoint` (see
    /// [`CheckpointWindow::bounding_checkpoint`]) whose items `listed` yields in listing order.
    /// Each item goes out on its own frame, whose cursor is the item's: a live version as
    /// `object`, a removal as `removal` with the fields [`Self::removal_read_mask`] selects. The
    /// frame with the [`Self::limit`]th item ends the stream with `ITEM_LIMIT`, whatever `end`
    /// says, and `listed` is not polled again. A `listed` that ends first is followed by a final
    /// frame ending with `end`, whose cursor is `end_cursor` (see [`Self::end_cursor`]), so a
    /// listing with no items still returns a resume cursor. An error `listed` yields ends the
    /// stream with it.
    pub fn frames(
        &self,
        bounding_checkpoint: u64,
        listed: BoxStream<'static, Result<ObjectVersionItem>>,
        end: QueryEndReason,
        end_cursor: Bytes,
    ) -> ObjectVersionFrames {
        let query = self.clone();
        async_stream::try_stream! {
            let mut listed = listed;
            let mut produced = 0;
            while let Some(item) = listed.next().await {
                let item = item?;
                produced += 1;
                let mut frame = ListObjectVersionsResponse::default()
                    .with_cursor(query.cursor(item.version().value(), bounding_checkpoint));
                match item {
                    ObjectVersionItem::Live { object, .. } => frame.set_object(object),
                    ObjectVersionItem::Removal {
                        version,
                        kind,
                        transaction,
                    } => {
                        let removal_read_mask = &query.removal_read_mask;
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
                if produced == query.limit {
                    frame.end = Some(query_end(QueryEndReason::ItemLimit));
                    yield frame;
                    return;
                }
                yield frame;
            }
            yield ListObjectVersionsResponse::default()
                .with_cursor(end_cursor)
                .with_end(query_end(end));
        }
        .boxed()
    }
}

fn query_end(reason: QueryEndReason) -> QueryEnd {
    let mut end = QueryEnd::default();
    end.reason = Some(reason as i32);
    end
}

/// The cursor in request field `field`, which must name a version of the object `object_id`.
fn request_cursor(field: &str, cursor: &Bytes, object_id: ObjectID) -> Result<RequestCursor> {
    let decoded: ObjectVersionCursor = bcs::from_bytes(cursor).map_err(|e| {
        FieldViolation::new(field)
            .with_description(format!("invalid cursor encoding: {e}"))
            .with_reason(ErrorReason::FieldInvalid)
    })?;
    let (cursor_object_id, cursor) = match decoded {
        ObjectVersionCursor::Version {
            object_id,
            version,
            bounding_checkpoint,
        } => (
            object_id,
            RequestCursor {
                version,
                bounding_checkpoint,
                boundary: false,
            },
        ),
        ObjectVersionCursor::Boundary {
            object_id,
            version,
            bounding_checkpoint,
        } => (
            object_id,
            RequestCursor {
                version,
                bounding_checkpoint,
                boundary: true,
            },
        ),
    };
    if cursor_object_id != object_id {
        return Err(FieldViolation::new(field)
            .with_description("cursor object ID does not match request object ID")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }
    Ok(cursor)
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
