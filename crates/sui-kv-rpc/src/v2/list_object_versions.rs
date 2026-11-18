// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Instant;

use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream;
use prost::Message;
use sui_kvstore::ObjectStatus;
use sui_kvstore::tables::objects::ObjectTombstone;
use sui_kvstore::tables::objects::ObjectVersionRow;
use sui_rpc::proto::sui::rpc::v2::ListObjectVersionsRequest;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use sui_rpc_api::AtCheckpointUpperBound;
use sui_rpc_api::RpcError;
use sui_rpc_api::ledger_history::object_versions::ObjectRemovalKind;
use sui_rpc_api::ledger_history::object_versions::ObjectVersionFrames;
use sui_rpc_api::ledger_history::object_versions::ObjectVersionItem;
use sui_rpc_api::ledger_history::object_versions::ObjectVersionsQuery;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use tracing::info;

use crate::KvRpcServer;
use crate::operation::QueryContext;
use crate::pipeline::RenderAheadError;
use crate::pipeline::ResolvedWatermarked;
use crate::pipeline::render_ahead;
use crate::render::needs_only_object_id_and_version;
use crate::render::object_to_response;
use crate::render::object_version_to_response;
use crate::served_height::ServedHeight;

/// The operation name ListObjectVersions' deadline, limiter, metrics and logs are labelled with.
pub(super) const OPERATION: &str = "list_object_versions";

/// Serve `ListObjectVersions` from the `objects` table, listing live rows as live versions and
/// tombstones as removals. Every listing is bounded by a checkpoint: `end_checkpoint - 1`, or the
/// served height when `end_checkpoint` is unset or above it (see
/// [`ObjectVersionsQuery::checkpoint_window`]). `objects` rows above the served height can be
/// written out of checkpoint order, so a listing past it could list a version and skip an older
/// one still being written, which a client paging on from the newer version's cursor would never
/// see. The object's versions at the end of that checkpoint and of the one before
/// `start_checkpoint`, read at once, the request's version range, and its cursors fold into one
/// inclusive version range (see [`ObjectVersionsQuery::plan`]), read as one range scan of up to
/// `limit` rows whose items stream to the client as they arrive. A read mask that needs only
/// `object_id` and `version` reads live rows' keys alone; any other is rendered from the objects,
/// `render-ahead` items at a time.
///
/// The checkpoint bounds read the `object_versions_by_checkpoint` index, so every request needs
/// `enable-object-at-checkpoint`, which also requires the tombstones that list removals to be
/// backfilled from genesis. kv-rpc keeps every version, so any `start_checkpoint` and
/// `end_checkpoint` up to the served height is served.
pub(crate) async fn list_object_versions(
    server: &KvRpcServer,
    request: tonic::Request<ListObjectVersionsRequest>,
) -> Result<ObjectVersionFrames, RpcError> {
    let started = Instant::now();
    let served_height = ServedHeight::of(&request);
    let request = request.into_inner();
    let config = server.ledger_history.list_object_versions();
    let query =
        ObjectVersionsQuery::new(&request, config.default_limit_items, config.max_limit_items)?;
    let client = server.limited_client(OPERATION);

    // Both checks fail the request whatever the object's history, so they come before any read.
    let reads = server.at_checkpoint_reads(client.clone(), served_height)?;
    let upper_bound = AtCheckpointUpperBound::new(reads.served_height.checkpoint);
    let window = query.checkpoint_window(&upper_bound)?;
    let ctx = server.query_context(OPERATION, client.clone(), reads.served_height)?;
    let object_id = query.object_id;
    let read_mask = query.read_mask.clone();
    let lists_removal_transactions = query.lists_removal_transactions();
    let with_contents = !needs_only_object_id_and_version(&read_mask);
    let resolution = if with_contents { "objects" } else { "versions" };
    let observed = |frames| observe_frames(ctx.clone(), resolution, &query, started, frames);
    // The object's versions at the window's two edges. An empty checkpoint range lists nothing,
    // so it reads no version at its checkpoints.
    let version_at = async |checkpoint: Option<u64>| match checkpoint {
        Some(checkpoint) if !window.is_empty() => Ok(reads
            .version_at_checkpoint(object_id, checkpoint)
            .await?
            .map(|(version, _)| version)),
        _ => Ok::<_, RpcError>(None),
    };
    let (newest_visible, newest_before_start) = futures::try_join!(
        version_at(Some(window.bounding_checkpoint)),
        version_at(window.below_start),
    )?;
    let plan = query.plan(&window, &upper_bound, newest_visible, newest_before_start)?;
    let bounding_checkpoint = window.bounding_checkpoint;
    let end_cursor = query.end_cursor(&plan, bounding_checkpoint);
    let Some(versions) = plan.versions else {
        return Ok(observed(query.frames(
            bounding_checkpoint,
            stream::empty().boxed(),
            plan.end,
            std::future::ready(end_cursor),
        )));
    };

    let rows = client
        .object_versions_stream(
            object_id,
            versions,
            query.limit,
            query.descending,
            with_contents,
        )
        .await?
        .map_err(RpcError::from)
        .map_ok(ResolvedWatermarked::<(SequenceNumber, ObjectVersionRow)>::Item)
        .boxed();
    let resolver = server.package_resolver.clone();
    let render_ctx = ctx.clone();
    let listed = render_ahead(
        rows,
        config.render_ahead,
        move |(version, row): (SequenceNumber, ObjectVersionRow)| {
            let read_mask = read_mask.clone();
            let resolver = resolver.clone();
            let ctx = render_ctx.clone();
            async move {
                let render_started = Instant::now();
                let item = match row {
                    ObjectVersionRow::Live(Some(object)) => ObjectVersionItem::Live {
                        object: object_to_response(&object, &read_mask, &resolver).await,
                        version,
                    },
                    ObjectVersionRow::Live(None) => ObjectVersionItem::Live {
                        object: object_version_to_response(object_id, version),
                        version,
                    },
                    ObjectVersionRow::Tombstone(tombstone) => {
                        removal(object_id, version, tombstone, lists_removal_transactions)?
                    }
                };
                ctx.observe_response_render(resolution, render_started.elapsed());
                Ok::<_, RpcError>(item)
            }
        },
    )
    .map(|frame| match frame {
        Ok(ResolvedWatermarked::Item(listed)) => Ok(listed),
        Ok(ResolvedWatermarked::Watermark { .. }) => {
            unreachable!("object version reads emit no watermarks")
        }
        Err(RenderAheadError::Upstream(error) | RenderAheadError::Render(error)) => Err(error),
    })
    .boxed();
    Ok(observed(query.frames(
        bounding_checkpoint,
        listed,
        plan.end,
        std::future::ready(end_cursor),
    )))
}

/// `frames`, recording what ListCheckpoints, ListTransactions and ListEvents record for theirs:
/// each poll's wait (`kv_rpc_final_stream_poll_wait_ms`), each item frame's encoded size
/// (`kv_rpc_response_page_bytes`), the terminal-only frame
/// (`kv_rpc_stream_watermark_frames_total`), the latency to the first frame, the wait after
/// yielding each frame, and the completion log. Every frame of `frames` but the last carries an
/// item, and the frame with `end` is the last.
fn observe_frames(
    ctx: QueryContext,
    resolution: &'static str,
    query: &ObjectVersionsQuery,
    started: Instant,
    mut frames: ObjectVersionFrames,
) -> ObjectVersionFrames {
    let limit_items = query.limit;
    let descending = query.descending;
    async_stream::try_stream! {
        let mut emitted = 0usize;
        let mut first_frame_emitted = false;
        let terminal_reason: Option<QueryEndReason> = loop {
            let Some(frame) = ctx.next_response_item(resolution, &mut frames).await else {
                break None;
            };
            let frame = frame?;
            if frame.item.is_some() {
                emitted += 1;
                ctx.observe_response_page_bytes(resolution, frame.encoded_len());
            } else {
                ctx.inc_stream_watermark_frames();
            }
            let end = frame.end.as_ref().map(|end| end.reason());
            if !first_frame_emitted {
                ctx.observe_stream_first_frame_latency(resolution, started.elapsed());
                first_frame_emitted = true;
            }
            let yield_started = Instant::now();
            yield frame;
            ctx.observe_stream_frame_yield_wait(resolution, yield_started.elapsed());
            if end.is_some() {
                break end;
            }
        };
        info!(
            limit_items,
            descending,
            emitted,
            ?terminal_reason,
            elapsed_ms = started.elapsed().as_millis(),
            "list_object_versions: done"
        );
    }
    .boxed()
}

