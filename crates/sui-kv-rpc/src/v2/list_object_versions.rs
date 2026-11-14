// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Instant;

use futures::StreamExt;
use futures::TryStreamExt;
use futures::stream;
use prost::Message;
use sui_kvstore::tables::objects::ObjectTombstone;
use sui_kvstore::tables::objects::ObjectVersionRow;
use sui_rpc::proto::sui::rpc::v2::ListObjectVersionsRequest;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use sui_rpc_api::AtCheckpointUpperBound;
use sui_rpc_api::RpcError;
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
use crate::v2::get_object::removal_kind;

/// The operation name ListObjectVersions' deadline, limiter, metrics and logs are labelled with.
pub(super) const OPERATION: &str = "list_object_versions";

/// Serve `ListObjectVersions` from the `objects` table, listing live rows as live versions and
/// tombstones as removals. Every listing is bounded by a checkpoint: `at_checkpoint`, or the
/// served height when it is unset. `objects` rows above the served height can be written out of
/// checkpoint order, so a listing past it could list a version and skip an older one still being
/// written, which a client paging on from the newer version's cursor would never see. The
/// object's version at the end of that checkpoint, the request's version range, and its cursors
/// fold into one inclusive version range (see [`ObjectVersionsQuery::plan`]), read as one range
/// scan of up to `limit` rows whose items stream to the client as they arrive. A read mask that
/// needs only `object_id` and `version` reads live rows' keys alone; any other is rendered from
/// the objects, `render-ahead` items at a time.
///
/// The checkpoint bound reads the `object_versions_by_checkpoint` index, so every request needs
/// `enable-object-at-checkpoint`, which also requires the tombstones that list removals to be
/// backfilled from genesis.
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
    let checkpoint = query.bounding_checkpoint(&upper_bound)?;
    let ctx = server.query_context(OPERATION, client.clone(), reads.served_height)?;
    let object_id = query.object_id;
    let read_mask = query.read_mask.clone();
    let lists_removal_transactions = query.lists_removal_transactions();
    let with_contents = !needs_only_object_id_and_version(&read_mask);
    let resolution = if with_contents { "objects" } else { "versions" };
    let observed = |frames| observe_frames(ctx.clone(), resolution, &query, started, frames);
    let newest_visible = reads
        .version_at_checkpoint(query.object_id, checkpoint)
        .await?
        .map(|(version, _)| version);
    let plan = query.plan(&upper_bound, newest_visible)?;
    let Some(versions) = plan.versions else {
        return Ok(observed(
            query.frames(stream::empty().boxed(), Ok(plan.end)),
        ));
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
    Ok(observed(query.frames(listed, Ok(plan.end))))
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
            if frame.cursor.is_some() {
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
        kind: removal_kind(tombstone.status),
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

    fn cursor(id: ObjectID, version: u64) -> Bytes {
        ObjectVersionCursor::Version {
            object_id: id,
            version,
        }
        .encode()
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
        cursors: Vec<Bytes>,
        end: QueryEndReason,
    }

    /// Everything `req`'s stream sends, checking the frames' shape: each item frame carries
    /// exactly one of `object` and `removal` and the cursor of its version, each listed object is
    /// the requested one, rendered with `digest` under the default read mask, each removal is
    /// deleted or wrapped, and `end` is set exactly once, on the final frame, which carries an
    /// item only for `ITEM_LIMIT`.
    async fn list(
        kv_rpc: &KvRpcServer,
        req: ListObjectVersionsRequest,
    ) -> Result<Listing, tonic::Status> {
        let requested = req.object_id.clone();
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
            end: QueryEndReason::Unknown,
        };
        for (index, frame) in frames.iter().enumerate() {
            let last = index + 1 == frames.len();
            assert_eq!(
                frame.end.is_some(),
                last,
                "end on frame {index} of {frames:?}"
            );
            let (version, status, transaction) = match (&frame.object, &frame.removal) {
                (Some(object), None) => {
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
                (None, Some(removal)) => {
                    let status = match removal.kind() {
                        object_removal::Kind::Deleted => ObjectStatus::Deleted,
                        object_removal::Kind::Wrapped => ObjectStatus::Wrapped,
                        kind => panic!("removal of kind {kind:?} in {frames:?}"),
                    };
                    let version = removal.version.expect("removal version is set");
                    (version, status, removal.transaction.clone())
                }
                (None, None) => {
                    assert!(last && frame.cursor.is_none(), "{frames:?}");
                    continue;
                }
                (Some(_), Some(_)) => panic!("frame {index} has an object and a removal"),
            };
            let listed_cursor = frame.cursor.clone().expect("cursor is set");
            assert_eq!(listed_cursor, cursor(id, version), "version {version}");
            listing.versions.push(version);
            listing.items.push((version, status, transaction));
            listing.cursors.push(listed_cursor);
        }
        let last = frames.last().expect("a stream ends with a frame");
        listing.end = last
            .end
            .and_then(|end| end.reason)
            .map(|reason| QueryEndReason::try_from(reason).unwrap())
            .expect("the final frame has a reason");
        assert_eq!(
            last.object.is_some() || last.removal.is_some(),
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
    /// versions and removals: `at_checkpoint` keeps the versions written by the end of the
    /// checkpoint, none before creation. A listing that fills its limit ends with `ITEM_LIMIT`,
    /// on a live version or a removal, and one that does not, with another reason.
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
                        at_checkpoint: Option<u64>,
                        ordering: Ordering,
                        limit: Option<u32>| {
            let mut items: Vec<(u64, ObjectStatus)> = history
                .iter()
                .filter(|&&(checkpoint, version, _)| {
                    start.is_none_or(|start| version >= start)
                        && end.is_none_or(|end| version < end)
                        && after.is_none_or(|after| version > after)
                        && before.is_none_or(|before| version < before)
                        && at_checkpoint.is_none_or(|bound| checkpoint <= bound)
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
        // checkpoints before creation, at it, between changes, at version 8, and past the end.
        for start in [None, Some(5), Some(10)] {
            for end in [None, Some(9), Some(13)] {
                // A range that ends below its start is rejected (see
                // `test_list_object_versions_rejects_invalid_requests`).
                if end < start && end.is_some() {
                    continue;
                }
                for after in [None, Some(5), Some(8)] {
                    for before in [None, Some(9), Some(14)] {
                        for at_checkpoint in [None, Some(1), Some(2), Some(4), Some(5), Some(10)] {
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
                                    req.at_checkpoint = at_checkpoint;
                                    let context = format!(
                                        "versions [{start:?}, {end:?}) after {after:?} before \
                                         {before:?} at_checkpoint {at_checkpoint:?} {ordering:?} \
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
                                            at_checkpoint,
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
    /// reads are enabled. `at_checkpoint` above the served height is `OutOfRange`.
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

        // The served height is 10, so checkpoint 11 is not yet indexed, even for a request whose
        // version bounds leave nothing to list.
        for start_version in [None, Some(100)] {
            let mut req = ascending();
            req.at_checkpoint = Some(11);
            req.start_version = start_version;
            let status = list(&kv_rpc, req).await.unwrap_err();
            assert_eq!(status.code(), tonic::Code::OutOfRange, "{status:?}");
            assert_eq!(
                status.message(),
                "requested data above latest available; at_checkpoint is 11, and only \
                 checkpoints below 11 are available",
            );
        }

        mock_server.abort();
        disabled_server.abort();
    }

    /// Every listing reads the version index to bound itself by a checkpoint, so without
    /// `at_checkpoint` reads enabled every request is `Unimplemented` / `NOT_SERVED_HERE` with
    /// cause `disabled_by_config`, whether or not it sets `at_checkpoint`, even one above the
    /// served height of 10, and even when its version bounds leave nothing to list.
    #[tokio::test]
    async fn test_list_object_versions_needs_the_at_checkpoint_flag() {
        let (kv_rpc, id, mock_server) = start_fixture(false).await;

        for at_checkpoint in [None, Some(5), Some(11)] {
            for start_version in [None, Some(100)] {
                let mut req = request(id, Ordering::Ascending, None, None, None);
                req.at_checkpoint = at_checkpoint;
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

    /// Without `at_checkpoint`, a listing is bounded by the served height: `objects` rows above
    /// it can be written out of checkpoint order, and a version listed past it could hide an
    /// older one still being written from a client paging on from its cursor. A cursor above
    /// the object's version at the served height, such as one from a replica that has indexed
    /// further, follows the ledger List APIs: descending, it is `OutOfRange` even when the
    /// version bounds leave nothing to list, because listing from this replica's view would
    /// start below versions it lacks; ascending, the listing is served from this replica's view.
    /// An `after` past the tip then lists nothing and ends with `LEDGER_TIP` in a window that
    /// reaches past the tip, or with `CURSOR_BOUND` against a `before` at the version after it,
    /// and a `before` past the tip leaves the tip as the end. `at_checkpoint` caps the listing
    /// at an indexed checkpoint, so it accepts any cursor.
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
        let cases: [(Bounds, Option<Listed>); 17] = [
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
                Some((all, LedgerTip)),
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
            // A window reaching past the tip ends at `after`, even below the cursor.
            (
                (Ascending, None, Some(18), Some(20), None, None, None),
                Some((vec![], LedgerTip)),
            ),
            (
                (Ascending, None, Some(21), Some(20), None, None, None),
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
            // `at_checkpoint` caps the listing at checkpoint 10 whatever the cursor.
            (
                (Descending, None, None, None, Some(21), Some(10), Some(1)),
                Some((vec![14], ItemLimit)),
            ),
            (
                (Ascending, None, None, Some(20), None, Some(10), None),
                Some((vec![], CheckpointBound)),
            ),
        ];
        for ((ordering, start, end, after, before, at_checkpoint, limit), expected) in cases {
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
            req.at_checkpoint = at_checkpoint;
            let context = format!(
                "{ordering:?} [{start:?}, {end:?}) after {after:?} before {before:?} \
                 at_checkpoint {at_checkpoint:?} limit {limit:?}"
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
    /// ends it with `CURSOR_BOUND`; an ascending listing without `at_checkpoint` that reaches the
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
                (Ascending, None, None, None, None, Some(7), None),
                vec![3, 5, 8, 9, 12],
                CheckpointBound,
            ),
            (
                (Ascending, None, None, None, None, Some(5), None),
                vec![3, 5, 8],
                CheckpointBound,
            ),
            (
                (Ascending, None, None, None, None, Some(1), None),
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
                (Descending, None, None, None, None, Some(4), None),
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
                    (Ascending, None, None, Some(12), None, Some(10), None),
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
                    (Ascending, None, None, None, None, Some(3), None),
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
                    (Ascending, None, None, None, None, Some(3), None),
                    vec![],
                    CheckpointBound,
                ),
                (
                    (Ascending, None, None, None, None, Some(4), None),
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

    /// `(ordering, start_version, end_version, after, before, at_checkpoint, limit)`.
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
        for &((ordering, start, end, after, before, at_checkpoint, limit), ref versions, reason) in
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
            req.at_checkpoint = at_checkpoint;
            let listing = list(kv_rpc, req).await.unwrap();
            assert_eq!(
                (&listing.versions, listing.end),
                (versions, reason),
                "{ordering:?} [{start:?}, {end:?}) after {after:?} before {before:?} \
                 at_checkpoint {at_checkpoint:?} limit {limit:?}",
            );
        }
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
                for object in frames.iter().filter_map(|frame| frame.object.as_ref()) {
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
    /// and its frame its cursor whatever the mask. With `transaction` selected, every removal
    /// carries its removing transaction, and a tombstone without one fails the listing with
    /// `INTERNAL` once the listing reaches it; without it, such a tombstone lists like any other.
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
                    .filter_map(|frame| Some((frame.cursor?, frame.removal?)))
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
    /// that ends before its limit, or lists nothing, ends with a frame of neither item nor
    /// cursor, and one that fills its limit ends on its last item.
    #[tokio::test]
    async fn test_list_object_versions_records_item_and_terminal_frames() {
        let (kv_rpc, id, mock_server) = start_fixture(true).await;
        let mut before_creation = request(id, Ordering::Ascending, None, None, None);
        before_creation.at_checkpoint = Some(1);
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
