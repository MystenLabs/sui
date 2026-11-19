// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::RangeInclusive;

use crate::Result;
use crate::RpcError;
use crate::RpcService;
use crate::grpc::v2::render::RenderContext;
use crate::metrics::ListStreamMetrics;
use bytes::Bytes;
use futures::StreamExt;
use futures::stream::BoxStream;
use sui_rpc::proto::sui::rpc::v2::ListObjectVersionsRequest;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::digests::ObjectDigest;
use sui_types::storage::ObjectOrTombstone;
use tokio_util::sync::CancellationToken;

use super::chunked_scan::ChunkArgs;
use super::chunked_scan::ChunkedScan;
use super::chunked_scan::ScanChunkDone;
use super::chunked_scan::cancelled;
use super::chunked_scan::spawn_list_chunk;
use super::ledger_read::storage_error;
use crate::ledger_history::object_versions::CheckpointWindow;
use crate::ledger_history::object_versions::ObjectRemovalKind;
use crate::ledger_history::object_versions::ObjectVersionFrames;
use crate::ledger_history::object_versions::ObjectVersionItem;
use crate::ledger_history::object_versions::ObjectVersionsPlan;
use crate::ledger_history::object_versions::ObjectVersionsQuery;
use crate::reader::AtCheckpointUpperBound;
use crate::reader::StateReader;
use removals::FoundRemoval;
use removals::removal_transaction;
use retention::Retention;
use retention::read_floor;

mod history;
mod removals;
mod retention;

const METHOD: &str = "list_object_versions";
/// Listings are not told apart by read mask: only the chunk timers are recorded, and they are
/// labeled by method alone.
const RESOLUTION: &str = "all";

