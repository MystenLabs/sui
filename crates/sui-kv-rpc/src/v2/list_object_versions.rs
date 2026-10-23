// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_rpc::proto::sui::rpc::v2::ListObjectVersionsRequest;
use sui_rpc::proto::sui::rpc::v2::ListObjectVersionsResponse;
use sui_rpc_api::RpcError;
use sui_rpc_api::grpc::v2::ledger_service::ObjectVersionsQuery;

use crate::KvRpcServer;
use crate::render::object_to_response;

/// Serve `ListObjectVersions` from the `objects` table. Every listing is bounded by a checkpoint:
/// `at_checkpoint`, or the served height when it is unset. `objects` rows above the served height
/// can be written out of checkpoint order, so a listing past it could list a version and skip an
/// older one still being written, which a client paging on from the newer version's cursor would
/// never see. The object's version at the end of that checkpoint, the request's version range,
/// and its cursors fold into one inclusive version range, from which one page of live versions
/// is read; a range with no versions left is answered without reading it.
///
/// The checkpoint bound reads the `object_versions_by_checkpoint` index, so every request needs
/// `enable-object-at-checkpoint`. Tombstones are not needed: tombstone rows are skipped, so live
/// versions are listed correctly whether or not they are backfilled.
pub(crate) async fn list_object_versions(
    server: &KvRpcServer,
    request: ListObjectVersionsRequest,
) -> Result<ListObjectVersionsResponse, RpcError> {
    let query = ObjectVersionsQuery::new(&request)?;
    let client = server.limited_client("ListObjectVersions");

    // Both checks fail the request whatever the object's history, so they come before any read.
    let reads = server.at_checkpoint_reads(client.clone())?;
    let checkpoint = match query.at_checkpoint {
        Some(checkpoint) => {
            reads.check_indexed(checkpoint)?;
            checkpoint
        }
        None => reads.served_height.checkpoint,
    };
    if query.versions.is_empty() {
        return Ok(ListObjectVersionsResponse::default());
    }
    // The object's version at the end of the checkpoint, even one that deleted or wrapped it, is
    // the newest version written by then. An object that did not exist yet has none.
    let Some((newest_visible, _)) = reads
        .version_at_checkpoint(query.object_id, checkpoint)
        .await?
    else {
        return Ok(ListObjectVersionsResponse::default());
    };
    let Some(versions) = query.version_range(newest_visible) else {
        return Ok(ListObjectVersionsResponse::default());
    };

    let objects = client
        .get_live_object_versions(query.object_id, versions, query.limit, query.descending)
        .await?;
    let mut listed = Vec::with_capacity(objects.len());
    for object in &objects {
        let rendered = object_to_response(object, &query.read_mask, &server.package_resolver).await;
        listed.push(query.listed_version(rendered, object.version()));
    }
    Ok(ListObjectVersionsResponse::default().with_versions(listed))
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use prost::Message;
    use sui_kvstore::ObjectStatus;
    use sui_kvstore::testing::MockBigtableServer;
    use sui_rpc::proto::google::rpc::BadRequest;
    use sui_rpc::proto::sui::rpc::v2::Ordering;
    use sui_rpc::proto::sui::rpc::v2::QueryOptions;
    use sui_rpc_api::grpc::v2::ledger_service::ObjectVersionCursor;
    use sui_types::base_types::ObjectID;

    use super::*;
    use crate::StagesConfig;
    use crate::v2::test_utils::insert_history;
    use crate::v2::test_utils::start_kv_rpc;

    /// `(checkpoint, version, status)` for each change to the fixture object: created at version
    /// 3 in checkpoint 2, mutated at 5 in checkpoint 3, wrapped at 8 in checkpoint 5, unwrapped at
    /// 9 in checkpoint 6, mutated at 12 in checkpoint 7, and deleted at 14 in checkpoint 9. Its
    /// live versions are 3, 5, 9 and 12.
    const HISTORY: [(u64, u64, ObjectStatus); 6] = [
        (2, 3, ObjectStatus::Live),
        (3, 5, ObjectStatus::Live),
        (5, 8, ObjectStatus::Wrapped),
        (6, 9, ObjectStatus::Live),
        (7, 12, ObjectStatus::Live),
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
        let kv_rpc = start_kv_rpc(
            addr,
            StagesConfig::default(),
            enable_object_at_checkpoint,
            false,
        )
        .await;
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

    /// The versions `req` lists, in order, checking that each listed object is the requested one
    /// and carries the cursor of its version.
    async fn list(
        kv_rpc: &KvRpcServer,
        req: ListObjectVersionsRequest,
    ) -> Result<Vec<u64>, tonic::Status> {
        let requested = req.object_id.clone();
        let response = list_object_versions(kv_rpc, req).await?;
        Ok(response
            .versions
            .iter()
            .map(|listed| {
                let object = listed.object.as_ref().expect("object is set");
                let version = object.version.expect("version is set");
                assert_eq!(object.object_id, requested);
                assert!(
                    object.digest.is_some(),
                    "the default read mask renders digest"
                );
                let id: ObjectID = requested.as_deref().unwrap().parse().unwrap();
                assert_eq!(
                    listed.cursor,
                    Some(cursor(id, version)),
                    "version {version}"
                );
                version
            })
            .collect())
    }

    /// The cursor a full ascending listing gives `version`.
    async fn listed_cursor(kv_rpc: &KvRpcServer, id: ObjectID, version: u64) -> Bytes {
        let response =
            list_object_versions(kv_rpc, request(id, Ordering::Ascending, None, None, None))
                .await
                .unwrap();
        response
            .versions
            .into_iter()
            .find(|listed| listed.object.as_ref().unwrap().version == Some(version))
            .and_then(|listed| listed.cursor)
            .unwrap_or_else(|| panic!("version {version} is not listed"))
    }

    /// Every request bound, alone and combined, lists the live versions of [`HISTORY`] inside all
    /// of them, in either order, up to the limit: tombstones are skipped, and `at_checkpoint`
    /// keeps the versions written by the end of the checkpoint, none before creation.
    #[tokio::test]
    async fn test_list_object_versions_intersects_every_bound() {
        let (kv_rpc, id, mock_server) = start_fixture(true).await;

        let expected = |start: Option<u64>,
                        end: Option<u64>,
                        after: Option<u64>,
                        before: Option<u64>,
                        at_checkpoint: Option<u64>,
                        ordering: Ordering,
                        limit: Option<u32>| {
            let mut versions: Vec<u64> = HISTORY
                .iter()
                .filter(|&&(checkpoint, version, status)| {
                    status == ObjectStatus::Live
                        && start.is_none_or(|start| version >= start)
                        && end.is_none_or(|end| version < end)
                        && after.is_none_or(|after| version > after)
                        && before.is_none_or(|before| version < before)
                        && at_checkpoint.is_none_or(|bound| checkpoint <= bound)
                })
                .map(|&(_, version, _)| version)
                .collect();
            if ordering == Ordering::Descending {
                versions.reverse();
            }
            versions.truncate(limit.map_or(usize::MAX, |limit| limit as usize));
            versions
        };

        // Bounds on either side of live versions and tombstones (8 is the wrap), and checkpoints
        // before creation, at it, between changes, at the wrap, and past the deletion.
        for start in [None, Some(5), Some(10)] {
            for end in [None, Some(9), Some(13)] {
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
                                    assert_eq!(
                                        list(&kv_rpc, req).await.unwrap(),
                                        expected(
                                            start,
                                            end,
                                            after,
                                            before,
                                            at_checkpoint,
                                            ordering,
                                            limit
                                        ),
                                        "versions [{start:?}, {end:?}) after {after:?} before \
                                         {before:?} at_checkpoint {at_checkpoint:?} {ordering:?} \
                                         limit {limit:?}",
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // An object with no versions lists none.
        let other = ObjectID::random();
        for ordering in [Ordering::Ascending, Ordering::Descending] {
            let listed = list(&kv_rpc, request(other, ordering, None, None, None)).await;
            assert_eq!(listed.unwrap(), Vec::<u64>::new(), "{ordering:?}");
        }

        mock_server.abort();
    }

    /// `at_checkpoint` excludes the versions written after the checkpoint, and a descending page
    /// under it starts at the newest version written by then and still fills up.
    #[tokio::test]
    async fn test_list_object_versions_at_checkpoint() {
        let (kv_rpc, id, mock_server) = start_fixture(true).await;

        for (at_checkpoint, ascending) in [
            (1, vec![]),
            (2, vec![3]),
            (4, vec![3, 5]),
            // Wrapped at version 8 in checkpoint 5: no live version is added.
            (5, vec![3, 5]),
            (6, vec![3, 5, 9]),
            (10, vec![3, 5, 9, 12]),
        ] {
            let mut req = request(id, Ordering::Ascending, None, None, None);
            req.at_checkpoint = Some(at_checkpoint);
            assert_eq!(
                list(&kv_rpc, req).await.unwrap(),
                ascending,
                "{at_checkpoint}"
            );

            let mut req = request(id, Ordering::Descending, Some(2), None, None);
            req.at_checkpoint = Some(at_checkpoint);
            let descending: Vec<u64> = ascending.iter().rev().take(2).copied().collect();
            assert_eq!(
                list(&kv_rpc, req).await.unwrap(),
                descending,
                "{at_checkpoint}"
            );
        }

        mock_server.abort();
    }

    /// A backward page from the first version a client holds, followed by forward pages from
    /// that page's cursors, lists every live version with no gaps or repeats, each direction
    /// ending with a short page. Cursors from descending pages bound ascending ones.
    #[tokio::test]
    async fn test_list_object_versions_pages_backward_then_forward_from_cursors() {
        let (kv_rpc, id, mock_server) = start_fixture(true).await;
        let page = |ordering, after: Option<Bytes>, before: Option<Bytes>| {
            let req = request(id, ordering, Some(2), after.as_ref(), before.as_ref());
            list_object_versions(&kv_rpc, req)
        };
        let versions = |response: &ListObjectVersionsResponse| -> Vec<u64> {
            response
                .versions
                .iter()
                .map(|listed| listed.object.as_ref().unwrap().version.unwrap())
                .collect()
        };
        let cursor_at = |response: &ListObjectVersionsResponse, index: usize| {
            response.versions[index].cursor.clone()
        };

        // The client holds the page after version 5.
        let held = page(
            Ordering::Ascending,
            Some(listed_cursor(&kv_rpc, id, 5).await),
            None,
        )
        .await
        .unwrap();
        assert_eq!(versions(&held), [9, 12]);

        // Backward from the first version held, until a short page reaches the start.
        let back = page(Ordering::Descending, None, cursor_at(&held, 0))
            .await
            .unwrap();
        assert_eq!(versions(&back), [5, 3]);
        let start = page(Ordering::Descending, None, cursor_at(&back, 1))
            .await
            .unwrap();
        assert_eq!(versions(&start), Vec::<u64>::new());

        // Forward from the backward page's lowest version, until a short page reaches the end.
        let forward = page(Ordering::Ascending, cursor_at(&back, 1), None)
            .await
            .unwrap();
        assert_eq!(versions(&forward), [5, 9]);
        let end = page(Ordering::Ascending, cursor_at(&forward, 1), None)
            .await
            .unwrap();
        assert_eq!(versions(&end), [12]);

        mock_server.abort();
    }

    /// `after` and `before` together list the versions strictly between them, the lowest first
    /// ascending and the highest first descending, and nothing when they leave no room.
    #[tokio::test]
    async fn test_list_object_versions_between_two_cursors() {
        let (kv_rpc, id, mock_server) = start_fixture(true).await;
        let mut cursors = std::collections::BTreeMap::new();
        for version in [3, 5, 9, 12] {
            cursors.insert(version, listed_cursor(&kv_rpc, id, version).await);
        }

        for (after, before, limit, ascending, descending) in [
            (3, 12, None, vec![5, 9], vec![9, 5]),
            (3, 12, Some(1), vec![5], vec![9]),
            (5, 9, None, vec![], vec![]),
            (9, 9, None, vec![], vec![]),
            (12, 3, None, vec![], vec![]),
        ] {
            for (ordering, expected) in [
                (Ordering::Ascending, ascending),
                (Ordering::Descending, descending),
            ] {
                let req = request(
                    id,
                    ordering,
                    limit,
                    Some(&cursors[&after]),
                    Some(&cursors[&before]),
                );
                assert_eq!(
                    list(&kv_rpc, req).await.unwrap(),
                    expected,
                    "after {after} before {before} limit {limit:?} {ordering:?}"
                );
            }
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

    /// Malformed requests are `InvalidArgument` naming the field, whether or not `at_checkpoint`
    /// reads are enabled. `at_checkpoint` above the served height is `NotFound`.
    #[tokio::test]
    async fn test_list_object_versions_rejects_invalid_requests() {
        let (kv_rpc, id, mock_server) = start_fixture(true).await;

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
        let mut invalid_ordering = ascending();
        invalid_ordering.options.as_mut().unwrap().ordering = Some(7);

        for (req, field, message) in [
            (missing_object_id, "object_id", "missing object_id"),
            (invalid_object_id, "object_id", "invalid object_id"),
            (invalid_read_mask, "read_mask", "invalid read_mask path"),
            (invalid_ordering, "options.ordering", "invalid ordering"),
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
            let status = list(&kv_rpc, req).await.unwrap_err();
            assert_eq!(status.code(), tonic::Code::InvalidArgument, "{message}");
            assert!(
                status.message().contains(message),
                "expected {message:?}, got {:?}",
                status.message()
            );
            assert_eq!(bad_request_fields(&status), [field], "{message}");
        }

        // The served height is 10, so checkpoint 11 is not yet indexed, even for a request whose
        // version bounds leave nothing to list.
        for start_version in [None, Some(100)] {
            let mut req = ascending();
            req.at_checkpoint = Some(11);
            req.start_version = start_version;
            let status = list(&kv_rpc, req).await.unwrap_err();
            assert_eq!(status.code(), tonic::Code::NotFound, "{status:?}");
            assert!(
                status.message().contains("is not yet indexed"),
                "{status:?}"
            );
        }

        mock_server.abort();
    }

    /// Every listing reads the version index to bound itself by a checkpoint, so without
    /// `at_checkpoint` reads enabled every request is `Unimplemented`, whether or not it sets
    /// `at_checkpoint`, and even when its version bounds leave nothing to list.
    #[tokio::test]
    async fn test_list_object_versions_needs_the_at_checkpoint_flag() {
        let (kv_rpc, id, mock_server) = start_fixture(false).await;

        for at_checkpoint in [None, Some(5)] {
            for start_version in [None, Some(100)] {
                let mut req = request(id, Ordering::Ascending, None, None, None);
                req.at_checkpoint = at_checkpoint;
                req.start_version = start_version;
                let status = list(&kv_rpc, req).await.unwrap_err();
                assert_eq!(status.code(), tonic::Code::Unimplemented, "{status:?}");
            }
        }

        mock_server.abort();
    }

    /// Without `at_checkpoint`, a listing is bounded by the served height: `objects` rows above
    /// it can be written out of checkpoint order, and a version listed past it could hide an
    /// older one still being written from a client paging on from its cursor.
    #[tokio::test]
    async fn test_list_object_versions_stops_at_the_served_height() {
        // Above the served height of 10, version 20 from checkpoint 12 is written while the
        // version from checkpoint 11 is still being written.
        let mut history = HISTORY.to_vec();
        history.push((12, 20, ObjectStatus::Live));
        let (kv_rpc, id, mock_server) = start_fixture_with_history(true, &history).await;

        // Cursors on either side of version 20 do not reach it either.
        let after_12 = cursor(id, 12);
        let before_21 = cursor(id, 21);
        for (ordering, limit, after, before, expected) in [
            (Ordering::Ascending, None, None, None, vec![3, 5, 9, 12]),
            (Ordering::Descending, Some(1), None, None, vec![12]),
            (Ordering::Ascending, None, Some(&after_12), None, vec![]),
            (
                Ordering::Descending,
                Some(1),
                None,
                Some(&before_21),
                vec![12],
            ),
        ] {
            let req = request(id, ordering, limit, after, before);
            assert_eq!(
                list(&kv_rpc, req).await.unwrap(),
                expected,
                "{ordering:?} after {after:?} before {before:?}"
            );
        }

        mock_server.abort();
    }
}