/// The removal a tombstone at `version` lists, with its transaction if `with_transaction`. A
/// tombstone without its transaction can't serve a request that asks for it, which is `INTERNAL`:
/// listing removals requires the tombstone backfill, which writes every tombstone's transaction.
fn removal(
    object_id: ObjectID,
    version: SequenceNumber,
    tombstone: ObjectTombstone,
    with_transaction: bool,
) -> Result<ObjectVersionItem, RpcError> {
    let kind = match tombstone.status {
        ObjectStatus::Deleted => ObjectRemovalKind::Deleted,
        ObjectStatus::Wrapped => ObjectRemovalKind::Wrapped,
        ObjectStatus::Live => unreachable!("decoded tombstones are deleted or wrapped"),
    };
    let transaction = if with_transaction {
        let transaction = tombstone.transaction.ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                format!(
                    "the tombstone of {object_id} at version {} has no transaction",
                    version.value()
                ),
            )
        })?;
        Some(transaction)
    } else {
        None
    };
    Ok(ObjectVersionItem::Removal {
        version,
        kind,
        transaction,
    })
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use futures::TryStreamExt;
    use prometheus::core::Collector;
    use prost::Message;
    use sui_kvstore::ObjectStatus;
    use sui_kvstore::testing::MockBigtableServer;
    use sui_rpc::proto::google::rpc::BadRequest;
    use sui_rpc::proto::google::rpc::ErrorInfo;
    use sui_rpc::proto::sui::rpc::v2::ListObjectVersionsResponse;
    use sui_rpc::proto::sui::rpc::v2::ObjectRemoval;
    use sui_rpc::proto::sui::rpc::v2::Ordering;
    use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
    use sui_rpc::proto::sui::rpc::v2::QueryOptions;
    use sui_rpc::proto::sui::rpc::v2::list_object_versions_response::Item;
    use sui_rpc::proto::sui::rpc::v2::object_removal;
    use sui_rpc_api::ErrorReason;
    use sui_rpc_api::ledger_history::object_versions::ObjectVersionCursor;
    use sui_types::base_types::ObjectID;
    use sui_types::digests::TransactionDigest;
    use sui_types::storage::ObjectKey;

    use super::*;
    use crate::StagesConfig;
    use crate::v2::test_utils::insert_history;
    use crate::v2::test_utils::list_counter;
    use crate::v2::test_utils::list_histogram;
    use crate::v2::test_utils::mask;
    use crate::v2::test_utils::start_kv_rpc;
    use crate::v2::test_utils::start_kv_rpc_at_heights;
    use crate::v2::test_utils::start_kv_rpc_with_request_concurrency;
    use sui_kvstore::tables::object_versions_by_checkpoint;
    use sui_kvstore::tables::objects;
    use sui_kvstore::testing::ReadRowsCall;

    /// `(checkpoint, version, status)` for each change to the fixture object: created at version
    /// 3 in checkpoint 2, mutated at 5 in checkpoint 3, wrapped at 8 in checkpoint 5, unwrapped at
    /// 9 in checkpoint 6, mutated at 12 in checkpoint 7, and deleted at 14 in checkpoint 9. Its
    /// live versions are 3, 5, 9 and 12, and its removals 8 (wrapped) and 14 (deleted).
    const HISTORY: [(u64, u64, ObjectStatus); 6] = [
        (2, 3, ObjectStatus::Live),
        (3, 5, ObjectStatus::Live),
        (5, 8, ObjectStatus::Wrapped),
        (6, 9, ObjectStatus::Live),
        (7, 12, ObjectStatus::Live),
        (9, 14, ObjectStatus::Deleted),
    ];

    /// [`HISTORY`]'s versions and checkpoints for an object whose ID is used again after its
    /// deletion, as a dynamic field removed and re-added under the same name is: created at 3,
    /// deleted at 5, created again at 8, mutated at 9, wrapped at 12, and deleted at 14.
    const RECREATED_HISTORY: [(u64, u64, ObjectStatus); 6] = [
        (2, 3, ObjectStatus::Live),
        (3, 5, ObjectStatus::Deleted),
        (5, 8, ObjectStatus::Live),
        (6, 9, ObjectStatus::Live),
        (7, 12, ObjectStatus::Wrapped),
        (9, 14, ObjectStatus::Deleted),
    ];

    /// A kv-rpc server, with every watermark at checkpoint 10 and `at_checkpoint` reads enabled
    /// if `enable_object_at_checkpoint`, over a mock BigTable holding `history` for a new object.
    async fn start_fixture_with_history(
        enable_object_at_checkpoint: bool,
        history: &[(u64, u64, ObjectStatus)],
    ) -> (KvRpcServer, ObjectID, tokio::task::JoinHandle<()>) {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let id = ObjectID::random();
        insert_history(&mock, id, history).await;
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), enable_object_at_checkpoint).await;
        (kv_rpc, id, mock_server)
    }

    /// [`start_fixture_with_history`] for [`HISTORY`].
    async fn start_fixture(
        enable_object_at_checkpoint: bool,
    ) -> (KvRpcServer, ObjectID, tokio::task::JoinHandle<()>) {
        start_fixture_with_history(enable_object_at_checkpoint, &HISTORY).await
    }

    /// The checkpoint every fixture's watermarks are at (see [`start_kv_rpc`]), which bounds a
    /// listing without `end_checkpoint`.
    const SERVED_CHECKPOINT: u64 = 10;

    /// The cursor of `version` minted by a listing on the fixture without `end_checkpoint`.
    fn cursor(id: ObjectID, version: u64) -> Bytes {
        cursor_at(id, version, SERVED_CHECKPOINT)
    }

    /// The cursor of `version` minted by a listing bounded by `bounding_checkpoint`.
    fn cursor_at(id: ObjectID, version: u64, bounding_checkpoint: u64) -> Bytes {
        ObjectVersionCursor::Version {
            object_id: id,
            version,
            bounding_checkpoint,
        }
        .encode()
    }

    /// The cursor of the boundary between `version` and the version after it, minted by a
    /// listing bounded by `bounding_checkpoint`.
    fn boundary_at(id: ObjectID, version: u64, bounding_checkpoint: u64) -> Bytes {
        ObjectVersionCursor::Boundary {
            object_id: id,
            version,
            bounding_checkpoint,
        }
        .encode()
    }

    /// The position `cursor` names: its version, and whether it is a boundary.
    fn decoded(cursor: &Bytes) -> (u64, bool) {
        match bcs::from_bytes(cursor).expect("cursors decode") {
            ObjectVersionCursor::Version { version, .. } => (version, false),
            ObjectVersionCursor::Boundary { version, .. } => (version, true),
        }
    }

    fn request(
        id: ObjectID,
        ordering: Ordering,
        limit: Option<u32>,
        after: Option<&Bytes>,
        before: Option<&Bytes>,
    ) -> ListObjectVersionsRequest {
        let mut options = QueryOptions::default();
        options.limit = limit;
        options.ordering = Some(ordering as i32);
        options.after = after.cloned();
        options.before = before.cloned();
        let mut req = ListObjectVersionsRequest::default();
        req.object_id = Some(id.to_canonical_string(true));
        req.options = Some(options);
        req
    }

    /// What one stream listed, in order, and the reason it ended.
    #[derive(Debug)]
    struct Listing {
        /// Each item's version.
        versions: Vec<u64>,
        /// Each item's version, status (`Live`, or how it was removed), and removing
        /// transaction.
        items: Vec<(u64, ObjectStatus, Option<String>)>,
        /// Each item frame's cursor.
        cursors: Vec<Bytes>,
        /// The last frame's cursor.
        end_cursor: Bytes,
        end: QueryEndReason,
    }

    /// The checkpoint a listing for `req` on a server that serves `served_checkpoint` is bounded
    /// by: `end_checkpoint - 1` when it is set and indexed, else the served height.
    fn bounding_checkpoint(req: &ListObjectVersionsRequest, served_checkpoint: u64) -> u64 {
        req.end_checkpoint
            .filter(|end| *end <= served_checkpoint + 1)
            .map_or(served_checkpoint, |end| end.saturating_sub(1))
    }

    /// [`list_served_at`] on a fixture server, which serves [`SERVED_CHECKPOINT`].
    async fn list(
        kv_rpc: &KvRpcServer,
        req: ListObjectVersionsRequest,
    ) -> Result<Listing, tonic::Status> {
        list_served_at(kv_rpc, req, SERVED_CHECKPOINT).await
    }

    /// Everything `req`'s stream sends on `kv_rpc`, which serves `served_checkpoint`, checking
    /// the frames' shape: every frame carries a cursor, each item frame carries exactly one of
    /// `object` and `removal` and, as its cursor, the cursor of its version, minted at the
    /// listing's bounding checkpoint, each listed object is the requested one, rendered with
    /// `digest` under the default read mask, each removal is deleted or wrapped, and `end` is set
    /// exactly once, on the final frame, which carries an item only for `ITEM_LIMIT`.
    async fn list_served_at(
        kv_rpc: &KvRpcServer,
        req: ListObjectVersionsRequest,
        served_checkpoint: u64,
    ) -> Result<Listing, tonic::Status> {
        let requested = req.object_id.clone();
        let bounding_checkpoint = bounding_checkpoint(&req, served_checkpoint);
        let default_mask = req.read_mask.is_none();
        let frames: Vec<ListObjectVersionsResponse> =
            list_object_versions(kv_rpc, kv_rpc.admitted_request(req))
                .await?
                .try_collect()
                .await?;
        let id: ObjectID = requested.as_deref().unwrap().parse().unwrap();
        let mut listing = Listing {
            versions: vec![],
            items: vec![],
            cursors: vec![],
            end_cursor: Bytes::new(),
            end: QueryEndReason::Unknown,
        };
        for (index, frame) in frames.iter().enumerate() {
            let last = index + 1 == frames.len();
            assert_eq!(
                frame.end.is_some(),
                last,
                "end on frame {index} of {frames:?}"
            );
            let cursor = frame
                .cursor
                .clone()
                .unwrap_or_else(|| panic!("frame {index} has no cursor: {frames:?}"));
            if last {
                listing.end_cursor = cursor.clone();
            }
            let (version, status, transaction) = match &frame.item {
                Some(Item::Object(object)) => {
                    if let Some(object_id) = &object.object_id {
                        assert_eq!(Some(object_id), requested.as_ref());
                    }
                    if default_mask {
                        assert!(
                            object.digest.is_some(),
                            "the default read mask renders digest"
                        );
                    }
                    let version = object.version.expect("version is set");
                    (version, ObjectStatus::Live, None)
                }
                Some(Item::Removal(removal)) => {
                    let status = match removal.kind() {
                        object_removal::Kind::Deleted => ObjectStatus::Deleted,
                        object_removal::Kind::Wrapped => ObjectStatus::Wrapped,
                        kind => panic!("removal of kind {kind:?} in {frames:?}"),
                    };
                    let version = removal.version.expect("removal version is set");
                    (version, status, removal.transaction.clone())
                }
                Some(item) => panic!("frame {index} has an unknown item {item:?}"),
                None => {
                    assert!(last, "{frames:?}");
                    continue;
                }
            };
            assert_eq!(
                cursor,
                cursor_at(id, version, bounding_checkpoint),
                "version {version}"
            );
            listing.versions.push(version);
            listing.items.push((version, status, transaction));
            listing.cursors.push(cursor);
        }
        let last = frames.last().expect("a stream ends with a frame");
        listing.end = last
            .end
            .and_then(|end| end.reason)
            .map(|reason| QueryEndReason::try_from(reason).unwrap())
            .expect("the final frame has a reason");
        assert_eq!(
            last.item.is_some(),
            listing.end == QueryEndReason::ItemLimit,
            "{frames:?}"
        );
        Ok(listing)
    }

    /// The cursor a full ascending listing gives `version`.
    async fn listed_cursor(kv_rpc: &KvRpcServer, id: ObjectID, version: u64) -> Bytes {
        let listing = list(kv_rpc, request(id, Ordering::Ascending, None, None, None))
            .await
            .unwrap();
        let index = listing
            .versions
            .iter()
            .position(|&listed| listed == version)
            .unwrap_or_else(|| panic!("version {version} is not listed"));
        listing.cursors[index].clone()
    }

    /// Every request bound, alone and combined, lists the items of [`HISTORY`], and of
    /// [`RECREATED_HISTORY`], whose object is deleted and used again, inside all of them, live
    /// versions and removals alike, in either order, up to the limit, with cursors at live
    /// versions and removals: `[start_checkpoint, end_checkpoint)` keeps the versions written in
    /// those checkpoints, none before creation. A listing that fills its limit ends with
    /// `ITEM_LIMIT`, on a live version or a removal, and one that does not, with another reason.
    #[tokio::test]
    async fn test_list_object_versions_intersects_every_bound() {
        for history in [&HISTORY, &RECREATED_HISTORY] {
            assert_listings_intersect_every_bound(history).await;
        }
    }

    /// The model check of [`test_list_object_versions_intersects_every_bound`] for a fixture
    /// object with `history`.
    async fn assert_listings_intersect_every_bound(history: &[(u64, u64, ObjectStatus)]) {
        let (kv_rpc, id, mock_server) = start_fixture_with_history(true, history).await;

        let expected = |start: Option<u64>,
                        end: Option<u64>,
                        after: Option<u64>,
                        before: Option<u64>,
                        (start_checkpoint, end_checkpoint): (Option<u64>, Option<u64>),
                        ordering: Ordering,
                        limit: Option<u32>| {
            let mut items: Vec<(u64, ObjectStatus)> = history
                .iter()
                .filter(|&&(checkpoint, version, _)| {
                    start.is_none_or(|start| version >= start)
                        && end.is_none_or(|end| version < end)
                        && after.is_none_or(|after| version > after)
                        && before.is_none_or(|before| version < before)
                        && start_checkpoint.is_none_or(|start| checkpoint >= start)
                        && end_checkpoint.is_none_or(|end| checkpoint < end)
                })
                .map(|&(_, version, status)| (version, status))
                .collect();
            if ordering == Ordering::Descending {
                items.reverse();
            }
            items.truncate(limit.map_or(usize::MAX, |limit| limit as usize));
            items
        };

        // Bounds on either side of each item (in `HISTORY`, 8 is the wrap and 14 the deletion; in
        // `RECREATED_HISTORY`, 5 is the deletion and 8 the object's next creation), and
        // checkpoint ranges that end before creation, at it, between changes, and at version 8,
        // that start at a change or between changes, that hold nothing, and that end at the
        // served height.
        for start in [None, Some(5), Some(10)] {
            for end in [None, Some(9), Some(13)] {
                // A range that ends below its start is rejected (see
                // `test_list_object_versions_rejects_invalid_requests`).
                if end < start && end.is_some() {
                    continue;
                }
                for after in [None, Some(5), Some(8)] {
                    for before in [None, Some(9), Some(14)] {
                        for checkpoints in [
                            (None, None),
                            (None, Some(2)),
                            (None, Some(3)),
                            (None, Some(6)),
                            (Some(3), None),
                            (Some(4), Some(8)),
                            (Some(5), Some(6)),
                            (Some(6), Some(6)),
                            (Some(0), Some(11)),
                        ] {
                            for ordering in [Ordering::Ascending, Ordering::Descending] {
                                for limit in [None, Some(1)] {
                                    let mut req = request(
                                        id,
                                        ordering,
                                        limit,
                                        after.map(|v| cursor(id, v)).as_ref(),
                                        before.map(|v| cursor(id, v)).as_ref(),
                                    );
                                    req.start_version = start;
                                    req.end_version = end;
                                    (req.start_checkpoint, req.end_checkpoint) = checkpoints;
                                    let context = format!(
                                        "versions [{start:?}, {end:?}) after {after:?} before \
                                         {before:?} checkpoints {checkpoints:?} {ordering:?} \
                                         limit {limit:?} history {history:?}"
                                    );
                                    let listing = list(&kv_rpc, req).await.unwrap();
                                    let items: Vec<(u64, ObjectStatus)> = listing
                                        .items
                                        .iter()
                                        .map(|&(version, status, _)| (version, status))
                                        .collect();
                                    assert_eq!(
                                        items,
                                        expected(
                                            start,
                                            end,
                                            after,
                                            before,
                                            checkpoints,
                                            ordering,
                                            limit
                                        ),
                                        "{context}",
                                    );
                                    assert_eq!(
                                        listing.end == QueryEndReason::ItemLimit,
                                        limit == Some(1) && !listing.versions.is_empty(),
                                        "{context}: {listing:?}",
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        mock_server.abort();
    }

    /// A backward page from the first item a client holds, followed by forward pages from that
    /// page's cursors, lists every item, live versions and removals, with no gaps or repeats.
    /// Full pages end with `ITEM_LIMIT`; the backward pages end at the object's first version
    /// with `CHECKPOINT_BOUND`, and the forward ones at its newest, its deletion, with
    /// `LEDGER_TIP`. Cursors from descending pages bound ascending ones.
    #[tokio::test]
    async fn test_list_object_versions_pages_backward_then_forward_from_cursors() {
        let (kv_rpc, id, mock_server) = start_fixture(true).await;
        let page = async |ordering, after: Option<&Bytes>, before: Option<&Bytes>| {
            let listing = list(&kv_rpc, request(id, ordering, Some(2), after, before))
                .await
                .unwrap();
            (listing.versions.clone(), listing.end, listing.cursors)
        };

        // The client holds the page after version 5, starting at the wrap.
        let after_5 = listed_cursor(&kv_rpc, id, 5).await;
        let (held, end, held_cursors) = page(Ordering::Ascending, Some(&after_5), None).await;
        assert_eq!((held, end), (vec![8, 9], QueryEndReason::ItemLimit));

        // Backward from the first item held, until the start.
        let (back, end, back_cursors) =
            page(Ordering::Descending, None, Some(&held_cursors[0])).await;
        assert_eq!((back, end), (vec![5, 3], QueryEndReason::ItemLimit));
        let (start, end, _) = page(Ordering::Descending, None, Some(&back_cursors[1])).await;
        assert_eq!((start, end), (vec![], QueryEndReason::CheckpointBound));

        // Forward from the backward page's lowest version, until the newest version.
        let mut after = back_cursors[1].clone();
        for (expected, reason) in [
            (vec![5, 8], QueryEndReason::ItemLimit),
            (vec![9, 12], QueryEndReason::ItemLimit),
            (vec![14], QueryEndReason::LedgerTip),
        ] {
            let (forward, end, cursors) = page(Ordering::Ascending, Some(&after), None).await;
            assert_eq!((forward, end), (expected, reason));
            after = cursors.last().unwrap().clone();
        }

        mock_server.abort();
    }

    /// The fields named by the `BadRequest` field violations in `status`'s details.
    fn bad_request_fields(status: &tonic::Status) -> Vec<String> {
        sui_rpc::proto::google::rpc::Status::decode(status.details())
            .expect("error details should decode as google.rpc.Status")
            .details
            .iter()
            .filter_map(|detail| detail.to_msg::<BadRequest>().ok())
            .flat_map(|bad_request| bad_request.field_violations)
            .map(|violation| violation.field)
            .collect()
    }

    /// The one `ErrorInfo` in `status`'s details.
    fn error_info(status: &tonic::Status) -> ErrorInfo {
        let infos: Vec<ErrorInfo> = sui_rpc::proto::google::rpc::Status::decode(status.details())
            .expect("error details should decode as google.rpc.Status")
            .details
            .iter()
            .filter_map(|detail| detail.to_msg::<ErrorInfo>().ok())
            .collect();
        let [info] = <[ErrorInfo; 1]>::try_from(infos)
            .unwrap_or_else(|infos| panic!("expected one ErrorInfo, got {infos:?}"));
        info
    }

    /// Malformed requests are `InvalidArgument` naming the field, whether or not `at_checkpoint`
    /// reads are enabled, including an `end_checkpoint` below `start_checkpoint`, before any
    /// range or tip rule, as `ListTransactions` validates first: even a descending listing past
    /// the tip. A descending listing whose checkpoint range starts above the served height is
    /// `OutOfRange` / `ABOVE_LATEST_AVAILABLE`, naming the bound, as `ListTransactions` is.
    #[tokio::test]
    async fn test_list_object_versions_rejects_invalid_requests() {
        let (kv_rpc, id, mock_server) = start_fixture(true).await;
        let (disabled, _, disabled_server) = start_fixture(false).await;

        let other_object = cursor(ObjectID::random(), 5);
        let malformed = Bytes::from_static(&[0xFF, 0xFF]);
        let mut trailing = cursor(id, 5).to_vec();
        trailing.push(0);
        let trailing = Bytes::from(trailing);
        let ascending = || request(id, Ordering::Ascending, None, None, None);

        let mut missing_object_id = ascending();
        missing_object_id.object_id = None;
        let mut invalid_object_id = ascending();
        invalid_object_id.object_id = Some("0xnot-an-object".to_string());
        let mut invalid_read_mask = ascending();
        invalid_read_mask.read_mask = Some(prost_types::FieldMask {
            paths: vec!["no_such_field".to_string()],
        });
        let mut invalid_removal_read_mask = ascending();
        invalid_removal_read_mask.removal_read_mask = Some(mask(&["object_id"]));
        let mut invalid_ordering = ascending();
        invalid_ordering.options.as_mut().unwrap().ordering = Some(7);
        let mut reversed_range = ascending();
        reversed_range.start_version = Some(5);
        reversed_range.end_version = Some(4);
        let mut reversed_checkpoints = ascending();
        reversed_checkpoints.start_checkpoint = Some(5);
        reversed_checkpoints.end_checkpoint = Some(4);
        let mut malformed_past_tip =
            request(id, Ordering::Descending, None, Some(&malformed), None);
        malformed_past_tip.end_checkpoint = Some(u64::MAX);
        let mut reversed_past_tip = request(id, Ordering::Descending, None, None, None);
        reversed_past_tip.start_version = Some(5);
        reversed_past_tip.end_version = Some(4);
        reversed_past_tip.start_checkpoint = Some(20);

        for (req, field, message) in [
            (missing_object_id, "object_id", "missing object_id"),
            (invalid_object_id, "object_id", "invalid object_id"),
            (invalid_read_mask, "read_mask", "invalid read_mask path"),
            (
                invalid_removal_read_mask,
                "removal_read_mask",
                "invalid removal_read_mask path",
            ),
            (invalid_ordering, "options.ordering", "invalid ordering"),
            (
                reversed_range,
                "end_version",
                "end_version must be greater than or equal to start_version",
            ),
            (
                reversed_checkpoints,
                "end_checkpoint",
                "end_checkpoint must be greater than or equal to start_checkpoint",
            ),
            (
                request(id, Ordering::Ascending, None, Some(&malformed), None),
                "options.after",
                "invalid cursor encoding",
            ),
            (
                request(id, Ordering::Descending, None, None, Some(&trailing)),
                "options.before",
                "invalid cursor encoding",
            ),
            (
                request(id, Ordering::Descending, None, None, Some(&other_object)),
                "options.before",
                "cursor object ID does not match request object ID",
            ),
            (
                request(id, Ordering::Ascending, None, Some(&other_object), None),
                "options.after",
                "cursor object ID does not match request object ID",
            ),
            (
                malformed_past_tip,
                "options.after",
                "invalid cursor encoding",
            ),
            (
                reversed_past_tip,
                "end_version",
                "end_version must be greater than or equal to start_version",
            ),
        ] {
            for server in [&kv_rpc, &disabled] {
                let status = list(server, req.clone()).await.unwrap_err();
                assert_eq!(status.code(), tonic::Code::InvalidArgument, "{message}");
                assert!(
                    status.message().contains(message),
                    "expected {message:?}, got {:?}",
                    status.message()
                );
                assert_eq!(bad_request_fields(&status), [field], "{message}");
            }
        }

        // The served height is 10, so checkpoint 11 is not yet indexed. A descending listing that
        // would start above it is refused, even when its version bounds leave nothing to list:
        // with `end_checkpoint` above 11, or with `end_checkpoint` unset and `start_checkpoint` at
        // 11 or above.
        for (start_checkpoint, end_checkpoint, bound, requested) in [
            (None, Some(12), "end_checkpoint", 12),
            (Some(3), Some(20), "end_checkpoint", 20),
            (Some(11), None, "start_checkpoint", 11),
            (Some(20), None, "start_checkpoint", 20),
        ] {
            for start_version in [None, Some(100)] {
                let mut req = request(id, Ordering::Descending, None, None, None);
                req.start_checkpoint = start_checkpoint;
                req.end_checkpoint = end_checkpoint;
                req.start_version = start_version;
                let status = list(&kv_rpc, req).await.unwrap_err();
                assert_eq!(status.code(), tonic::Code::OutOfRange, "{status:?}");
                assert_eq!(
                    status.message(),
                    format!(
                        "requested data above latest available; {bound} is {requested}, and \
                         only checkpoints below 11 are available"
                    ),
                );
                let info = error_info(&status);
                assert_eq!(info.reason, ErrorReason::AboveLatestAvailable.as_str_name());
                assert_eq!(
                    info.metadata,
                    [
                        ("bound".to_owned(), bound.to_owned()),
                        ("requestedCheckpoint".to_owned(), requested.to_string()),
                        ("firstUnavailableCheckpoint".to_owned(), "11".to_owned()),
                    ]
                    .into(),
                );
            }
        }
        // At the checkpoint after the served height, or with the start at the served height, the
        // range is indexed.
        for (start_checkpoint, end_checkpoint) in [(None, Some(11)), (Some(10), None)] {
            let mut req = request(id, Ordering::Descending, None, None, None);
            req.start_checkpoint = start_checkpoint;
            req.end_checkpoint = end_checkpoint;
            list(&kv_rpc, req).await.unwrap();
        }

        mock_server.abort();
        disabled_server.abort();
    }

    /// Every listing reads the version index to bound itself by a checkpoint, so without
    /// `at_checkpoint` reads enabled every request is `Unimplemented` / `NOT_SERVED_HERE` with
    /// cause `disabled_by_config`, whatever its checkpoint range, even one above the served
    /// height of 10, and even when its version bounds leave nothing to list.
    #[tokio::test]
    async fn test_list_object_versions_needs_the_at_checkpoint_flag() {
        let (kv_rpc, id, mock_server) = start_fixture(false).await;

        for (start_checkpoint, end_checkpoint) in [
            (None, None),
            (Some(3), Some(6)),
            (None, Some(12)),
            (Some(11), None),
        ] {
            for start_version in [None, Some(100)] {
                let mut req = request(id, Ordering::Descending, None, None, None);
                req.start_checkpoint = start_checkpoint;
                req.end_checkpoint = end_checkpoint;
                req.start_version = start_version;
                let status = list(&kv_rpc, req).await.unwrap_err();
                assert_eq!(status.code(), tonic::Code::Unimplemented, "{status:?}");
                let info = error_info(&status);
                assert_eq!(info.reason, ErrorReason::NotServedHere.as_str_name());
                assert_eq!(info.domain, ErrorReason::DOMAIN);
                assert_eq!(
                    info.metadata,
                    [("cause".to_owned(), "disabled_by_config".to_owned())].into(),
                    "{status:?}"
                );
            }
        }

        mock_server.abort();
    }

    /// Without `end_checkpoint`, a listing is bounded by the served height: `objects` rows above
    /// it can be written out of checkpoint order, and a version listed past it could hide an
    /// older one still being written from a client paging on from its cursor. A cursor above
    /// the object's version at the served height, such as one from a replica that has indexed
    /// further, follows the ledger List APIs: descending, it is `OutOfRange` even when the
    /// version bounds leave nothing to list, because listing from this replica's view would
    /// start below versions it lacks; ascending, the listing is served from this replica's view.
    /// An `after` past the tip then lists nothing and ends with `LEDGER_TIP` in a window that
    /// reaches past the tip, with `CURSOR_BOUND` against a `before` at or below the version
    /// after it, or with `CHECKPOINT_BOUND` against an `end_version` there, and a `before` past
    /// the tip leaves the tip as the end. An ascending listing whose `end_checkpoint` is above
    /// the checkpoint after the served height is not refused either: it runs to the tip and
    /// ends with `LEDGER_TIP`, never `CHECKPOINT_BOUND`, since its range is not complete. An
    /// indexed `end_checkpoint` caps the listing whatever the cursor.
    #[tokio::test]
    async fn test_list_object_versions_stops_at_the_served_height() {
        use Ordering::*;
        use QueryEndReason::*;
        // Above the served height of 10, version 20 from checkpoint 12 is written while the
        // version from checkpoint 11 is still being written.
        let mut history = HISTORY.to_vec();
        history.push((12, 20, ObjectStatus::Live));
        let (kv_rpc, id, mock_server) = start_fixture_with_history(true, &history).await;

        // Version 14 deleted the object in checkpoint 9, so 14 is its version at the served
        // height, listed as a removal, and a cursor at it is served. `None` expects the cursor to
        // be past the tip.
        let all = vec![3, 5, 8, 9, 12, 14];
        let cases: [(Bounds, Option<Listed>); 21] = [
            (
                (Ascending, None, None, None, None, None, None),
                Some((all.clone(), LedgerTip)),
            ),
            (
                (Descending, None, None, None, None, None, Some(1)),
                Some((vec![14], ItemLimit)),
            ),
            (
                (Ascending, None, None, Some(12), None, None, None),
                Some((vec![14], LedgerTip)),
            ),
            (
                (Descending, None, None, Some(14), None, None, None),
                Some((vec![], CursorBound)),
            ),
            (
                (Ascending, None, None, Some(20), None, None, None),
                Some((vec![], LedgerTip)),
            ),
            ((Descending, None, None, Some(20), None, None, None), None),
            (
                (Ascending, None, None, None, Some(21), None, None),
                Some((all.clone(), LedgerTip)),
            ),
            (
                (Descending, None, None, None, Some(21), None, Some(1)),
                None,
            ),
            // A descending listing's empty version range does not hide a cursor past the tip.
            (
                (Descending, None, Some(5), Some(20), None, None, None),
                None,
            ),
            // An `end_version` at or below the tip ends the window there, by its own rules.
            (
                (Ascending, None, Some(5), Some(20), None, None, None),
                Some((vec![], CheckpointBound)),
            ),
            // A window reaching past the tip ends at `after`, complete with `CHECKPOINT_BOUND`
            // when `end_version` leaves nothing above the cursor.
            (
                (Ascending, None, Some(18), Some(20), None, None, None),
                Some((vec![], CheckpointBound)),
            ),
            (
                (Ascending, None, Some(21), Some(20), None, None, None),
                Some((vec![], CheckpointBound)),
            ),
            (
                (Ascending, None, Some(22), Some(20), None, None, None),
                Some((vec![], LedgerTip)),
            ),
            // A `before` at the version after `after` leaves nothing between them.
            (
                (Ascending, None, None, Some(20), Some(21), None, None),
                Some((vec![], CursorBound)),
            ),
            (
                (Ascending, None, None, Some(20), Some(22), None, None),
                Some((vec![], LedgerTip)),
            ),
            (
                (Ascending, None, None, Some(12), Some(21), None, None),
                Some((vec![14], LedgerTip)),
            ),
            // An indexed `end_checkpoint` caps the listing at checkpoint 10 whatever the cursor.
            (
                (Descending, None, None, None, Some(21), Some(11), Some(1)),
                Some((vec![14], ItemLimit)),
            ),
            (
                (Ascending, None, None, Some(20), None, Some(11), None),
                Some((vec![], CheckpointBound)),
            ),
            // An ascending `end_checkpoint` past the tip lists through the tip, and an `after`
            // past the tip lists nothing; both end with `LEDGER_TIP`.
            (
                (Ascending, None, None, None, None, Some(13), None),
                Some((all.clone(), LedgerTip)),
            ),
            (
                (Ascending, None, None, Some(20), None, Some(13), None),
                Some((vec![], LedgerTip)),
            ),
            (
                (Ascending, None, None, Some(20), None, Some(30), None),
                Some((vec![], LedgerTip)),
            ),
        ];
        for ((ordering, start, end, after, before, end_checkpoint, limit), expected) in cases {
            let after_cursor = after.map(|version| cursor(id, version));
            let before_cursor = before.map(|version| cursor(id, version));
            let mut req = request(
                id,
                ordering,
                limit,
                after_cursor.as_ref(),
                before_cursor.as_ref(),
            );
            req.start_version = start;
            req.end_version = end;
            req.end_checkpoint = end_checkpoint;
            let context = format!(
                "{ordering:?} [{start:?}, {end:?}) after {after:?} before {before:?} \
                 end_checkpoint {end_checkpoint:?} limit {limit:?}"
            );
            match (list(&kv_rpc, req).await, expected) {
                (Ok(listed), Some(expected)) => {
                    assert_eq!((listed.versions, listed.end), expected, "{context}")
                }
                (Err(status), None) => {
                    assert_eq!(status.code(), tonic::Code::OutOfRange, "{context}");
                    assert_eq!(
                        status.message(),
                        "requested data above latest available; cursor is at checkpoint 11, \
                         and only checkpoints below 11 are available",
                        "{context}"
                    );
                }
                (outcome, expected) => {
                    panic!("{context}: expected {expected:?}, got {outcome:?}")
                }
            }
        }
        mock_server.abort();

        // An object first written above the served height has no version at it, so any cursor
        // for it is past the tip: descending it is `OutOfRange`, and ascending, like a listing
        // without one, finds nothing up to the tip.
        let (kv_rpc, id, mock_server) =
            start_fixture_with_history(true, &[(12, 20, ObjectStatus::Live)]).await;
        let after_20 = cursor(id, 20);
        let before_21 = cursor(id, 21);
        for ordering in [Ascending, Descending] {
            let listed = list(&kv_rpc, request(id, ordering, None, None, None)).await;
            assert_eq!(listed.unwrap().versions, Vec::<u64>::new(), "{ordering:?}");
            for (name, after, before) in [
                ("after 20", Some(&after_20), None),
                ("before 21", None, Some(&before_21)),
            ] {
                let listed = list(&kv_rpc, request(id, ordering, None, after, before)).await;
                match ordering {
                    Ascending => {
                        let listed = listed.unwrap();
                        assert_eq!((listed.versions, listed.end), (vec![], LedgerTip), "{name}");
                    }
                    _ => assert_eq!(
                        listed.unwrap_err().code(),
                        tonic::Code::OutOfRange,
                        "{ordering:?} {name}"
                    ),
                }
            }
        }

        mock_server.abort();
    }

    /// [`start_fixture_with_history`], also returning the mock BigTable to inspect its reads.
    async fn start_fixture_with_mock(
        history: &[(u64, u64, ObjectStatus)],
    ) -> (
        KvRpcServer,
        ObjectID,
        MockBigtableServer,
        tokio::task::JoinHandle<()>,
    ) {
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let id = ObjectID::random();
        insert_history(&mock, id, history).await;
        let kv_rpc = start_kv_rpc(addr, StagesConfig::default(), true).await;
        (kv_rpc, id, mock, mock_server)
    }

    /// The `objects` table reads since the mock's calls were last cleared.
    async fn objects_reads(mock: &MockBigtableServer) -> Vec<ReadRowsCall> {
        mock.read_rows_calls()
            .await
            .into_iter()
            .filter(|call| call.table == objects::NAME)
            .collect()
    }

    /// A listing ends with the reason of the bound it reaches, as the ledger List APIs name them,
    /// whether its last item is a live version or a removal: the far cursor in listing order
    /// (`before` ascending, `after` descending), when at least as tight as the request's window,
    /// ends it with `CURSOR_BOUND`; an ascending listing without `end_checkpoint` that reaches the
    /// object's newest version before `end_version` ends with `LEDGER_TIP`, even from a starting
    /// cursor at that version and even when that version is the object's deletion; and every
    /// other one ends at the window the request set, with `CHECKPOINT_BOUND`. A listing that
    /// fills its limit ends with `ITEM_LIMIT` on its last item, even when no item remains.
    #[tokio::test]
    async fn test_list_object_versions_ends_with_the_reason_of_its_bound() {
        use Ordering::*;
        use QueryEndReason::*;
        let (kv_rpc, id, mock_server) = start_fixture(true).await;

        // The versions each request lists and the reason the stream ends with. The served height
        // is 10, at which the object's newest version is 14, its deletion.
        let cases = [
            (
                (Ascending, None, None, None, None, None, None),
                vec![3, 5, 8, 9, 12, 14],
                LedgerTip,
            ),
            (
                (Ascending, None, Some(10), None, None, None, None),
                vec![3, 5, 8, 9],
                CheckpointBound,
            ),
            // `end_version` just above the newest version reaches both at once.
            (
                (Ascending, None, Some(15), None, None, None, None),
                vec![3, 5, 8, 9, 12, 14],
                CheckpointBound,
            ),
            (
                (Ascending, None, Some(16), None, None, None, None),
                vec![3, 5, 8, 9, 12, 14],
                LedgerTip,
            ),
            (
                (Ascending, None, None, None, None, Some(8), None),
                vec![3, 5, 8, 9, 12],
                CheckpointBound,
            ),
            (
                (Ascending, None, None, None, None, Some(6), None),
                vec![3, 5, 8],
                CheckpointBound,
            ),
            (
                (Ascending, None, None, None, None, Some(2), None),
                vec![],
                CheckpointBound,
            ),
            (
                (Ascending, Some(20), None, None, None, None, None),
                vec![],
                LedgerTip,
            ),
            // An empty range holds no version, so even one above the tip never reaches it.
            (
                (Ascending, Some(10), Some(10), None, None, None, None),
                vec![],
                CheckpointBound,
            ),
            (
                (Ascending, Some(20), Some(20), None, None, None, None),
                vec![],
                CheckpointBound,
            ),
            (
                (Ascending, None, None, None, Some(9), None, None),
                vec![3, 5, 8],
                CursorBound,
            ),
            // A cursor as tight as `end_version` ends the listing; a looser one does not.
            (
                (Ascending, None, Some(9), None, Some(9), None, None),
                vec![3, 5, 8],
                CursorBound,
            ),
            (
                (Ascending, None, Some(6), None, Some(9), None, None),
                vec![3, 5],
                CheckpointBound,
            ),
            (
                (Ascending, None, None, Some(12), None, None, None),
                vec![14],
                LedgerTip,
            ),
            // Resuming from the deletion's cursor reaches the tip with nothing listed.
            (
                (Ascending, None, None, Some(14), None, None, None),
                vec![],
                LedgerTip,
            ),
            (
                (Ascending, None, None, Some(12), Some(13), None, None),
                vec![],
                CursorBound,
            ),
            (
                (Ascending, None, None, Some(9), Some(14), None, None),
                vec![12],
                CursorBound,
            ),
            (
                (Ascending, None, None, None, None, None, Some(4)),
                vec![3, 5, 8, 9],
                ItemLimit,
            ),
            // The limit-th item is the wrap.
            (
                (Ascending, None, None, None, None, None, Some(3)),
                vec![3, 5, 8],
                ItemLimit,
            ),
            (
                (Descending, None, None, None, None, None, None),
                vec![14, 12, 9, 8, 5, 3],
                CheckpointBound,
            ),
            (
                (Descending, None, None, None, None, Some(5), None),
                vec![5, 3],
                CheckpointBound,
            ),
            (
                (Descending, None, None, Some(5), None, None, None),
                vec![14, 12, 9, 8],
                CursorBound,
            ),
            (
                (Descending, None, None, Some(8), None, None, None),
                vec![14, 12, 9],
                CursorBound,
            ),
            // `after` at `start_version` binds; one below it admits the same versions but
            // leaves the window's edge in place, as a ledger cursor below `start_checkpoint` does.
            (
                (Descending, Some(5), None, Some(5), None, None, None),
                vec![14, 12, 9, 8],
                CursorBound,
            ),
            (
                (Descending, Some(5), None, Some(4), None, None, None),
                vec![14, 12, 9, 8, 5],
                CheckpointBound,
            ),
            (
                (Descending, None, None, None, Some(9), None, None),
                vec![8, 5, 3],
                CheckpointBound,
            ),
            (
                (Descending, None, None, Some(5), Some(6), None, None),
                vec![],
                CursorBound,
            ),
            // The limit-th item is the deletion, then the wrap.
            (
                (Descending, None, None, None, None, None, Some(1)),
                vec![14],
                ItemLimit,
            ),
            (
                (Descending, None, None, None, Some(14), None, Some(3)),
                vec![12, 9, 8],
                ItemLimit,
            ),
        ];
        assert_ends(&kv_rpc, id, &cases).await;

        // An object with no version at the served height has nothing to list.
        let other = ObjectID::random();
        for (ordering, reason) in [(Ascending, LedgerTip), (Descending, CheckpointBound)] {
            let listing = list(&kv_rpc, request(other, ordering, None, None, None))
                .await
                .unwrap();
            assert_eq!((listing.versions, listing.end), (vec![], reason));
        }
        mock_server.abort();

        // An object whose newest version is live: a client following its new versions resumes
        // from that version's cursor and reaches the tip, and a starting cursor at the window's
        // edge ends the listing there, not at the cursor.
        let (kv_rpc, id, mock_server) = start_fixture_with_history(true, &HISTORY[..5]).await;
        assert_ends(
            &kv_rpc,
            id,
            &[
                (
                    (Ascending, None, None, Some(12), None, None, None),
                    vec![],
                    LedgerTip,
                ),
                (
                    (Ascending, None, Some(20), Some(12), None, None, None),
                    vec![],
                    LedgerTip,
                ),
                (
                    (Ascending, None, None, Some(12), None, Some(11), None),
                    vec![],
                    CheckpointBound,
                ),
                (
                    (Ascending, None, Some(13), Some(12), None, None, None),
                    vec![],
                    CheckpointBound,
                ),
                (
                    (Descending, Some(5), None, None, Some(5), None, None),
                    vec![],
                    CheckpointBound,
                ),
                (
                    (Descending, None, None, None, Some(3), None, None),
                    vec![],
                    CheckpointBound,
                ),
            ],
        )
        .await;
        mock_server.abort();

        // An object deleted and later created again: the deletion is an item like any other, so
        // pages run across it, and only the window's own bounds end the listing.
        let (kv_rpc, id, mock_server) = start_fixture_with_history(true, &RECREATED_HISTORY).await;
        assert_ends(
            &kv_rpc,
            id,
            &[
                (
                    (Ascending, None, None, None, None, None, None),
                    vec![3, 5, 8, 9, 12, 14],
                    LedgerTip,
                ),
                (
                    (Ascending, None, None, Some(5), None, None, None),
                    vec![8, 9, 12, 14],
                    LedgerTip,
                ),
                (
                    (Ascending, None, None, None, None, None, Some(2)),
                    vec![3, 5],
                    ItemLimit,
                ),
                (
                    (Ascending, None, Some(6), None, None, None, None),
                    vec![3, 5],
                    CheckpointBound,
                ),
                (
                    (Ascending, None, None, None, None, Some(4), None),
                    vec![3, 5],
                    CheckpointBound,
                ),
                (
                    (Ascending, None, None, Some(3), Some(8), None, None),
                    vec![5],
                    CursorBound,
                ),
                (
                    (Descending, None, None, None, Some(8), None, None),
                    vec![5, 3],
                    CheckpointBound,
                ),
                (
                    (Descending, None, None, Some(5), None, None, None),
                    vec![14, 12, 9, 8],
                    CursorBound,
                ),
            ],
        )
        .await;
        mock_server.abort();
    }

    /// An object created already wrapped and then unwrapped and deleted in one transaction never
    /// had a live version: its only item is the deletion, listed in either order and inside
    /// every bound like any other item.
    #[tokio::test]
    async fn test_list_object_versions_lists_an_object_that_is_only_a_deletion() {
        use Ordering::*;
        use QueryEndReason::*;
        let (kv_rpc, id, mock_server) =
            start_fixture_with_history(true, &[(4, 7, ObjectStatus::Deleted)]).await;

        for ordering in [Ascending, Descending] {
            let listing = list(&kv_rpc, request(id, ordering, None, None, None))
                .await
                .unwrap();
            assert_eq!(
                listing.items,
                [(7, ObjectStatus::Deleted, None)],
                "{ordering:?}"
            );
        }
        assert_ends(
            &kv_rpc,
            id,
            &[
                (
                    (Ascending, None, None, None, None, None, None),
                    vec![7],
                    LedgerTip,
                ),
                (
                    (Descending, None, None, None, None, None, None),
                    vec![7],
                    CheckpointBound,
                ),
                (
                    (Ascending, None, None, None, None, None, Some(1)),
                    vec![7],
                    ItemLimit,
                ),
                (
                    (Descending, None, None, None, None, None, Some(1)),
                    vec![7],
                    ItemLimit,
                ),
                (
                    (Ascending, None, None, None, None, Some(4), None),
                    vec![],
                    CheckpointBound,
                ),
                (
                    (Ascending, None, None, None, None, Some(5), None),
                    vec![7],
                    CheckpointBound,
                ),
                (
                    (Ascending, None, None, Some(7), None, None, None),
                    vec![],
                    LedgerTip,
                ),
                (
                    (Descending, None, None, None, Some(7), None, None),
                    vec![],
                    CheckpointBound,
                ),
                (
                    (Ascending, Some(8), None, None, None, None, None),
                    vec![],
                    LedgerTip,
                ),
                (
                    (Ascending, None, Some(7), None, None, None, None),
                    vec![],
                    CheckpointBound,
                ),
            ],
        )
        .await;

        mock_server.abort();
    }

    /// The versions a listing lists and the reason it ends with.
    type Listed = (Vec<u64>, QueryEndReason);

    /// `(ordering, start_version, end_version, after, before, end_checkpoint, limit)`.
    type Bounds = (
        Ordering,
        Option<u64>,
        Option<u64>,
        Option<u64>,
        Option<u64>,
        Option<u64>,
        Option<u32>,
    );

    /// Each request of `cases` for the object `id` lists the versions given with it and ends with
    /// the reason given with it.
    async fn assert_ends(
        kv_rpc: &KvRpcServer,
        id: ObjectID,
        cases: &[(Bounds, Vec<u64>, QueryEndReason)],
    ) {
        for &((ordering, start, end, after, before, end_checkpoint, limit), ref versions, reason) in
            cases
        {
            let after_cursor = after.map(|version| cursor(id, version));
            let before_cursor = before.map(|version| cursor(id, version));
            let mut req = request(
                id,
                ordering,
                limit,
                after_cursor.as_ref(),
                before_cursor.as_ref(),
            );
            req.start_version = start;
            req.end_version = end;
            req.end_checkpoint = end_checkpoint;
            let listing = list(kv_rpc, req).await.unwrap();
            assert_eq!(
                (&listing.versions, listing.end),
                (versions, reason),
                "{ordering:?} [{start:?}, {end:?}) after {after:?} before {before:?} \
                 end_checkpoint {end_checkpoint:?} limit {limit:?}",
            );
        }
    }

    /// `(checkpoint, version, status)` for an object written more than once in some checkpoints:
    /// created at 3 in checkpoint 2, mutated at 5 and 6 in checkpoint 4, at 9 in checkpoint 6,
    /// and at 11 and 12 in checkpoint 8. The version index records 3, 6, 9 and 12.
    const SPLIT_HISTORY: [(u64, u64, ObjectStatus); 6] = [
        (2, 3, ObjectStatus::Live),
        (4, 5, ObjectStatus::Live),
        (4, 6, ObjectStatus::Live),
        (6, 9, ObjectStatus::Live),
        (8, 11, ObjectStatus::Live),
        (8, 12, ObjectStatus::Live),
    ];

    /// `request` for `ordering`, `limit` and the cursors, with `configure` applied.
    fn configured(
        ordering: Ordering,
        limit: Option<u32>,
        after: Option<&Bytes>,
        before: Option<&Bytes>,
        id: ObjectID,
        configure: impl FnOnce(&mut ListObjectVersionsRequest),
    ) -> ListObjectVersionsRequest {
        let mut req = request(id, ordering, limit, after, before);
        configure(&mut req);
        req
    }

    /// A request of [`test_list_object_versions_ends_with_a_resume_cursor`], named, with the
    /// versions it lists, its end reason, and its last frame's cursor.
    type EndCase = (
        &'static str,
        ListObjectVersionsRequest,
        Vec<u64>,
        QueryEndReason,
        Bytes,
    );

    /// The last frame's cursor is where a request in the same order resumes: the far cursor
    /// for `CURSOR_BOUND`, the starting cursor when it sits past the window, including an
    /// ascending `after` past the tip, which comes back unchanged, and otherwise the boundary at
    /// the window's edge: ascending, below its end; descending, below its lowest version. A
    /// listing that lists nothing, from an empty checkpoint range or `end_checkpoint` 0
    /// included, still ends with a cursor. A listing that fills its limit ends on its last
    /// item's cursor.
    #[tokio::test]
    async fn test_list_object_versions_ends_with_a_resume_cursor() {
        use Ordering::*;
        use QueryEndReason::*;
        let (kv_rpc, id, mock_server) = start_fixture_with_history(true, &SPLIT_HISTORY).await;
        let at = |version| cursor(id, version);
        let edge = |version| boundary_at(id, version, SERVED_CHECKPOINT);
        // A cursor from a replica that has indexed checkpoint 15.
        let past_tip = cursor_at(id, 30, 15);
        let all = vec![3, 5, 6, 9, 11, 12];
        let cases: Vec<EndCase> = vec![
            (
                "ascending to the tip",
                configured(Ascending, None, None, None, id, |_| {}),
                all.clone(),
                LedgerTip,
                edge(12),
            ),
            (
                "ascending, ending on the limit",
                configured(Ascending, Some(3), None, None, id, |_| {}),
                vec![3, 5, 6],
                ItemLimit,
                at(6),
            ),
            (
                "ascending, resumed in the middle of checkpoint 4",
                configured(Ascending, None, Some(&at(5)), None, id, |_| {}),
                vec![6, 9, 11, 12],
                LedgerTip,
                edge(12),
            ),
            (
                "ascending from start_version 6",
                configured(Ascending, None, None, None, id, |req| {
                    req.start_version = Some(6)
                }),
                vec![6, 9, 11, 12],
                LedgerTip,
                edge(12),
            ),
            (
                "ascending from start_checkpoint 5",
                configured(Ascending, None, None, None, id, |req| {
                    req.start_checkpoint = Some(5)
                }),
                vec![9, 11, 12],
                LedgerTip,
                edge(12),
            ),
            (
                "ascending to end_version 12",
                configured(Ascending, None, None, None, id, |req| {
                    req.end_version = Some(12)
                }),
                vec![3, 5, 6, 9, 11],
                CheckpointBound,
                edge(11),
            ),
            (
                "ascending to end_checkpoint 7",
                configured(Ascending, None, None, None, id, |req| {
                    req.end_checkpoint = Some(7)
                }),
                vec![3, 5, 6, 9],
                CheckpointBound,
                boundary_at(id, 9, 6),
            ),
            (
                "ascending to an end_checkpoint past the tip",
                configured(Ascending, None, None, None, id, |req| {
                    req.end_checkpoint = Some(30)
                }),
                all.clone(),
                LedgerTip,
                edge(12),
            ),
            (
                "ascending after a cursor past the tip",
                configured(Ascending, None, Some(&past_tip), None, id, |req| {
                    req.end_checkpoint = Some(30)
                }),
                vec![],
                LedgerTip,
                past_tip.clone(),
            ),
            (
                "ascending after a cursor past the tip, without end_checkpoint",
                configured(Ascending, None, Some(&past_tip), None, id, |_| {}),
                vec![],
                LedgerTip,
                past_tip.clone(),
            ),
            (
                "ascending, an empty checkpoint range",
                configured(Ascending, None, None, None, id, |req| {
                    req.start_checkpoint = Some(5);
                    req.end_checkpoint = Some(5);
                }),
                vec![],
                CheckpointBound,
                boundary_at(id, 0, 4),
            ),
            (
                "ascending to end_checkpoint 0",
                configured(Ascending, None, None, None, id, |req| {
                    req.end_checkpoint = Some(0)
                }),
                vec![],
                CheckpointBound,
                boundary_at(id, 0, 0),
            ),
            (
                "descending from end_checkpoint 0",
                configured(Descending, None, None, None, id, |req| {
                    req.end_checkpoint = Some(0)
                }),
                vec![],
                CheckpointBound,
                boundary_at(id, 0, 0),
            ),
            // An empty range past the tip ends at the tip, as `ListTransactions` ends it: this
            // server has not indexed `end_checkpoint - 1`.
            (
                "ascending, an empty checkpoint range past the tip",
                configured(Ascending, None, None, None, id, |req| {
                    req.start_checkpoint = Some(20);
                    req.end_checkpoint = Some(20);
                }),
                vec![],
                LedgerTip,
                boundary_at(id, 0, 10),
            ),
            (
                "ascending after a cursor, in an empty checkpoint range past the tip",
                configured(Ascending, None, Some(&at(6)), None, id, |req| {
                    req.start_checkpoint = Some(20);
                    req.end_checkpoint = Some(20);
                }),
                vec![],
                LedgerTip,
                at(6),
            ),
            // At the checkpoint after the tip, an ascending empty range ends there too.
            (
                "ascending, an empty checkpoint range at the checkpoint after the tip",
                configured(Ascending, None, None, None, id, |req| {
                    req.start_checkpoint = Some(11);
                    req.end_checkpoint = Some(11);
                }),
                vec![],
                LedgerTip,
                boundary_at(id, 0, 10),
            ),
            // An `after` cursor past the tip that leaves nothing below `end_version` ends there,
            // complete, and one that leaves versions below it ends at the tip.
            (
                "ascending after a cursor past the tip, at end_version",
                configured(Ascending, None, Some(&at(19)), None, id, |req| {
                    req.end_version = Some(20)
                }),
                vec![],
                CheckpointBound,
                at(19),
            ),
            (
                "ascending after a cursor past the tip and past end_version",
                configured(Ascending, None, Some(&past_tip), None, id, |req| {
                    req.end_version = Some(20)
                }),
                vec![],
                CheckpointBound,
                past_tip.clone(),
            ),
            (
                "ascending after a cursor past the tip, below end_version",
                configured(Ascending, None, Some(&at(19)), None, id, |req| {
                    req.end_version = Some(25)
                }),
                vec![],
                LedgerTip,
                at(19),
            ),
            // Past the tip, the tighter of `before` and `end_version` names the end.
            (
                "ascending after a cursor past the tip, end_version tighter than before",
                configured(Ascending, None, Some(&past_tip), Some(&at(25)), id, |req| {
                    req.end_version = Some(20)
                }),
                vec![],
                CheckpointBound,
                past_tip.clone(),
            ),
            (
                "ascending after a cursor past the tip, before tighter than end_version",
                configured(Ascending, None, Some(&past_tip), Some(&at(18)), id, |req| {
                    req.end_version = Some(20)
                }),
                vec![],
                CursorBound,
                at(18),
            ),
            (
                "ascending to end_version 0",
                configured(Ascending, None, None, None, id, |req| {
                    req.end_version = Some(0)
                }),
                vec![],
                CheckpointBound,
                edge(0),
            ),
            (
                "ascending to a before cursor",
                configured(Ascending, None, None, Some(&at(11)), id, |_| {}),
                vec![3, 5, 6, 9],
                CursorBound,
                at(11),
            ),
            (
                "descending",
                configured(Descending, None, None, None, id, |_| {}),
                vec![12, 11, 9, 6, 5, 3],
                CheckpointBound,
                edge(0),
            ),
            (
                "descending, ending on the limit",
                configured(Descending, Some(2), None, None, id, |_| {}),
                vec![12, 11],
                ItemLimit,
                at(11),
            ),
            (
                "descending to start_version 7",
                configured(Descending, None, None, None, id, |req| {
                    req.start_version = Some(7)
                }),
                vec![12, 11, 9],
                CheckpointBound,
                edge(6),
            ),
            // The object's version at the end of checkpoint 3 is 3, so the range starts at 4.
            (
                "descending to start_checkpoint 4",
                configured(Descending, None, None, None, id, |req| {
                    req.start_checkpoint = Some(4)
                }),
                vec![12, 11, 9, 6, 5],
                CheckpointBound,
                edge(3),
            ),
            (
                "descending from end_checkpoint 7",
                configured(Descending, None, None, None, id, |req| {
                    req.end_checkpoint = Some(7)
                }),
                vec![9, 6, 5, 3],
                CheckpointBound,
                boundary_at(id, 0, 6),
            ),
            (
                "descending to an after cursor",
                configured(Descending, None, Some(&at(6)), None, id, |_| {}),
                vec![12, 11, 9],
                CursorBound,
                at(6),
            ),
            (
                "descending to an after cursor at a version the object never had",
                configured(Descending, None, Some(&at(7)), None, id, |_| {}),
                vec![12, 11, 9],
                CursorBound,
                at(7),
            ),
            (
                "descending from a before cursor",
                configured(Descending, None, None, Some(&at(9)), id, |_| {}),
                vec![6, 5, 3],
                CheckpointBound,
                edge(0),
            ),
            // A `before` cursor at or below the window's lowest version reached nothing.
            (
                "descending from a before cursor below start_version",
                configured(Descending, None, None, Some(&at(5)), id, |req| {
                    req.start_version = Some(6)
                }),
                vec![],
                CheckpointBound,
                at(5),
            ),
            // Nothing is written from checkpoint 9 to the tip, so the window lies above version 12
            // and ends at the boundary above it, a cursor the server serves back.
            (
                "descending from start_checkpoint 9, above every version",
                configured(Descending, None, None, None, id, |req| {
                    req.start_checkpoint = Some(9)
                }),
                vec![],
                CheckpointBound,
                edge(12),
            ),
            (
                "descending from a before cursor at the version after the newest",
                configured(Descending, None, None, Some(&at(13)), id, |_| {}),
                all.iter().rev().copied().collect(),
                CheckpointBound,
                edge(0),
            ),
        ];
        for (name, req, versions, end, end_cursor) in cases {
            let listing = list(&kv_rpc, req).await.unwrap();
            assert_eq!(
                (listing.versions, listing.end, listing.end_cursor),
                (versions, end, end_cursor),
                "{name}"
            );
        }
        mock_server.abort();
    }

    /// The versions of `history` a listing in `ordering` lists between the positions `after`
    /// and `before` name (see [`decoded`]), among those written before `end_checkpoint`: above
    /// `after`'s version, and below `before`'s, or at or below it for a boundary.
    fn between(
        history: &[(u64, u64, ObjectStatus)],
        ordering: Ordering,
        after: Option<(u64, bool)>,
        before: Option<(u64, bool)>,
        end_checkpoint: Option<u64>,
    ) -> Vec<u64> {
        let mut versions: Vec<u64> = history
            .iter()
            .filter(|&&(checkpoint, version, _)| {
                after.is_none_or(|(after, _)| version > after)
                    && before.is_none_or(|(before, boundary)| {
                        version < before || (boundary && version == before)
                    })
                    && end_checkpoint.is_none_or(|end| checkpoint < end)
            })
            .map(|&(_, version, _)| version)
            .collect();
        if ordering == Ordering::Descending {
            versions.reverse();
        }
        versions
    }

    /// Turning back from any cursor a listing minted, an item's or the boundary at the window
    /// edge its last frame names, lists exactly the items on the other side of it in the new
    /// order, with no repeat and no gap: a cursor names a position, not a direction, and `after`
    /// and `before` keep their meaning in both orders. That holds whatever order minted the
    /// cursor, whatever `end_checkpoint` bounded the listing that minted it and the one that
    /// turns back, and every cursor a listing mints is accepted back. In particular, an
    /// ascending page followed by a descending listing before its first item's cursor lists the
    /// items before the page, newest first, and a descending page followed by an ascending
    /// listing after its last item's cursor lists the items after it.
    #[tokio::test]
    async fn test_list_object_versions_reverses_direction_from_any_cursor() {
        use Ordering::*;
        for history in [&HISTORY[..], &SPLIT_HISTORY[..], &RECREATED_HISTORY[..]] {
            let (kv_rpc, id, mock_server) = start_fixture_with_history(true, history).await;
            for minting_end in [None, Some(5), Some(8)] {
                for minting_ordering in [Ascending, Descending] {
                    // Page through the listing two items at a time, keeping every item's cursor
                    // and the last page's final cursor: a full page ends on its last item's.
                    let mut minted_cursors = vec![];
                    let mut resume: Option<Bytes> = None;
                    loop {
                        let (after, before) = match minting_ordering {
                            Ascending => (resume.as_ref(), None),
                            _ => (None, resume.as_ref()),
                        };
                        let mut req = request(id, minting_ordering, Some(2), after, before);
                        req.end_checkpoint = minting_end;
                        let listing = list(&kv_rpc, req).await.unwrap();
                        minted_cursors.extend(listing.cursors.iter().cloned());
                        if listing.end != QueryEndReason::ItemLimit {
                            minted_cursors.push(listing.end_cursor);
                            break;
                        }
                        resume = Some(listing.end_cursor);
                    }
                    for cursor in &minted_cursors {
                        let position = decoded(cursor);
                        for ordering in [Ascending, Descending] {
                            for end_checkpoint in [None, Some(5), Some(8)] {
                                for (after, before) in [(Some(cursor), None), (None, Some(cursor))]
                                {
                                    let context = format!(
                                        "history {history:?}: cursor at {position:?} minted \
                                         {minting_ordering:?} to end_checkpoint \
                                         {minting_end:?}; turning {ordering:?} to \
                                         end_checkpoint {end_checkpoint:?}, as after: {}",
                                        after.is_some()
                                    );
                                    let mut req = request(id, ordering, None, after, before);
                                    req.end_checkpoint = end_checkpoint;
                                    let listing = list(&kv_rpc, req)
                                        .await
                                        .unwrap_or_else(|e| panic!("{context}: {e:?}"));
                                    assert_eq!(
                                        listing.versions,
                                        between(
                                            history,
                                            ordering,
                                            after.map(|_| position),
                                            before.map(|_| position),
                                            end_checkpoint,
                                        ),
                                        "{context}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
            mock_server.abort();
        }
    }

    /// An empty window ends at a boundary, in either order: one above every version the object
    /// has by the served height, and one before its creation. Resuming from such a cursor, in
    /// the order that minted it or the other, lists exactly the versions on its side, with no
    /// repeat and no gap, both on the server that minted it and on one that has since served a
    /// checkpoint writing new versions, the first of them right above the object's newest: they
    /// land above the boundary.
    #[tokio::test]
    async fn test_list_object_versions_resumes_from_an_empty_window_after_new_versions() {
        use Ordering::*;
        let (kv_rpc, id, mock_server) = start_fixture_with_history(true, &SPLIT_HISTORY).await;

        // From `start_checkpoint` 9 the window lies above version 12, written in checkpoint 8;
        // to `end_checkpoint` 2 it ends before the creation, in checkpoint 2.
        let mut minted = vec![];
        for (start_checkpoint, end_checkpoint, edge) in [
            (Some(9), None, boundary_at(id, 12, SERVED_CHECKPOINT)),
            (None, Some(2), boundary_at(id, 0, 1)),
        ] {
            for ordering in [Ascending, Descending] {
                let mut req = request(id, ordering, None, None, None);
                req.start_checkpoint = start_checkpoint;
                req.end_checkpoint = end_checkpoint;
                let listing = list(&kv_rpc, req).await.unwrap();
                assert_eq!(
                    (listing.versions, &listing.end_cursor),
                    (vec![], &edge),
                    "{ordering:?} from {start_checkpoint:?} to {end_checkpoint:?}"
                );
                minted.push(listing.end_cursor);
            }
        }

        let assert_resumes =
            async |kv_rpc: &KvRpcServer,
                   served_checkpoint: u64,
                   history: &[(u64, u64, ObjectStatus)]| {
                for cursor in &minted {
                    let position = decoded(cursor);
                    for ordering in [Ascending, Descending] {
                        for (after, before) in [(Some(cursor), None), (None, Some(cursor))] {
                            let req = request(id, ordering, None, after, before);
                            let listing = list_served_at(kv_rpc, req, served_checkpoint)
                                .await
                                .unwrap();
                            assert_eq!(
                                listing.versions,
                                between(
                                    history,
                                    ordering,
                                    after.map(|_| position),
                                    before.map(|_| position),
                                    None,
                                ),
                                "served {served_checkpoint}: {ordering:?} from {position:?}, as \
                             after: {}",
                                after.is_some()
                            );
                        }
                    }
                }
            };
        assert_resumes(&kv_rpc, SERVED_CHECKPOINT, &SPLIT_HISTORY).await;

        // Checkpoint 11 writes version 13, right above the newest, then 15, and a replica serves
        // it.
        let history = [
            &SPLIT_HISTORY[..],
            &[(11, 13, ObjectStatus::Live)],
            &[(11, 15, ObjectStatus::Live)],
        ]
        .concat();
        let caught_up_mock = MockBigtableServer::new();
        let (caught_up_addr, caught_up_server) =
            caught_up_mock.start().await.expect("start mock BigTable");
        insert_history(&caught_up_mock, id, &history).await;
        let caught_up =
            start_kv_rpc_at_heights(caught_up_addr, StagesConfig::default(), true, |_| 11).await;
        assert_resumes(&caught_up, 11, &history).await;

        mock_server.abort();
        caught_up_server.abort();
    }

    /// A listing reads the version index only for the object's versions at its window's two
    /// checkpoint edges, then the `objects` table once, whatever its cursors and version bounds.
    #[tokio::test]
    async fn test_list_object_versions_reads_the_edges_and_one_range() {
        use Ordering::*;
        let (kv_rpc, id, mock, mock_server) = start_fixture_with_mock(&SPLIT_HISTORY).await;
        for ordering in [Ascending, Descending] {
            for (bounds, after, before, start_version, start_checkpoint) in [
                ("none", None, None, None, None),
                ("after", Some(cursor(id, 5)), None, None, None),
                ("before", None, Some(cursor(id, 11)), None, None),
                ("start_version", None, None, Some(6), None),
                ("start_checkpoint", None, None, None, Some(5)),
            ] {
                let context = format!("{ordering:?} {bounds}");
                let mut req = request(id, ordering, None, after.as_ref(), before.as_ref());
                req.start_version = start_version;
                req.start_checkpoint = start_checkpoint;
                mock.clear_read_rows_calls().await;
                let listing = list(&kv_rpc, req).await.unwrap();
                assert!(!listing.versions.is_empty(), "{context}");
                let calls = mock.read_rows_calls().await;
                let index_reads = calls
                    .iter()
                    .filter(|call| call.table == object_versions_by_checkpoint::NAME)
                    .count();
                assert_eq!(
                    index_reads,
                    1 + usize::from(start_checkpoint.is_some()),
                    "{context}: {calls:?}"
                );
                assert_eq!(objects_reads(&mock).await.len(), 1, "{context}: {calls:?}");
            }
        }
        mock_server.abort();
    }

    /// [`list`], failing the test if the listing takes more than 10 seconds.
    async fn list_within_deadline(kv_rpc: &KvRpcServer, req: ListObjectVersionsRequest) -> Listing {
        let context = format!("{req:?}");
        tokio::time::timeout(std::time::Duration::from_secs(10), list(kv_rpc, req))
            .await
            .unwrap_or_else(|_| panic!("listing hung: {context}"))
            .unwrap()
    }

    /// With one BigTable permit per request, every listing completes: no read that holds the
    /// permit waits on a read that waits for it. That holds for listings that seek from cursors
    /// and version bounds in either order, and with more listings at once than permits (each
    /// request has its own limiter, so they do not compete for permits).
    #[tokio::test]
    async fn test_list_object_versions_completes_with_one_permit() {
        use Ordering::*;
        let mock = MockBigtableServer::new();
        let (addr, mock_server) = mock.start().await.expect("start mock BigTable");
        let id = ObjectID::random();
        insert_history(&mock, id, &SPLIT_HISTORY).await;
        let kv_rpc =
            start_kv_rpc_with_request_concurrency(addr, StagesConfig::default(), true, 1).await;
        let at = |version| cursor(id, version);
        let mut requests = vec![];
        for ordering in [Ascending, Descending] {
            for limit in [Some(1), Some(2), None] {
                for (after, before) in [
                    (None, None),
                    (Some(at(5)), None),
                    (None, Some(at(11))),
                    (Some(cursor(id, 5)), Some(cursor(id, 12))),
                ] {
                    for (start_version, end_version) in [(None, None), (Some(6), Some(12))] {
                        let mut req = request(id, ordering, limit, after.as_ref(), before.as_ref());
                        req.start_version = start_version;
                        req.end_version = end_version;
                        requests.push(req);
                    }
                }
            }
        }
        for req in requests.iter().cloned() {
            list_within_deadline(&kv_rpc, req).await;
        }
        let listings = futures::future::join_all(
            requests
                .iter()
                .cloned()
                .map(|req| list_within_deadline(&kv_rpc, req)),
        )
        .await;
        assert_eq!(listings.len(), requests.len());
        mock_server.abort();
    }

    /// A read mask that asks only for `object_id` and `version` lists the same items from live
    /// rows' keys alone: every `objects` read empties live objects' values, and nothing else, so
    /// removals still carry their kind. Any other field reads the objects.
    #[tokio::test]
    async fn test_list_object_versions_version_only_masks_read_keys() {
        let (kv_rpc, id, mock, mock_server) = start_fixture_with_mock(&HISTORY).await;

        for (paths, keys_only) in [
            (vec!["object_id", "version"], true),
            (vec!["version"], true),
            (vec!["object_id", "version", "digest"], false),
        ] {
            for ordering in [Ordering::Ascending, Ordering::Descending] {
                let context = format!("{paths:?} {ordering:?}");
                let mut req = request(id, ordering, None, None, None);
                req.read_mask = Some(mask(&paths));
                mock.clear_read_rows_calls().await;
                let frames: Vec<ListObjectVersionsResponse> =
                    list_object_versions(&kv_rpc, kv_rpc.admitted_request(req.clone()))
                        .await
                        .unwrap()
                        .try_collect()
                        .await
                        .unwrap();
                for object in frames.iter().filter_map(|frame| frame.object_opt()) {
                    assert_eq!(object.digest.is_some(), !keys_only, "{context}");
                }
                let reads = objects_reads(&mock).await;
                // One range scan, limited to the listing's limit (the default, 50).
                assert_eq!(reads.len(), 1, "{context}: {reads:?}");
                assert_eq!(reads[0].rows_limit, 50, "{context}");
                let object_column = Bytes::from_static(objects::col::OBJECT.as_bytes());
                let stripped: &[Bytes] = if keys_only { &[object_column] } else { &[] };
                assert!(
                    reads
                        .iter()
                        .all(|read| !read.values_stripped && read.values_stripped_in == stripped),
                    "{context}: {reads:?}"
                );

                let listing = list(&kv_rpc, req).await.unwrap();
                let mut items = listing.items;
                if ordering == Ordering::Descending {
                    items.reverse();
                }
                assert_eq!(
                    items,
                    [
                        (3, ObjectStatus::Live, None),
                        (5, ObjectStatus::Live, None),
                        (8, ObjectStatus::Wrapped, None),
                        (9, ObjectStatus::Live, None),
                        (12, ObjectStatus::Live, None),
                        (14, ObjectStatus::Deleted, None),
                    ],
                    "{context}"
                );
            }
        }
        mock_server.abort();
    }

    /// Each removal carries the fields `removal_read_mask` selects, `version,kind` by default,
    /// and its frame its cursor whatever the mask. With `transaction` selected, every
    /// removal carries its removing transaction, and a tombstone without one fails the listing
    /// with `INTERNAL` once the listing reaches it; without it, such a tombstone lists like any
    /// other.
    #[tokio::test]
    async fn test_list_object_versions_renders_the_removal_read_mask() {
        let (kv_rpc, id, mock, mock_server) = start_fixture_with_mock(&HISTORY).await;
        // The deletion's tombstone records its transaction; the wrap's lacks one.
        let deletion = TransactionDigest::random();
        mock.insert_row(
            objects::NAME,
            objects::encode_key(&ObjectKey(id, SequenceNumber::from_u64(14))),
            [(
                objects::col::TRANSACTION,
                Bytes::copy_from_slice(deletion.inner()),
            )],
        )
        .await;
        let removals = async |removal_read_mask: Option<&[&str]>, start_version| {
            let mut req = request(id, Ordering::Ascending, None, None, None);
            req.removal_read_mask = removal_read_mask.map(mask);
            req.start_version = start_version;
            let frames: Vec<ListObjectVersionsResponse> =
                list_object_versions(&kv_rpc, kv_rpc.admitted_request(req))
                    .await?
                    .try_collect()
                    .await?;
            Ok::<_, tonic::Status>(
                frames
                    .into_iter()
                    .filter_map(|frame| match frame.item {
                        Some(Item::Removal(removal)) => Some((frame.cursor?, removal)),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            )
        };
        let removal = |version: Option<u64>, kind: Option<object_removal::Kind>, transaction| {
            let mut removal = ObjectRemoval::default();
            removal.version = version;
            removal.kind = kind.map(Into::into);
            removal.transaction = transaction;
            removal
        };
        use object_removal::Kind::*;

        for (removal_read_mask, expected) in [
            (
                None,
                [
                    removal(Some(8), Some(Wrapped), None),
                    removal(Some(14), Some(Deleted), None),
                ],
            ),
            (
                Some(&["kind"][..]),
                [
                    removal(None, Some(Wrapped), None),
                    removal(None, Some(Deleted), None),
                ],
            ),
            (
                Some(&["version"][..]),
                [removal(Some(8), None, None), removal(Some(14), None, None)],
            ),
            // A mask that selects nothing leaves each removal empty, still on its own frame with
            // its cursor.
            (
                Some(&[][..]),
                [removal(None, None, None), removal(None, None, None)],
            ),
        ] {
            let listed = removals(removal_read_mask, None).await.unwrap();
            assert_eq!(
                listed,
                [
                    (cursor(id, 8), expected[0].clone()),
                    (cursor(id, 14), expected[1].clone())
                ],
                "{removal_read_mask:?}"
            );
        }

        let all = &["version", "kind", "transaction"][..];
        assert_eq!(
            removals(Some(all), Some(9)).await.unwrap(),
            [(
                cursor(id, 14),
                removal(Some(14), Some(Deleted), Some(deletion.to_string()))
            )],
        );
        let status = removals(Some(all), None).await.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Internal, "{status:?}");
        assert!(
            status.message().contains("at version 8 has no transaction"),
            "{status:?}"
        );
        mock_server.abort();
    }

    /// Each item frame's encoded size is recorded, and so is each terminal-only frame: a listing
    /// that ends before its limit, or lists nothing, ends with a frame that carries no item, only
    /// its cursor and end, and one that fills its limit ends on its last item.
    #[tokio::test]
    async fn test_list_object_versions_records_item_and_terminal_frames() {
        let (kv_rpc, id, mock_server) = start_fixture(true).await;
        let mut before_creation = request(id, Ordering::Ascending, None, None, None);
        before_creation.end_checkpoint = Some(2);
        // The items each request lists, and the item and terminal-only frames recorded by the
        // end of it.
        for (req, listed, item_frames, terminal_frames) in [
            (request(id, Ordering::Ascending, None, None, None), 6, 6, 1),
            (
                request(id, Ordering::Ascending, Some(2), None, None),
                2,
                8,
                1,
            ),
            (before_creation, 0, 8, 2),
        ] {
            let context = format!("{req:?}");
            let listing = list(&kv_rpc, req).await.unwrap();
            assert_eq!(listing.versions.len(), listed, "{context}");
            let mut families = kv_rpc.metrics.response_page_bytes.collect();
            families.extend(kv_rpc.metrics.stream_watermark_frames_total.collect());
            let page_bytes = list_histogram(
                &families,
                "kv_rpc_response_page_bytes",
                OPERATION,
                "objects",
            );
            assert_eq!(page_bytes.get_sample_count(), item_frames, "{context}");
            let watermark_frames =
                list_counter(&families, "kv_rpc_stream_watermark_frames_total", OPERATION);
            assert_eq!(
                watermark_frames.value(),
                terminal_frames as f64,
                "{context}"
            );
        }
        mock_server.abort();
    }
}