/// Serve `ListObjectVersions` from this node's object store, within its retention window. Every
/// listing is bounded by a checkpoint the rpc indexes have committed: `end_checkpoint - 1`, or the
/// highest indexed checkpoint when `end_checkpoint` is unset or above it (see
/// [`ObjectVersionsQuery::checkpoint_window`]). The object's versions at the end of that
/// checkpoint and of the one before `start_checkpoint`, the request's version range, and its
/// cursors fold into one inclusive version range (see [`ObjectVersionsQuery::plan`]): the
/// listing covers the history written in the checkpoint range. Its items, live versions and the
/// tombstones at which the object was deleted or wrapped, are read in chunks of blocking reads
/// ([`ChunkedScan`]), so a slow client holds neither an iterator nor a blocking thread.
///
/// Pruning removes the versions an object replaced, and the tombstones it left, at or before the
/// end of the pruned floor checkpoint F ([`retention::read_floor`]), keeping its live version at
/// the end of it. Its oldest retained version is that version, or for an object with none, the
/// first item after the floor, live or a removal, that the node places there (see
/// [`retention::first_retained_version_after_removal`]). The lowest `start_checkpoint` the node
/// accepts is F + 1, since a range starts above the object's version at the end of the
/// checkpoint before it: a lower `start_checkpoint`, or an `end_checkpoint` at or below F + 1 in
/// a listing with no lower bound of its own, fails with `BELOW_EARLIEST_AVAILABLE` (see
/// [`ObjectVersionsQuery::check_checkpoint_floor`]). A listing bounded by a checkpoint below the
/// object floor reads no version there: when its `after` cursor is at or above the object's
/// version at the floor, the cursors leave it nothing to list and it is served empty, as
/// `ListTransactions` serves one, and otherwise it fails the same way. Every
/// `BELOW_EARLIEST_AVAILABLE` error of this method reports F + 1 as `lowestAvailableCheckpoint`,
/// the lowest `start_checkpoint` it accepts, while point reads report F, the lowest
/// `at_checkpoint` they accept. A request whose `after` cursor comes from a server that has
/// indexed further than this node gets that cursor's `ABOVE_LATEST_AVAILABLE` instead of any of
/// these refusals, as `ListTransactions` puts that cursor's tip refusal first (see
/// [`ObjectVersionsQuery::floor_error`]).
///
/// Without `start_checkpoint`, a listing covers only the versions the node retains: it starts at
/// the oldest retained version when its range reaches below it, and lists nothing for an object
/// the node retains nothing of, so a descending listing that resumes from a `before` cursor below
/// the oldest retained version gets an empty page that ends normally. A descending listing that
/// stops at the oldest retained version ends with a cursor at it, and one that pruning cuts short
/// while it lists ends where it stopped (see [`CutShort`]), unless it ends at its far cursor,
/// which it hands back. When it retains nothing of the
/// object, a request whose listing runs to the tip and whose cursors all come from servers that
/// have indexed further than this node fails with `ABOVE_LATEST_AVAILABLE`, in either order,
/// until it catches up (see [`ObjectVersionsQuery::check_some_cursor_indexed`]), since the node
/// cannot tell an object newer than its tip from one it pruned. Items written at or before the
/// floor checkpoint that pruning has not deleted yet are passed over, since the versions between
/// them and the oldest retained one may be gone. A listing fails with `BELOW_EARLIEST_AVAILABLE`
/// instead when the client already holds history right below the versions it lists, which it
/// would otherwise follow with a gap: below an `after` cursor, in either order, that names an
/// item, that is a boundary from a listing bounded below the floor, including one pruning passes
/// while the listing runs (see [`ObjectVersionsQuery::has_held_after_cursor`]), or that is a
/// boundary above version 0 when the object's version at the floor is above the version after it
/// (see [`ObjectVersionsQuery::needs_history_retained_from`]), or an item an earlier chunk of an
/// ascending listing sent. So does a request with an item's or such a passed boundary's `after`
/// cursor for an object of which the node retains nothing, and a listing from `start_checkpoint`
/// that pruning overtakes, which covers its checkpoint range whole or fails, as
/// `ListTransactions` does. Pruning that overtakes any other listing only trims what it lists.
///
/// A listing whose removal mask selects `transaction` finds each removal's transaction (see
/// [`removals::removal_transaction`], and the notice in [`removals`] on the index that would
/// replace it), and so has the higher floor of the object floor and the last checkpoint whose
/// transaction data is pruned; above it, a transaction it cannot find is `INTERNAL`. Without
/// `transaction`, removals are listed without looking for their transactions.
///
/// Nodes without rpc indexes answer `UNIMPLEMENTED` (`NOT_SERVED_HERE`); nodes whose indexes have
/// not committed a checkpoint yet, or are still backfilling history after a restore, answer
/// `UNAVAILABLE` (`NOT_READY`) (see [`crate::reader::StateReader::at_checkpoint_upper_bound`]).
pub(super) async fn list_object_versions(
    service: RpcService,
    request: ListObjectVersionsRequest,
) -> Result<ObjectVersionFrames> {
    let endpoint = service.config.ledger_history().list_object_versions();
    let query = ObjectVersionsQuery::new(
        &request,
        endpoint.default_limit_items,
        endpoint.max_limit_items,
    )?;
    let chunk_metrics = service
        .list_metrics
        .as_ref()
        .map(|metrics| metrics.stream_metrics(METHOD, RESOLUTION));
    // Planning runs on a blocking thread; dropping the request cancels it.
    let planning_cancel = CancellationToken::new();
    let _planning_cancel_guard = planning_cancel.clone().drop_guard();
    let listing = spawn_list_chunk(chunk_metrics.clone(), {
        let service = service.clone();
        let query = query.clone();
        move |_| Listing::plan(&service.reader, &query, &planning_cancel)
    })
    .await
    .map_err(storage_error)??;
    // Tests pause here to prune between planning a listing and reading its first chunk.
    sui_macros::fail_point_async!("list-object-versions-after-plan");

    let (cut_short_tx, cut_short) = tokio::sync::oneshot::channel();
    let listed = list_items(
        service,
        query.clone(),
        listing.initial_state,
        endpoint.chunk_max,
        chunk_metrics,
        cut_short_tx,
    );
    let bounding_checkpoint = listing.bounding_checkpoint;
    // A listing that ends at its far cursor hands that back, wherever pruning cut it short: the
    // checks that admitted the cut found its client holding what pruning took.
    let ends_at_far_cursor = listing.end == QueryEndReason::CursorBound;
    let end_cursor = {
        let query = query.clone();
        let planned = listing.end_cursor;
        async move {
            match cut_short.await {
                Ok(Some(stop)) if !ends_at_far_cursor => stop.cursor(&query, bounding_checkpoint),
                _ => planned,
            }
        }
    };
    Ok(query.frames(bounding_checkpoint, listed, listing.end, end_cursor))
}

/// What a fullnode listing reads and how it ends, resolved before it lists anything.
#[derive(Debug)]
struct Listing {
    /// The checkpoint that bounds the listing (see
    /// [`crate::ledger_history::object_versions::CheckpointWindow::bounding_checkpoint`]).
    bounding_checkpoint: u64,
    /// The cursor the listing's last frame resumes from, when it ends below its limit.
    end_cursor: Bytes,
    /// Where the first chunk starts reading: the retained versions to read, or `None` if none
    /// are left to list.
    initial_state: Option<ObjectVersionScanState>,
    /// How the listing ends once it has read every item it lists below its limit.
    end: QueryEndReason,
}

impl Listing {
    fn plan(
        reader: &StateReader,
        query: &ObjectVersionsQuery,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        let upper_bound = reader.at_checkpoint_upper_bound()?;
        let window = query.checkpoint_window(&upper_bound)?;
        let checkpoint = window.bounding_checkpoint;
        // An empty checkpoint range lists nothing, whatever the floor, so it reads no version at
        // its checkpoints.
        if window.is_empty() {
            let plan = query.plan(&window, &upper_bound, None, None)?;
            return Ok(Self::reading_nothing(query, &plan, checkpoint));
        }
        let lists_transactions = query.lists_removal_transactions();
        let version_at = |checkpoint| {
            let indexed = reader.get_object_version_at_checkpoint(
                query.object_id,
                checkpoint,
                &upper_bound,
            )?;
            stored_version_of_indexed(reader, query.object_id, indexed)
        };
        let floors = read_floor(reader, lists_transactions)?;
        query.check_checkpoint_floor(floors.floor_checkpoint, &upper_bound, version_at)?;
        if let Some(listing) = Self::bounded_below_object_floor(
            reader,
            query,
            &window,
            &upper_bound,
            floors.object_floor,
            lists_transactions,
            version_at,
        )? {
            return Ok(listing);
        }
        let newest_at_tip = version_at(upper_bound.highest_indexed())?;
        let newest_visible = if checkpoint == upper_bound.highest_indexed() {
            newest_at_tip
        } else {
            version_at(checkpoint)?
        };
        let newest_before_start = match window.below_start {
            Some(below_start) => version_at(below_start)?,
            None => None,
        };
        // Nothing of this object is retained up to the listing's checkpoint. On a node that has
        // pruned objects, it never existed, or was deleted or wrapped at or before the floor and
        // pruning dropped its rows, or its history starts above that checkpoint, and the node
        // cannot tell which. A request whose listing runs to the tip and whose cursors all come
        // from servers that have indexed further may hold versions this node has not indexed
        // yet, or ones it pruned, so it fails with `ABOVE_LATEST_AVAILABLE`, in either order,
        // until this node catches up. A request whose `after` cursor holds history that pruning
        // may have cut a gap after (see `ObjectVersionsQuery::has_held_after_cursor`) then fails
        // with the floor error: a removal the node no longer has rows for may follow it. Any
        // other lists nothing, and a descending one ends as at the object's first version, even
        // from a `before` cursor the plan would take for one past the tip. The floor is read
        // after the version: pruning raises it no later than it drops rows.
        let nothing_retained_floor = match newest_visible {
            None if reader.at_checkpoint_lower_bound()?.pruned_floor().is_some() => {
                read_floor(reader, lists_transactions)?.floor_checkpoint
            }
            _ => None,
        };
        if nothing_retained_floor.is_some() {
            query.check_some_cursor_indexed(&window, &upper_bound)?;
        }
        let plan = match nothing_retained_floor {
            Some(floor_checkpoint) if query.has_held_after_cursor(Some(floor_checkpoint)) => {
                return Err(query.floor_error(floor_checkpoint, &upper_bound));
            }
            Some(_) if query.descending => query.nothing_retained(),
            _ => query.plan(&window, &upper_bound, newest_visible, newest_before_start)?,
        };
        // After the reads: pruning may have raised the floor past the checkpoints they read at
        // while they ran (see `StateReader::at_checkpoint_lower_bound`).
        let floors = read_floor(reader, lists_transactions)?;
        query.check_checkpoint_floor(floors.floor_checkpoint, &upper_bound, version_at)?;
        if let Some(listing) = Self::bounded_below_object_floor(
            reader,
            query,
            &window,
            &upper_bound,
            floors.object_floor,
            lists_transactions,
            version_at,
        )? {
            return Ok(listing);
        }
        // An object with a version at the listing's checkpoint has one at the tip too.
        let (Some(versions), Some(newest_version)) = (plan.versions.clone(), newest_at_tip) else {
            return Ok(Self::reading_nothing(query, &plan, checkpoint));
        };
        let tip = IndexedTip {
            newest_version,
            upper_bound,
        };
        let retention = Retention::read(
            reader,
            query.object_id,
            &tip,
            floors.floor_checkpoint,
            lists_transactions,
            cancel,
        )?;
        // The floor `Retention::read` settled on, which pruning may have raised since the check
        // above, bounds a listing from `start_checkpoint` too.
        if retention.floor_checkpoint != floors.floor_checkpoint {
            query.check_checkpoint_floor(retention.floor_checkpoint, &upper_bound, version_at)?;
        }
        let start = *versions.start();
        let client_holds_item_below =
            query.needs_history_retained_from(start, retention.floor_checkpoint);
        // A `start_checkpoint` below the lowest one this floor accepts passed
        // `check_checkpoint_floor` only on its `after` cursor, which leaves the object's history
        // from `start` unvouched for when the object has no version at the floor: it may have
        // been removed at or before it, its later versions pruned. Unless the cursor bounds the
        // listing, which starts right after it, and is a boundary bounded at or above the floor
        // (see `ObjectVersionsQuery::has_held_after_cursor`; a boundary above version 0 has
        // already passed `check_checkpoint_floor`'s check of the object's version at the floor),
        // that history must then be retained, as for a client holding the item right below it.
        let start_below_floor = retention
            .lowest_start_checkpoint()
            .zip(query.start_checkpoint)
            .is_some_and(|(lowest_start, start_checkpoint)| start_checkpoint < lowest_start);
        if client_holds_item_below
            || (start_below_floor
                && (query.has_held_after_cursor(retention.floor_checkpoint)
                    || !query.starts_after_cursor(start)))
        {
            retention.check_keeps_from(start, query, &upper_bound)?;
        }
        let read_from = retention
            .first_kept_from(start)
            .filter(|read_from| read_from <= versions.end());
        // A listing whose range reaches below the oldest retained version starts there, and a
        // descending one that would end at its window's edge stops there and resumes from it. One
        // that ends at its far cursor hands that cursor back.
        let stops_at_oldest = read_from.filter(|&read_from| {
            query.descending && plan.end != QueryEndReason::CursorBound && read_from > start
        });
        Ok(Self {
            bounding_checkpoint: checkpoint,
            end_cursor: match stops_at_oldest {
                Some(oldest) => query.cursor(oldest.value(), checkpoint),
                None => query.end_cursor(&plan, checkpoint),
            },
            initial_state: read_from.map(|read_from| ObjectVersionScanState {
                versions: read_from..=*versions.end(),
                retention,
                tip,
                previous_removal: None,
                client_holds_item_below,
                after_cursor_below: query.starts_after_cursor(start),
            }),
            end: plan.end,
        })
    }

    /// A listing of `plan`, bounded by `bounding_checkpoint`, that reads nothing.
    fn reading_nothing(
        query: &ObjectVersionsQuery,
        plan: &ObjectVersionsPlan,
        bounding_checkpoint: u64,
    ) -> Self {
        Self {
            bounding_checkpoint,
            end_cursor: query.end_cursor(plan, bounding_checkpoint),
            initial_state: None,
            end: plan.end,
        }
    }

    /// The listing of a request whose window's bounding checkpoint is below `object_floor`, the
    /// object floor as the caller last read it (see [`retention::Floors`]), or `None` when it is
    /// at or above it or nothing is pruned. The object's versions at checkpoints below the floor
    /// may be pruned, so its window is not read, and `check_checkpoint_floor` refuses every such
    /// request without an `after` cursor. An `after` cursor at or above the object's version at
    /// the end of the floor checkpoint is at or above every version written by the bounding
    /// checkpoint, so the cursors leave nothing to list, and the listing is served empty, as
    /// `ListTransactions` serves a scan its cursors alone leave empty: it is planned as if the
    /// object's newest version by then were its version at the floor, which the cursor passes.
    /// Any other request fails with the floor error, since this node cannot tell what its window
    /// holds.
    fn bounded_below_object_floor(
        reader: &StateReader,
        query: &ObjectVersionsQuery,
        window: &CheckpointWindow,
        upper_bound: &AtCheckpointUpperBound,
        object_floor: Option<u64>,
        lists_transactions: bool,
        version_at: impl FnOnce(u64) -> Result<Option<SequenceNumber>>,
    ) -> Result<Option<Self>> {
        let Some(object_floor) =
            object_floor.filter(|&object_floor| window.bounding_checkpoint < object_floor)
        else {
            return Ok(None);
        };
        let at_floor = version_at(object_floor)?;
        if !query.after_at_or_above(at_floor) {
            let floor_checkpoint = read_floor(reader, lists_transactions)?
                .floor_checkpoint
                .unwrap_or(object_floor);
            return Err(query.floor_error(floor_checkpoint, upper_bound));
        }
        let plan = query.plan(window, upper_bound, at_floor, None)?;
        Ok(Some(Self::reading_nothing(
            query,
            &plan,
            window.bounding_checkpoint,
        )))
    }
}

/// The version in the object store of the item the version index records at `indexed` (see
/// [`stored_version_at`]).
pub(super) fn stored_version_of_indexed(
    reader: &StateReader,
    object_id: ObjectID,
    indexed: Option<SequenceNumber>,
) -> Result<Option<SequenceNumber>> {
    let Some(indexed) = indexed else {
        return Ok(None);
    };
    let first_stored = reader
        .object_versions_iter(object_id, indexed..=SequenceNumber::MAX, false)?
        .next()
        .transpose()?;
    Ok(Some(stored_version_at(indexed, first_stored.as_ref())))
}

/// The version in the object store of the item the version index records at `indexed`, given
/// the object's first stored item at or above `indexed`. They differ for a deletion or wrap by a
/// transaction with V1 effects: the index records `TransactionEffectsAPI::lamport_version`,
/// which V1 effects recompute from the modified objects alone, while the tombstone sits at the
/// version the transaction executed at, which also counts its read-only inputs. No item lies
/// between the two, so when the store holds nothing at `indexed`, a tombstone right above it is
/// that removal.
fn stored_version_at(
    indexed: SequenceNumber,
    first_stored: Option<&ObjectOrTombstone>,
) -> SequenceNumber {
    match first_stored {
        Some(ObjectOrTombstone::Tombstone((_, version, _))) if *version > indexed => *version,
        _ => indexed,
    }
}

/// The highest checkpoint the indexes had committed when the listing was planned, and the
/// object's newest version at the end of it. Every read that places an item by the version
/// index, or finds the transaction behind a removal, is bounded by them, so it reads only what
/// the indexes have committed.
#[derive(Clone, Copy, Debug)]
struct IndexedTip {
    newest_version: SequenceNumber,
    upper_bound: AtCheckpointUpperBound,
}

/// How the object was removed at the tombstone whose digest is `digest`.
pub(super) fn removal_kind(digest: &ObjectDigest) -> Result<ObjectRemovalKind> {
    if digest.is_deleted() {
        Ok(ObjectRemovalKind::Deleted)
    } else if digest.is_wrapped() {
        Ok(ObjectRemovalKind::Wrapped)
    } else {
        Err(storage_error(format!(
            "tombstone with unexpected digest {digest}"
        )))
    }
}

pub(super) fn stored_version(item: &ObjectOrTombstone) -> SequenceNumber {
    match item {
        ObjectOrTombstone::Object(object) => object.version(),
        ObjectOrTombstone::Tombstone((_, version, _)) => *version,
    }
}

/// One chunk worker's input: the versions left to read, and what bounds them.
#[derive(Debug)]
struct ObjectVersionScanState {
    versions: RangeInclusive<SequenceNumber>,
    retention: Retention,
    tip: IndexedTip,
    /// The removal the listing found last, which the lookup for the next tombstone in listing
    /// order starts from when the two are next to each other (see [`removal_transaction`]).
    previous_removal: Option<FoundRemoval>,
    /// Whether the client holds history right below `versions` that pruning through the floor
    /// the listing was planned under may have cut a gap after: below the `after` cursor the
    /// listing starts right after (see [`ObjectVersionsQuery::needs_history_retained_from`]), or
    /// an item an earlier chunk of an ascending listing sent. Pruning that deletes versions
    /// between it and `versions` fails the listing, which would otherwise skip them.
    client_holds_item_below: bool,
    /// Whether `versions` starts right after the request's `after` cursor, of either kind (see
    /// [`ObjectVersionsQuery::starts_after_cursor`]): for every chunk of a descending listing,
    /// and until it sends an item for an ascending one. Pruning that later passes a boundary
    /// cursor's checkpoint makes its client hold history right below `versions` too.
    after_cursor_below: bool,
}

/// Where a descending listing that pruning cut short while it listed stops, which its last
/// frame's cursor names in place of the planned one, except when the listing ends with
/// `CURSOR_BOUND`, whose far cursor stays: the listing sent every version it reached above that
/// point and none at or below it, so a client resuming from it, on this node or on one that has
/// pruned less, neither skips nor repeats a version.
#[derive(Clone, Copy, Debug)]
enum CutShort {
    /// The lowest item the listing sent, which the chunk that found the cut sent.
    Item(SequenceNumber),
    /// The boundary right above this version, the highest of the versions the chunk that found
    /// the cut read: that chunk sent nothing, and earlier chunks sent every item above them.
    Boundary(SequenceNumber),
}

impl CutShort {
    /// The cursor of this point, minted by a listing bounded by `bounding_checkpoint`.
    fn cursor(self, query: &ObjectVersionsQuery, bounding_checkpoint: u64) -> Bytes {
        match self {
            Self::Item(version) => query.cursor(version.value(), bounding_checkpoint),
            Self::Boundary(version) => query.boundary_cursor(version.value(), bounding_checkpoint),
        }
    }
}

/// The items of the listing whose first chunk reads `initial_state`, rendered, in listing order,
/// up to the query's limit. Once they end, `cut_short` receives where pruning cut the listing
/// short, if it did.
fn list_items(
    service: RpcService,
    query: ObjectVersionsQuery,
    initial_state: Option<ObjectVersionScanState>,
    chunk_max: usize,
    chunk_metrics: Option<ListStreamMetrics>,
    cut_short: tokio::sync::oneshot::Sender<Option<CutShort>>,
) -> BoxStream<'static, Result<ObjectVersionItem>> {
    let limit = query.limit;
    async_stream::try_stream! {
        if let Some(initial_state) = initial_state {
            let mut scan = ChunkedScan::new(initial_state, limit, chunk_max, 0, move |state, args| {
                let service = service.clone();
                let query = query.clone();
                spawn_list_chunk(chunk_metrics.clone(), move |_| {
                    next_object_version_chunk(&service, &query, state, args)
                })
            });
            while let Some(item) = scan.next_item().await? {
                yield item;
            }
            // Sending fails only once the frames, which hold the receiver, are dropped.
            let _ = cut_short.send(scan.into_terminal().flatten());
        }
    }
    .boxed()
}

/// A chunk's result. Its terminal is where pruning cut a descending listing short, set by the
/// chunk that found the cut, which is the listing's last.
type ObjectVersionChunkDone =
    ScanChunkDone<ObjectVersionScanState, ObjectVersionItem, Option<CutShort>>;

/// Read and render up to `args.chunk_item_limit` items from `state.versions` in listing order,
/// finding the transaction behind each removal when the query lists them, then check the
/// retention floor against everything the chunk read, the items and the removal lookups alike.
/// Pruning that has raised the object's oldest retained version above the versions read fails
/// the listing if the client holds history right below them (see
/// [`ObjectVersionScanState::client_holds_item_below`] and
/// [`ObjectVersionScanState::after_cursor_below`]) or the listing is from `start_checkpoint`, and
/// otherwise leaves only the items at or above it: a descending listing reads no further and ends
/// where it stopped (see [`CutShort`]), and an ascending one goes on.
/// A lookup can miss a removal the floor still keeps when it walked from a removal pruning has
/// since taken, or through rows pruning deleted; when the floor moved, the chunk lists nothing
/// and leaves the same versions for the next chunk to read under the new floor. Fails with
/// `CANCELLED` once the chunk is cancelled, and with `INTERNAL` for a retained removal whose
/// transaction cannot be found under a floor that held still.
fn next_object_version_chunk(
    service: &RpcService,
    query: &ObjectVersionsQuery,
    state: ObjectVersionScanState,
    args: ChunkArgs,
) -> Result<ObjectVersionChunkDone> {
    let reader = &service.reader;
    let limit = args.chunk_item_limit;
    let cancel = &args.cancel;
    let render_context = RenderContext::empty(reader.inner());
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    let ObjectVersionScanState {
        versions,
        retention,
        tip,
        mut previous_removal,
        client_holds_item_below,
        after_cursor_below,
    } = state;
    let stored = reader
        .object_versions_iter(query.object_id, versions.clone(), query.descending)?
        .take(limit)
        .collect::<Result<Vec<_>>>()?;

    // A full chunk may leave versions past its last one; a short one read the whole range.
    let last_read = stored
        .last()
        .filter(|_| stored.len() == limit)
        .map(stored_version);
    let lowest_read = match last_read {
        Some(last) if query.descending => last,
        _ => *versions.start(),
    };

    let mut items = Vec::with_capacity(stored.len());
    for item in stored {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let item = match item {
            ObjectOrTombstone::Object(object) => ObjectVersionItem::Live {
                object: service.render_object_to_proto(&object, &query.read_mask, &render_context),
                version: object.version(),
            },
            ObjectOrTombstone::Tombstone((_, version, digest)) => {
                let kind = removal_kind(&digest)?;
                let transaction = if query.lists_removal_transactions() {
                    // Tests pause here to prune between two removal lookups of one chunk.
                    sui_macros::fail_point!("list-object-versions-before-removal-lookup");
                    previous_removal = removal_transaction(
                        reader,
                        query.object_id,
                        version,
                        previous_removal,
                        retention.floor_checkpoint,
                        &tip,
                        cancel,
                    )?
                    .map(|transaction| FoundRemoval {
                        version,
                        transaction,
                    });
                    previous_removal.map(|found| found.transaction.digest)
                } else {
                    None
                };
                ObjectVersionItem::Removal {
                    version,
                    kind,
                    transaction,
                }
            }
        };
        items.push(item);
    }

    // The floor, read after every read of the chunk: pruning raises it no later than it deletes
    // versions or the transactions behind them, so whatever this read keeps was read whole,
    // unless a lookup missed.
    let reread = retention.reread(reader, query.object_id, &tip, cancel)?;
    // With transactions listed, a removal without one is a lookup that missed.
    let missing = items.iter().find_map(|item| match item {
        ObjectVersionItem::Removal {
            version,
            transaction: None,
            ..
        } if query.lists_removal_transactions() && reread.keeps_from(*version) => Some(*version),
        _ => None,
    });
    if let Some(missing) = missing {
        // Checkpoint pruning deletes effects before it raises the floor `read_floor` reads, so a
        // prune racing a lookup can still end the listing here.
        if reread.floor_checkpoint == retention.floor_checkpoint {
            return Err(RpcError::new(
                tonic::Code::Internal,
                format!(
                    "missing the transaction that removed object {} at version {}, above the \
                     retention floor",
                    query.object_id,
                    missing.value()
                ),
            ));
        }
        return Ok(ScanChunkDone {
            produced: 0,
            next_state: Some(ObjectVersionScanState {
                versions,
                retention: reread,
                tip,
                previous_removal: None,
                client_holds_item_below,
                after_cursor_below,
            }),
            items: Vec::new(),
            terminal: None,
            remaining_scan_budget: args.scan_budget,
        });
    }
    let retention = reread;
    // Set when pruning has raised the oldest retained version above versions this chunk read,
    // which ends a descending listing.
    let overtaken = !retention.keeps_from(lowest_read);
    if overtaken {
        // A boundary `after` cursor that pruning has passed since the listing was planned now
        // leaves a gap below `versions` too.
        let client_holds_history_below = client_holds_item_below
            || (after_cursor_below && query.has_held_after_cursor(retention.floor_checkpoint));
        if client_holds_history_below || query.start_checkpoint.is_some() {
            retention.check_keeps_from(lowest_read, query, &tip.upper_bound)?;
        }
        items.retain(|item| retention.keeps_from(item.version()));
    }
    let produced = items.len();
    let cut_short = (overtaken && query.descending).then(|| match items.last() {
        Some(lowest_sent) => CutShort::Item(lowest_sent.version()),
        None => CutShort::Boundary(*versions.end()),
    });

    // A descending listing that pruning overtook has read every item it keeps. An ascending one
    // goes on from its last read, above the versions pruning deleted.
    let remaining = last_read.filter(|_| cut_short.is_none()).and_then(|last| {
        if query.descending {
            let below = last.one_before()?;
            (*versions.start() <= below).then(|| *versions.start()..=below)
        } else {
            let above = SequenceNumber::from_u64(last.value().checked_add(1)?);
            (above <= *versions.end()).then(|| above..=*versions.end())
        }
    });
    Ok(ScanChunkDone {
        produced,
        next_state: remaining.map(|versions| ObjectVersionScanState {
            versions,
            // A lookup next to a removal the floor no longer keeps could walk through rows
            // pruning deleted.
            previous_removal: previous_removal.filter(|found| retention.keeps_from(found.version)),
            retention,
            tip,
            client_holds_item_below: client_holds_item_below || (!query.descending && produced > 0),
            after_cursor_below: after_cursor_below && (query.descending || produced == 0),
        }),
        items,
        terminal: cut_short,
        remaining_scan_budget: args.scan_budget,
    })
}

#[cfg(test)]
mod tests {
    use sui_types::object::Object as StoredObject;
    use sui_types::object::Owner;

    use super::*;

    /// A V1 removal indexed below its tombstone resolves to the tombstone. Anything else at or
    /// above the indexed version, or nothing, leaves the indexed version as it is.
    #[test]
    fn removal_indexed_below_its_tombstone_resolves_to_it() {
        let id = ObjectID::from_single_byte(7);
        let version = SequenceNumber::from_u64;
        let tombstone = |at| {
            ObjectOrTombstone::Tombstone((id, version(at), ObjectDigest::OBJECT_DIGEST_DELETED))
        };
        let live = ObjectOrTombstone::Object(StoredObject::with_id_owner_version_for_testing(
            id,
            version(6),
            Owner::Immutable,
        ));

        assert_eq!(
            stored_version_at(version(5), Some(&tombstone(6))),
            version(6)
        );
        assert_eq!(
            stored_version_at(version(5), Some(&tombstone(5))),
            version(5)
        );
        assert_eq!(stored_version_at(version(5), Some(&live)), version(5));
        assert_eq!(stored_version_at(version(5), None), version(5));
    }
}
