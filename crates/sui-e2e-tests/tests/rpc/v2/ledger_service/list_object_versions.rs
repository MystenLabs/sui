// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use mysten_common::ZipDebugEqIteratorExt;
use prost::bytes::Bytes;
use sui_config::RpcConfig;
use sui_config::rpc_config::LedgerHistoryConfig;
use sui_config::rpc_config::LedgerHistoryMethodConfig;
use sui_macros::sim_test;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::ErrorReason;
use sui_rpc::proto::sui::rpc::v2::GetObjectRequest;
use sui_rpc::proto::sui::rpc::v2::ListObjectVersionsRequest;
use sui_rpc::proto::sui::rpc::v2::Ordering;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2::list_object_versions_response::Item;
use sui_rpc::proto::sui::rpc::v2::object_removal::Kind;
use sui_rpc_api::ledger_history::object_versions::ObjectVersionCursor;
use sui_sdk_types::Address;
use sui_types::SUI_CLOCK_OBJECT_ID;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::effects::IDOperation;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Owner;
use sui_types::storage::ObjectOrTombstone;
use sui_types::transaction::CallArg;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::SharedObjectMutability;
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use tonic::transport::Channel;

use super::get_object::call_object_basics;
use super::get_object::created_object;
use super::get_object::execute;
use super::get_object::output_ref;
use super::get_object::owned_arg;
use super::get_object::pure_arg;
use super::get_object::tx_builder;
use crate::v2::RpcStatusExt;
use crate::v2::assert_above_latest_available;
use crate::v2::assert_below_earliest_available;
use crate::v2::error_info;
use crate::v2::fullnode_highest_executed;
use crate::v2::halt_chain;

/// An object's items through creation, mutation, wrapping, unwrapping, a second mutation and
/// deletion, its live versions and the removals at the wrap and the deletion, each removal with
/// the transaction that made it: listed in both orders, paged by cursor, bounded by version range,
/// cursors and checkpoint range, with the `QueryEnd` reason each bound ends a listing with. An
/// ascending `after` cursor above the object's version at the indexed tip ends at the tip; a
/// descending one is `OUT_OF_RANGE` unless an indexed `end_checkpoint` bounds the listing. So is
/// a descending range that starts above the tip; an ascending one ends at the tip.
#[sim_test]
async fn list_object_versions_lifecycle() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        // Keep every version; the fullnode prunes by default.
        .disable_fullnode_pruning()
        .with_rpc_config(two_version_chunks())
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let package_id = publish_basics(&cluster).await;

    let sender = cluster.get_address_0();
    let (create, create_cp) = call_object_basics(
        &cluster,
        package_id,
        "create",
        vec![pure_arg(&7u64), pure_arg(&sender)],
    )
    .await;
    let object = created_object(&create);
    let (set_value, set_value_cp) = call_object_basics(
        &cluster,
        package_id,
        "set_value",
        vec![owned_arg(output_ref(&create, object)), pure_arg(&8u64)],
    )
    .await;
    let (wrap, wrap_cp) = call_object_basics(
        &cluster,
        package_id,
        "wrap",
        vec![owned_arg(output_ref(&set_value, object))],
    )
    .await;
    let wrapper = created_object(&wrap);
    let (unwrap, unwrap_cp) = call_object_basics(
        &cluster,
        package_id,
        "unwrap",
        vec![owned_arg(output_ref(&wrap, wrapper))],
    )
    .await;
    let (set_again, set_again_cp) = call_object_basics(
        &cluster,
        package_id,
        "set_value",
        vec![owned_arg(output_ref(&unwrap, object)), pure_arg(&9u64)],
    )
    .await;
    let (delete, delete_cp) = call_object_basics(
        &cluster,
        package_id,
        "delete",
        vec![owned_arg(output_ref(&set_again, object))],
    )
    .await;
    cluster.wait_for_rpc_index_ready().await;

    let version =
        |effects: &sui_types::effects::TransactionEffects| effects.lamport_version().value();
    let digest = |effects: &sui_types::effects::TransactionEffects| {
        Some(effects.transaction_digest().to_string())
    };
    // (checkpoint, version) of each item, live or a removal.
    let items = [
        (create_cp, version(&create)),
        (set_value_cp, version(&set_value)),
        (wrap_cp, version(&wrap)),
        (unwrap_cp, version(&unwrap)),
        (set_again_cp, version(&set_again)),
        (delete_cp, version(&delete)),
    ];
    let item_versions: Vec<u64> = items.iter().map(|&(_, version)| version).collect();
    let descending_versions: Vec<u64> = item_versions.iter().rev().copied().collect();
    let removals = vec![
        (version(&wrap), Kind::Wrapped, digest(&wrap)),
        (version(&delete), Kind::Deleted, digest(&delete)),
    ];
    let descending_removals: Vec<_> = removals.iter().rev().cloned().collect();

    // Open-ended: ascending runs to the indexed tip, descending to the object's first version.
    let listed = list(&mut client, request(object)).await;
    assert_eq!(listed.versions, item_versions);
    assert_eq!(listed.removals, removals);
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);
    // Each cursor names its item and the indexed tip the listing was bounded by.
    for (cursor, version) in listed.cursors.iter().zip_debug_eq(&item_versions) {
        let ObjectVersionCursor::Version {
            object_id,
            version: cursor_version,
            bounding_checkpoint,
        } = bcs::from_bytes(cursor).unwrap()
        else {
            panic!("an item frame's cursor names its item");
        };
        assert_eq!((object_id, cursor_version), (object, *version));
        assert!(bounding_checkpoint >= delete_cp, "{bounding_checkpoint}");
    }
    let listed = list(&mut client, descending(request(object))).await;
    assert_eq!(listed.versions, descending_versions);
    assert_eq!(listed.removals, descending_removals);
    assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);

    // The default removal mask lists each removal's version and kind, without its transaction.
    let listed = list(&mut client, without_removal_transactions(request(object))).await;
    assert_eq!(listed.versions, item_versions);
    assert_eq!(
        listed.removals,
        [
            (version(&wrap), Kind::Wrapped, None),
            (version(&delete), Kind::Deleted, None),
        ]
    );

    // The wrapper is live at the version that wrapped the object, and deleted by the unwrap.
    let listed = list(&mut client, request(wrapper)).await;
    assert_eq!(listed.versions, [version(&wrap), version(&unwrap)]);
    assert_eq!(
        listed.removals,
        [(version(&unwrap), Kind::Deleted, digest(&unwrap))]
    );
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);

    // Paging by each page's last cursor lists every item once, each removal with its
    // transaction whichever page it lands on, each full page ending with ITEM_LIMIT and the last
    // page with the open-ended listing's reason.
    for (is_descending, limit) in [(false, 1), (false, 3), (true, 1), (true, 3)] {
        let (expected, expected_removals, last_reason) = if is_descending {
            (
                &descending_versions,
                &descending_removals,
                QueryEndReason::CheckpointBound,
            )
        } else {
            (&item_versions, &removals, QueryEndReason::LedgerTip)
        };
        let mut paged: Vec<u64> = Vec::new();
        let mut paged_removals = Vec::new();
        let mut cursor: Option<Bytes> = None;
        loop {
            let mut page = request(object);
            let options = page.options.get_or_insert_default();
            options.limit = Some(limit);
            if is_descending {
                options.ordering = Some(Ordering::Descending as i32);
                options.before = cursor.clone();
            } else {
                options.after = cursor.clone();
            }
            let listed = list(&mut client, page).await;
            paged.extend(&listed.versions);
            paged_removals.extend(listed.removals);
            let end = listed.end.unwrap();
            if end != QueryEndReason::ItemLimit {
                assert_eq!(
                    end, last_reason,
                    "descending {is_descending}, limit {limit}"
                );
                break;
            }
            assert_eq!(listed.versions.len(), limit as usize);
            cursor = listed.cursors.last().cloned();
        }
        assert_eq!(
            &paged, expected,
            "descending {is_descending}, limit {limit}"
        );
        assert_eq!(
            &paged_removals, expected_removals,
            "descending {is_descending}, limit {limit}"
        );
    }

    // A checkpoint range lists the items written in it, and ends at its end.
    for checkpoint in create_cp - 1..=delete_cp {
        let mut bounded = request(object);
        bounded.end_checkpoint = Some(checkpoint + 1);
        let visible: Vec<u64> = items
            .iter()
            .filter(|&&(written, _)| written <= checkpoint)
            .map(|&(_, version)| version)
            .collect();
        let listed = list(&mut client, bounded.clone()).await;
        assert_eq!(listed.versions, visible, "up to checkpoint {checkpoint}");
        assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);
        let listed = list(&mut client, descending(bounded)).await;
        assert_eq!(
            listed.versions,
            visible.iter().rev().copied().collect::<Vec<_>>(),
            "descending up to checkpoint {checkpoint}"
        );
        // A descending listing that reaches the object's first version ends at the boundary
        // below every version.
        assert_eq!(listed.end_position(), (0, true));
        assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);

        let mut from = request(object);
        from.start_checkpoint = Some(checkpoint);
        let written_from: Vec<u64> = items
            .iter()
            .filter(|&&(written, _)| written >= checkpoint)
            .map(|&(_, version)| version)
            .collect();
        let listed = list(&mut client, from.clone()).await;
        assert_eq!(
            listed.versions, written_from,
            "from checkpoint {checkpoint}"
        );
        assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);
        let listed = list(&mut client, descending(from)).await;
        assert_eq!(
            listed.versions,
            written_from.iter().rev().copied().collect::<Vec<_>>(),
            "descending from checkpoint {checkpoint}"
        );
        assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);
    }
    assert!(wrap_cp < unwrap_cp);

    // A version range ends either ordering at its edge.
    let mut ranged = request(object);
    ranged.start_version = Some(item_versions[1]);
    ranged.end_version = Some(item_versions[3]);
    let listed = list(&mut client, ranged.clone()).await;
    assert_eq!(listed.versions, item_versions[1..3]);
    assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);
    let listed = list(&mut client, descending(ranged)).await;
    assert_eq!(listed.versions, [item_versions[2], item_versions[1]]);
    assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);

    // The far cursor in listing order ends a listing with CURSOR_BOUND.
    let mut before = request(object);
    before.options.get_or_insert_default().before = Some(cursor(object, item_versions[2]));
    let listed = list(&mut client, before).await;
    assert_eq!(listed.versions, item_versions[..2]);
    assert_eq!(listed.end.unwrap(), QueryEndReason::CursorBound);
    let mut after = descending(request(object));
    after.options.get_or_insert_default().after = Some(cursor(object, item_versions[3]));
    let listed = list(&mut client, after).await;
    assert_eq!(listed.versions, [item_versions[5], item_versions[4]]);
    assert_eq!(listed.end.unwrap(), QueryEndReason::CursorBound);

    // Turning back from any cursor a listing minted, an item's or the boundary its last frame
    // ends with, in a page of either order, lists exactly the items on the other side of it: a
    // cursor names a position, and `after` and `before` keep their meaning in both orders.
    let mut minted = Vec::new();
    for listing in [request(object), descending(request(object))] {
        let mut paged = listing.clone();
        paged.options.get_or_insert_default().limit = Some(2);
        let listed = list(&mut client, paged).await;
        minted.extend(listed.cursors.iter().cloned());
        minted.extend(listed.end_cursor.clone());
        let listed = list(&mut client, listing).await;
        minted.extend(listed.end_cursor.clone());
    }
    assert_turns_back_from(&mut client, object, &item_versions, &minted).await;

    // An ascending `after` cursor above the object's version at the indexed tip lists nothing
    // and ends at the tip, as the ledger List APIs do, handing back the cursor. A descending
    // listing from a cursor above the version after it would skip versions this server has not
    // indexed, so it fails unless an indexed `end_checkpoint` bounds the listing.
    let past_tip = cursor(object, version(&delete) + 2);
    let mut after_past_tip = request(object);
    after_past_tip.options.get_or_insert_default().after = Some(past_tip.clone());
    let listed = list(&mut client, after_past_tip).await;
    assert!(listed.versions.is_empty());
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);
    assert_eq!(listed.end_cursor, Some(past_tip.clone()));
    let mut before_past_tip = descending(request(object));
    before_past_tip.options.get_or_insert_default().before = Some(past_tip.clone());
    let listed = list(&mut client, before_past_tip.clone()).await;
    assert!(listed.versions.is_empty());
    assert_cursor_above_latest_available(&listed.end.unwrap_err());
    // A `before` at the version after the tip's leaves nothing unindexed between them.
    let mut before_next = descending(request(object));
    before_next.options.get_or_insert_default().before = Some(cursor(object, version(&delete) + 1));
    let listed = list(&mut client, before_next).await;
    assert_eq!(listed.versions, descending_versions);
    assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);
    let mut bounded = before_past_tip;
    bounded.end_checkpoint = Some(delete_cp + 1);
    let listed = list(&mut client, bounded).await;
    assert_eq!(listed.versions, descending_versions);
    assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);
    for (listed_cursor, version) in listed.cursors.iter().zip_debug_eq(&descending_versions) {
        assert_eq!(
            listed_cursor,
            &cursor_bounded_by(object, *version, delete_cp)
        );
    }

    // Above the tip, a descending range is refused, naming its bound, and an ascending one runs
    // to the tip.
    let mut unindexed_end = request(object);
    unindexed_end.end_checkpoint = Some(u64::MAX);
    let listed = list(&mut client, descending(unindexed_end.clone())).await;
    assert_above_latest_available(&listed.end.unwrap_err(), "end_checkpoint", u64::MAX);
    let listed = list(&mut client, unindexed_end).await;
    assert_eq!(listed.versions, item_versions);
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);
    let mut unindexed_start = request(object);
    unindexed_start.start_checkpoint = Some(u64::MAX);
    let listed = list(&mut client, descending(unindexed_start.clone())).await;
    assert_above_latest_available(&listed.end.unwrap_err(), "start_checkpoint", u64::MAX);
    let listed = list(&mut client, unindexed_start).await;
    assert!(listed.versions.is_empty());
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);

    // `end_checkpoint` 0 is an empty range: it lists nothing and ends with `CHECKPOINT_BOUND`,
    // at the boundary below every version.
    let mut empty = request(object);
    empty.end_checkpoint = Some(0);
    for listing in [empty.clone(), descending(empty)] {
        let listed = list(&mut client, listing.clone()).await;
        assert!(listed.versions.is_empty(), "{listing:?}");
        assert_eq!(listed.end_position(), (0, true), "{listing:?}");
        assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);
    }
}

/// A listing whose window holds nothing ends at a boundary, in either order: one from a
/// `start_checkpoint` after the object's last change, above its newest version, and one before
/// its creation, below every version. Resuming from each, in the order that minted it or the
/// other, lists exactly the versions on its side, with no repeat and no gap, both before and
/// after the object gets a new version, which lands above both boundaries.
#[sim_test]
async fn list_object_versions_resumes_from_an_empty_window() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
        .with_rpc_config(two_version_chunks())
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let package_id = publish_basics(&cluster).await;
    let sender = cluster.get_address_0();
    let (create, create_cp) = call_object_basics(
        &cluster,
        package_id,
        "create",
        vec![pure_arg(&7u64), pure_arg(&sender)],
    )
    .await;
    let object = created_object(&create);
    let (set_value, set_value_cp) = call_object_basics(
        &cluster,
        package_id,
        "set_value",
        vec![owned_arg(output_ref(&create, object)), pure_arg(&8u64)],
    )
    .await;
    // Another object's creation takes the indexes past the checkpoint after `set_value`'s.
    let (_, later_cp) = call_object_basics(
        &cluster,
        package_id,
        "create",
        vec![pure_arg(&1u64), pure_arg(&sender)],
    )
    .await;
    assert!(later_cp > set_value_cp);
    cluster.wait_for_rpc_index_ready().await;

    let version = |effects: &TransactionEffects| effects.lamport_version().value();
    let mut versions = vec![version(&create), version(&set_value)];
    let mut minted = Vec::new();
    for (start_checkpoint, end_checkpoint, position) in [
        (Some(set_value_cp + 1), None, (version(&set_value), true)),
        (None, Some(create_cp), (0, true)),
    ] {
        for listing in [request(object), descending(request(object))] {
            let mut empty = listing;
            empty.start_checkpoint = start_checkpoint;
            empty.end_checkpoint = end_checkpoint;
            let listed = list(&mut client, empty.clone()).await;
            assert!(listed.versions.is_empty(), "{empty:?}");
            assert_eq!(listed.end_position(), position, "{empty:?}");
            minted.extend(listed.end_cursor);
        }
    }
    assert_turns_back_from(&mut client, object, &versions, &minted).await;

    let (set_again, _) = call_object_basics(
        &cluster,
        package_id,
        "set_value",
        vec![owned_arg(output_ref(&set_value, object)), pure_arg(&9u64)],
    )
    .await;
    cluster.wait_for_rpc_index_ready().await;
    versions.push(version(&set_again));
    assert_turns_back_from(&mut client, object, &versions, &minted).await;
}

/// A descending listing of an object with no version at the tip lists nothing and ends at the
/// boundary below every version, bounded by the tip. On a node that keeps every version, sending
/// that boundary back as the `after` or the `before` cursor of the next descending page lists
/// nothing again, with `CHECKPOINT_BOUND`. The same boundary bounded above the tip, minted by a
/// server that has indexed further, fails with `ABOVE_LATEST_AVAILABLE` at its checkpoint.
#[sim_test]
async fn list_object_versions_resumes_from_an_absent_objects_boundary() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
        .with_rpc_config(two_version_chunks())
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();
    let tip = halt_chain(&cluster).await;

    let absent = ObjectID::random();
    let first_page = list(&mut client, descending(request(absent))).await;
    assert!(first_page.versions.is_empty());
    assert_eq!(first_page.end.unwrap(), QueryEndReason::CheckpointBound);
    let minted = boundary_bounded_by(absent, 0, tip);
    assert_eq!(first_page.end_cursor, Some(minted.clone()));

    for (boundary, served) in [
        (minted, true),
        (boundary_bounded_by(absent, 0, tip + 5), false),
    ] {
        for as_after in [true, false] {
            let mut next_page = descending(request(absent));
            let options = next_page.options.get_or_insert_default();
            if as_after {
                options.after = Some(boundary.clone());
            } else {
                options.before = Some(boundary.clone());
            }
            let listed = list(&mut client, next_page.clone()).await;
            assert!(listed.versions.is_empty(), "{next_page:?}");
            if served {
                assert_eq!(
                    listed.end.unwrap(),
                    QueryEndReason::CheckpointBound,
                    "{next_page:?}"
                );
            } else {
                assert_above_latest_available(&listed.end.unwrap_err(), "cursor", tip + 5);
            }
        }
    }
}

/// Pruning object versions through a checkpoint keeps each object's version at its end, the
/// oldest retained version. A listing covers only the retained versions: one whose range reaches
/// below the oldest retained version lists from it, in either order, and ends as a listing of
/// the whole history would, so a descending page resuming below it is empty, and a descending
/// listing that stops there resumes from it. The checkpoint after the floor is
/// the lowest `start_checkpoint` the node accepts: a lower one, and a range without a lower bound
/// of its own that ends at or below it, are refused, reporting it as `lowestAvailableCheckpoint`,
/// as is every other refusal below the floor. A range that ends at or below the floor is served
/// empty when its `after` cursor leaves nothing to list there, and refused otherwise. A listing
/// bounded below by an `after` cursor at a pruned version fails, in either order, since pruned
/// versions may follow the item the client holds; so does one from a boundary cursor bounded
/// below the floor, while one bounded at it is served. An object with no retained versions lists
/// nothing, even for a descending `before` cursor, and fails a request with an `after` cursor at
/// an item or at a boundary bounded below the floor. A request whose cursors all come from a
/// server that has indexed further fails with `ABOVE_LATEST_AVAILABLE` instead, in either order,
/// since the object may be newer than this node's tip, as does one whose `after` cursor does,
/// before any floor rule.
#[sim_test]
async fn list_object_versions_from_oldest_retained_version() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        // Only the explicit prune below moves the floor.
        .disable_fullnode_pruning()
        .with_rpc_config(two_version_chunks())
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // The clock changes in every checkpoint, so the prune deletes the versions each checkpoint
    // below the last pruned one ended with.
    let clock = SUI_CLOCK_OBJECT_ID;
    let tip = halt_chain(&cluster).await;
    let mut whole_history = request(clock);
    whole_history.options.get_or_insert_default().limit = Some(1_000);
    let history = list(&mut client, whole_history).await;
    assert_eq!(history.end.unwrap(), QueryEndReason::LedgerTip);
    let history = history.versions;
    assert!(
        history.windows(2).all(|pair| pair[0] < pair[1]),
        "versions listed out of order or twice: {history:?}"
    );
    // Every checkpoint ends with a version the listing has.
    for checkpoint in 0..=tip {
        let version = version_at_checkpoint(&mut client, clock, checkpoint).await;
        assert!(
            history.contains(&version),
            "{version}, the clock's version at checkpoint {checkpoint}, is not in {history:?}"
        );
    }
    // A client lists an object before it exists, on this node before it prunes: each listing
    // ends at the boundary below every version, bounded by its checkpoint K, one below the floor
    // the prune sets and one at it.
    let unknown = ObjectID::random();
    assert!(tip >= 3, "{tip}");
    let mut minted_before_creation = Vec::new();
    for bounding_checkpoint in [tip - 2, tip - 1] {
        let mut listing = request(unknown);
        listing.end_checkpoint = Some(bounding_checkpoint + 1);
        let listed = list(&mut client, listing).await;
        assert!(listed.versions.is_empty());
        assert_eq!(
            listed.end_cursor,
            Some(boundary_bounded_by(unknown, 0, bounding_checkpoint))
        );
        minted_before_creation.extend(listed.end_cursor);
    }

    let pruned_to = tip - 1;
    prune_through(&cluster, pruned_to, false).await;
    let oldest = version_at_checkpoint(&mut client, clock, pruned_to).await;
    let retained: Vec<u64> = history
        .iter()
        .copied()
        .filter(|&version| version >= oldest)
        .collect();
    assert_eq!(retained.first(), Some(&oldest));
    assert!(
        history.len() > retained.len() + 1,
        "the prune removed fewer than two versions: {history:?}, oldest retained {oldest}",
    );

    assert_listed_from(&mut client, request(clock), pruned_to, &retained).await;
    // A range wholly below it has no retained versions to list.
    let mut ending_at_oldest = request(clock);
    ending_at_oldest.end_version = Some(oldest);
    for listing in [ending_at_oldest.clone(), descending(ending_at_oldest)] {
        let listed = list(&mut client, listing).await;
        assert!(listed.versions.is_empty());
        assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);
    }
    // A descending page resuming at the oldest retained version has nothing left to list.
    let mut next_page = descending(request(clock));
    next_page.options.get_or_insert_default().before = Some(cursor(clock, oldest));
    let listed = list(&mut client, next_page).await;
    assert!(listed.versions.is_empty());
    assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);

    // The lowest `start_checkpoint` the node accepts is the checkpoint after the floor, from
    // which a range lists the versions written after the floor checkpoint.
    let lowest_start = pruned_to + 1;
    let written_after_floor = &retained[1..];
    assert!(!written_after_floor.is_empty(), "{retained:?}");
    let mut from_lowest = request(clock);
    from_lowest.start_checkpoint = Some(lowest_start);
    let listed = list(&mut client, from_lowest.clone()).await;
    assert_eq!(listed.versions, written_after_floor);
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);
    let listed = list(&mut client, descending(from_lowest)).await;
    assert_eq!(
        listed.versions,
        written_after_floor
            .iter()
            .rev()
            .copied()
            .collect::<Vec<_>>()
    );
    assert_eq!(listed.end_position(), (oldest, true));
    assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);
    // A lower `start_checkpoint`, and a range without a lower bound of its own that ends at or
    // below it, would reach pruned history, in either order.
    let mut below_lowest = request(clock);
    below_lowest.start_checkpoint = Some(pruned_to);
    let mut ending_at_lowest = request(clock);
    ending_at_lowest.end_checkpoint = Some(lowest_start);
    for listing in [below_lowest, ending_at_lowest.clone()] {
        for listing in [listing.clone(), descending(listing)] {
            let listed = list(&mut client, listing.clone()).await;
            assert!(listed.versions.is_empty(), "{listing:?}");
            assert_below_earliest_available(&listed.end.unwrap_err(), lowest_start);
        }
    }
    // With an `after` cursor the cursor governs instead: one at the oldest retained version
    // leaves nothing up to the floor checkpoint.
    let mut resumed = ending_at_lowest;
    resumed.options.get_or_insert_default().after = Some(cursor(clock, oldest));
    let listed = list(&mut client, resumed).await;
    assert!(listed.versions.is_empty());
    assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);
    // A range that ends at or below the floor checkpoint lies where this node reads no version.
    // An `after` cursor at or above the version at the floor leaves nothing to list there, so the
    // listing is served empty, handing the cursor back, as `ListTransactions` serves a scan its
    // cursors alone leave empty, also with a lower `start_checkpoint` the cursor bounds. One
    // below it may leave pruned versions in the range, so it is refused.
    let at_oldest = cursor(clock, oldest);
    let mut ending_below_floor = request(clock);
    ending_below_floor.end_checkpoint = Some(pruned_to);
    let mut from_below_floor = ending_below_floor.clone();
    from_below_floor.start_checkpoint = Some(pruned_to - 1);
    for (listing, end) in [
        (ending_below_floor.clone(), QueryEndReason::CheckpointBound),
        (
            descending(ending_below_floor.clone()),
            QueryEndReason::CursorBound,
        ),
        (from_below_floor, QueryEndReason::CheckpointBound),
    ] {
        let mut resumed = listing;
        resumed.options.get_or_insert_default().after = Some(at_oldest.clone());
        let listed = list(&mut client, resumed.clone()).await;
        assert!(listed.versions.is_empty(), "{resumed:?}");
        assert_eq!(listed.end_cursor, Some(at_oldest.clone()), "{resumed:?}");
        assert_eq!(listed.end.unwrap(), end, "{resumed:?}");
    }
    ending_below_floor.options.get_or_insert_default().after = Some(cursor(clock, oldest - 1));
    for listing in [ending_below_floor.clone(), descending(ending_below_floor)] {
        let listed = list(&mut client, listing.clone()).await;
        assert!(listed.versions.is_empty(), "{listing:?}");
        assert_below_earliest_available(&listed.end.unwrap_err(), lowest_start);
    }
    // A lower `start_checkpoint` is served when an `after` cursor at or above the oldest retained
    // version bounds the listing, as `ListTransactions` checks the start only after the cursors.
    let mut below_lowest_after = request(clock);
    below_lowest_after.start_checkpoint = Some(pruned_to);
    below_lowest_after.options.get_or_insert_default().after = Some(cursor(clock, oldest));
    let listed = list(&mut client, below_lowest_after).await;
    assert_eq!(listed.versions, written_after_floor);
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);
    // An `after` cursor below the oldest retained version leaves its next version, written at
    // or before the floor checkpoint, in the range, so the lower `start_checkpoint` is refused,
    // as `ListTransactions` refuses a start the cursors leave below its floor.
    let mut below_lowest_held_below = request(clock);
    below_lowest_held_below.start_checkpoint = Some(pruned_to);
    below_lowest_held_below
        .options
        .get_or_insert_default()
        .after = Some(cursor(clock, oldest - 1));
    for listing in [
        below_lowest_held_below.clone(),
        descending(below_lowest_held_below),
    ] {
        let listed = list(&mut client, listing.clone()).await;
        assert!(listed.versions.is_empty(), "{listing:?}");
        assert_below_earliest_available(&listed.end.unwrap_err(), lowest_start);
    }
    // An empty range lists nothing, whatever the floor.
    let mut empty_below = request(clock);
    empty_below.start_checkpoint = Some(1);
    empty_below.end_checkpoint = Some(1);
    for listing in [empty_below.clone(), descending(empty_below)] {
        let listed = list(&mut client, listing.clone()).await;
        assert!(listed.versions.is_empty(), "{listing:?}");
        assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);
    }
    // A range that ends after the lowest start lists the retained versions, and is complete.
    let mut to_tip = request(clock);
    to_tip.end_checkpoint = Some(lowest_start + 1);
    let listed = list(&mut client, to_tip).await;
    assert_eq!(listed.versions, retained);
    assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);

    // No retained version, as for an object that never existed: lists nothing, also for a
    // descending `before` cursor (the `after` cases are below).
    let mut before = descending(request(unknown));
    before.options.get_or_insert_default().before = Some(cursor(unknown, 5));
    for (listing, end) in [
        (request(unknown), QueryEndReason::LedgerTip),
        (
            descending(request(unknown)),
            QueryEndReason::CheckpointBound,
        ),
        (before, QueryEndReason::CheckpointBound),
    ] {
        let listed = list(&mut client, listing.clone()).await;
        assert!(listed.versions.is_empty(), "{listing:?}");
        assert_eq!(listed.end.unwrap(), end, "{listing:?}");
    }
    // An empty ascending listing ends at the boundary below every version, bounded by the tip:
    // the client holds what was written through it, and this node has pruned nothing after it,
    // so resuming from it lists nothing rather than failing with the floor.
    let empty = list(&mut client, request(unknown)).await;
    assert_eq!(empty.end_position(), (0, true));
    let mut resumed_empty = request(unknown);
    resumed_empty.options.get_or_insert_default().after = empty.end_cursor.clone();
    let listed = list(&mut client, resumed_empty).await;
    assert!(listed.versions.is_empty());
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);
    // So does polling from a boundary minted before the prune and bounded at the floor
    // checkpoint. One bounded below it vouches only for what was written through a checkpoint
    // whose successors the prune took, which may have held versions, so it is refused.
    let [below_floor, at_floor] = <[Bytes; 2]>::try_from(minted_before_creation).unwrap();
    for (ordering, end) in [
        (Ordering::Ascending, QueryEndReason::LedgerTip),
        (Ordering::Descending, QueryEndReason::CheckpointBound),
    ] {
        let mut poll = request(unknown);
        let options = poll.options.get_or_insert_default();
        options.ordering = Some(ordering as i32);
        options.after = Some(at_floor.clone());
        let listed = list(&mut client, poll.clone()).await;
        assert!(listed.versions.is_empty(), "{poll:?}");
        assert_eq!(listed.end.unwrap(), end, "{poll:?}");
        poll.options.get_or_insert_default().after = Some(below_floor.clone());
        let listed = list(&mut client, poll.clone()).await;
        assert_below_earliest_available(&listed.end.unwrap_err(), lowest_start);
    }
    // On an object whose history the node keeps from above the floor, a boundary below every
    // version bounded at the floor checkpoint lists every retained version, and descending, it
    // ends the listing at that cursor, handed back. One bounded below the floor is refused, in
    // either order, since versions the prune took may follow what its client holds.
    let below_every_version = boundary_bounded_by(clock, 0, pruned_to);
    let mut resumed_clock = request(clock);
    resumed_clock.options.get_or_insert_default().after = Some(below_every_version.clone());
    let listed = list(&mut client, resumed_clock.clone()).await;
    assert_eq!(listed.versions, retained);
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);
    let listed = list(&mut client, descending(resumed_clock.clone())).await;
    assert_eq!(
        listed.versions,
        retained.iter().rev().copied().collect::<Vec<_>>()
    );
    assert_eq!(listed.end_cursor, Some(below_every_version));
    assert_eq!(listed.end.unwrap(), QueryEndReason::CursorBound);
    resumed_clock.options.get_or_insert_default().after =
        Some(boundary_bounded_by(clock, 0, pruned_to - 1));
    for listing in [resumed_clock.clone(), descending(resumed_clock)] {
        let listed = list(&mut client, listing.clone()).await;
        assert!(listed.versions.is_empty(), "{listing:?}");
        assert_below_earliest_available(&listed.end.unwrap_err(), lowest_start);
    }

    // Cursors all minted by a server that has indexed further may name versions this node has
    // not indexed yet, or ones it pruned, so the request fails with `ABOVE_LATEST_AVAILABLE` at
    // the lowest cursor's checkpoint, in either order, until this node catches up.
    let ahead = cursor_bounded_by(unknown, 5, tip + 5);
    let further = cursor_bounded_by(unknown, 9, tip + 9);
    let held = cursor_bounded_by(unknown, 3, tip);
    for ordering in [Ordering::Ascending, Ordering::Descending] {
        for (after, before) in [
            (Some(&ahead), None),
            (None, Some(&ahead)),
            (Some(&ahead), Some(&further)),
        ] {
            let mut listing = request(unknown);
            let options = listing.options.get_or_insert_default();
            options.ordering = Some(ordering as i32);
            options.after = after.cloned();
            options.before = before.cloned();
            let listed = list(&mut client, listing.clone()).await;
            assert_above_latest_available(&listed.end.unwrap_err(), "cursor", tip + 5);
        }
    }
    // A cursor this node could have minted names a version it had, which pruning took, so an
    // `after` cursor fails with the floor, whatever the other cursor. Under an indexed
    // `end_checkpoint`, which bounds the listing at a checkpoint this node has indexed, an
    // `after` cursor from a server that has indexed further gets `ABOVE_LATEST_AVAILABLE` before
    // that floor rule, in either order.
    let mut held_after = request(unknown);
    let options = held_after.options.get_or_insert_default();
    options.after = Some(held);
    options.before = Some(ahead.clone());
    let listed = list(&mut client, held_after).await;
    assert_below_earliest_available(&listed.end.unwrap_err(), lowest_start);
    let mut bounded = request(unknown);
    bounded.end_checkpoint = Some(tip + 1);
    bounded.options.get_or_insert_default().after = Some(ahead);
    for listing in [bounded.clone(), descending(bounded)] {
        let listed = list(&mut client, listing.clone()).await;
        assert_above_latest_available(&listed.end.unwrap_err(), "cursor", tip + 5);
    }
}

/// A boundary cursor above version 0 vouches for nothing above it: one an `end_version` cut
/// mints can sit below versions written by its checkpoint. Resuming from one after pruning has
/// raised the object's version at the floor above the version after it is refused, in either
/// order, rather than skip the versions between, even though the boundary is bounded above the
/// floor. A boundary at or above the version at the floor, such as one a listing to the floor
/// checkpoint mints at the top of its window, resumes normally.
#[sim_test]
async fn list_object_versions_resumes_from_a_version_cut_boundary() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        // Only the explicit prune below moves the floor.
        .disable_fullnode_pruning()
        .with_rpc_config(two_version_chunks())
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let package_id = publish_basics(&cluster).await;
    let sender = cluster.get_address_0();
    let (create, _) = call_object_basics(
        &cluster,
        package_id,
        "create",
        vec![pure_arg(&7u64), pure_arg(&sender)],
    )
    .await;
    let object = created_object(&create);
    // The object's versions v1 to v4, each written in a later checkpoint.
    let mut versions = vec![create.lamport_version().value()];
    let mut checkpoints = Vec::new();
    let mut previous = create;
    for value in [8u64, 9, 10] {
        let (effects, checkpoint) = call_object_basics(
            &cluster,
            package_id,
            "set_value",
            vec![owned_arg(output_ref(&previous, object)), pure_arg(&value)],
        )
        .await;
        versions.push(effects.lamport_version().value());
        checkpoints.push(checkpoint);
        previous = effects;
    }
    assert!(checkpoints.windows(2).all(|pair| pair[0] < pair[1]));
    let [_, v2, v3, v4] = <[u64; 4]>::try_from(versions.clone()).unwrap();
    let v3_checkpoint = checkpoints[1];
    cluster.wait_for_rpc_index_ready().await;

    // Before pruning: a listing to `end_version` v2 lists v1 and ends at the boundary below
    // v2; one to `end_version` v4 ends at the boundary below v4; and one to the checkpoint that
    // wrote v3 ends at the boundary above v3, the top of its window.
    let mut minted = Vec::new();
    for (end_version, end_checkpoint, listed_versions, position) in [
        (Some(v2), None, &versions[..1], (v2 - 1, true)),
        (Some(v4), None, &versions[..3], (v4 - 1, true)),
        (None, Some(v3_checkpoint + 1), &versions[..3], (v3, true)),
    ] {
        let mut cut = request(object);
        cut.end_version = end_version;
        cut.end_checkpoint = end_checkpoint;
        let listed = list(&mut client, cut.clone()).await;
        assert_eq!(listed.versions, listed_versions, "{cut:?}");
        assert_eq!(listed.end_position(), position, "{cut:?}");
        assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);
        minted.push(listed.end_cursor.unwrap());
    }
    let [below_v2, below_v4, above_v3] = <[Bytes; 3]>::try_from(minted).unwrap();

    // Pruning through the checkpoint that wrote v3 deletes v1 and v2 and keeps v3, the object's
    // version at the floor.
    prune_through(&cluster, v3_checkpoint, false).await;
    let lowest_start = v3_checkpoint + 1;
    for listing in [request(object), descending(request(object))] {
        let mut resumed = listing.clone();
        resumed.options.get_or_insert_default().after = Some(below_v2.clone());
        let listed = list(&mut client, resumed.clone()).await;
        assert!(listed.versions.is_empty(), "{resumed:?}");
        assert_below_earliest_available(&listed.end.unwrap_err(), lowest_start);
    }
    for cursor in [below_v4, above_v3] {
        let mut resumed = request(object);
        resumed.options.get_or_insert_default().after = Some(cursor.clone());
        let listed = list(&mut client, resumed).await;
        assert_eq!(listed.versions, [v4], "{:?}", decoded(&cursor));
        assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);
    }
}

/// A derived object claimed, released and claimed again lists both incarnations under its one ID,
/// the release as a `DELETED` removal with its own transaction. On a node that has pruned, an
/// object claimed after the floor is listed from its first claim, also from a `start_checkpoint`
/// below the floor that a boundary cursor bounded at the floor checkpoint bounds, while one
/// bounded below it is refused.
#[sim_test]
async fn list_object_versions_reused_id() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        // Only the explicit prune below moves the floor.
        .disable_fullnode_pruning()
        .with_rpc_config(two_version_chunks())
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let package_id = publish_derived_object_release(&cluster).await;
    let (create_parent, _) = call_derived(&cluster, package_id, "create_parent", vec![]).await;
    let parent = created_object(&create_parent);

    let floor = fullnode_highest_executed(&cluster);
    prune_through(&cluster, floor, false).await;

    let (claim, claim_cp) = call_derived(
        &cluster,
        package_id,
        "claim_owned",
        vec![
            owned_arg(output_ref(&create_parent, parent)),
            pure_arg(&0u64),
        ],
    )
    .await;
    assert!(
        claim_cp > floor,
        "claimed at {claim_cp}, at or below the floor {floor}"
    );
    let object = claimed_object(&cluster, &claim);
    let (release, _) = call_derived(
        &cluster,
        package_id,
        "release",
        vec![
            owned_arg(output_ref(&claim, parent)),
            owned_arg(output_ref(&claim, object)),
        ],
    )
    .await;
    let (reclaim, _) = call_derived(
        &cluster,
        package_id,
        "claim_owned",
        vec![owned_arg(output_ref(&release, parent)), pure_arg(&1u64)],
    )
    .await;
    assert_eq!(claimed_object(&cluster, &reclaim), object);
    cluster.wait_for_rpc_index_ready().await;

    let version =
        |effects: &sui_types::effects::TransactionEffects| effects.lamport_version().value();
    let versions = [version(&claim), version(&release), version(&reclaim)];
    let removals = [(
        version(&release),
        Kind::Deleted,
        Some(release.transaction_digest().to_string()),
    )];

    assert_listed_from(&mut client, request(object), floor, &versions).await;
    // A client listed the object from `start_checkpoint` 0 before its first claim, and got the
    // boundary below every version, bounded at the checkpoint its listing stopped at. Polling
    // from one bounded at the floor checkpoint, the node keeps everything written after it, so
    // the object is listed whole, though it has no version at the floor; one bounded below the
    // floor may have missed versions the prune took, so it is refused, in either order.
    for (bounding_checkpoint, served) in [(floor, true), (floor - 1, false)] {
        let mut poll = request(object);
        poll.start_checkpoint = Some(0);
        poll.options.get_or_insert_default().after =
            Some(boundary_bounded_by(object, 0, bounding_checkpoint));
        for (listing, end) in [
            (poll.clone(), QueryEndReason::LedgerTip),
            (descending(poll), QueryEndReason::CursorBound),
        ] {
            let listed = list(&mut client, listing.clone()).await;
            if served {
                let mut expected = versions.to_vec();
                if end == QueryEndReason::CursorBound {
                    expected.reverse();
                }
                assert_eq!(listed.versions, expected, "{listing:?}");
                assert_eq!(listed.end.unwrap(), end, "{listing:?}");
            } else {
                assert!(listed.versions.is_empty(), "{listing:?}");
                assert_below_earliest_available(&listed.end.unwrap_err(), floor + 1);
            }
        }
    }
    let listed = list(&mut client, descending(request(object))).await;
    assert_eq!(listed.removals, removals);
}

/// A derived object released at or before the floor and claimed again after it is listed from
/// the re-claim, passing over the release's tombstone, which the pruner keeps as with
/// `killswitch_tombstone_pruning`: pruning dropped the release's row from the version index.
#[sim_test]
async fn list_object_versions_claimed_again_after_the_floor() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        // Only the explicit prune below moves the floor.
        .disable_fullnode_pruning()
        .with_rpc_config(two_version_chunks())
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let package_id = publish_derived_object_release(&cluster).await;
    let (create_parent, _) = call_derived(&cluster, package_id, "create_parent", vec![]).await;
    let parent = created_object(&create_parent);
    let (claim, _) = call_derived(
        &cluster,
        package_id,
        "claim_owned",
        vec![
            owned_arg(output_ref(&create_parent, parent)),
            pure_arg(&0u64),
        ],
    )
    .await;
    let object = claimed_object(&cluster, &claim);
    let (release, floor) = call_derived(
        &cluster,
        package_id,
        "release",
        vec![
            owned_arg(output_ref(&claim, parent)),
            owned_arg(output_ref(&claim, object)),
        ],
    )
    .await;
    prune_through(&cluster, floor, true).await;
    assert!(is_tombstone_stored(
        &cluster,
        object,
        release.lamport_version().value()
    ));

    let (reclaim, reclaim_cp) = call_derived(
        &cluster,
        package_id,
        "claim_owned",
        vec![owned_arg(output_ref(&release, parent)), pure_arg(&1u64)],
    )
    .await;
    assert!(reclaim_cp > floor);
    assert_eq!(claimed_object(&cluster, &reclaim), object);
    cluster.wait_for_rpc_index_ready().await;
    let reclaimed = reclaim.lamport_version().value();

    assert_listed_from(&mut client, request(object), floor, &[reclaimed]).await;
    // A client holding the claim lists from the release with a `start_checkpoint` below the
    // lowest one the node accepts. Its `after` cursor bounds the listing, but the object has no
    // version at the floor, so the node cannot tell whether versions from the release on were
    // written after the floor or pruned: it refuses, in either order, rather than list from the
    // re-claim and skip the release.
    let mut from_release = request(object);
    from_release.start_checkpoint = Some(floor);
    from_release.start_version = Some(release.lamport_version().value());
    from_release.options.get_or_insert_default().after =
        Some(cursor(object, claim.lamport_version().value()));
    for listing in [from_release.clone(), descending(from_release.clone())] {
        let listed = list(&mut client, listing.clone()).await;
        assert!(listed.versions.is_empty(), "{listing:?}");
        assert_below_earliest_available(&listed.end.unwrap_err(), floor + 1);
    }
    // So it does for a boundary bounded at the floor checkpoint that does not bound the listing,
    // since its next version is below `start_version`: the one below every version, and the one
    // a listing to `end_version` at the claim mints.
    for boundary_version in [0, claim.lamport_version().value() - 1] {
        let mut from_release = from_release.clone();
        from_release.options.get_or_insert_default().after =
            Some(boundary_bounded_by(object, boundary_version, floor));
        for listing in [from_release.clone(), descending(from_release)] {
            let listed = list(&mut client, listing.clone()).await;
            assert!(listed.versions.is_empty(), "{listing:?}");
            assert_below_earliest_available(&listed.end.unwrap_err(), floor + 1);
        }
    }
}

/// A derived object released at or before the floor and claimed again after it is listed from
/// the re-claim, on a node whose background pruner, gated on the rpc indexes and keeping its
/// retraction cursors between runs, removed the released incarnation.
///
/// Simulator only: the fullnode's background pruner runs every few seconds only under msim
/// (`AuthorityStorePruningConfig::pruning_run_delay_seconds`).
#[cfg(msim)]
#[sim_test]
async fn list_object_versions_claimed_again_after_background_pruning() {
    // The fullnode prunes every checkpoint it can, in the background.
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_rpc_config(two_version_chunks())
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let package_id = publish_derived_object_release(&cluster).await;
    let (create_parent, _) = call_derived(&cluster, package_id, "create_parent", vec![]).await;
    let parent = created_object(&create_parent);
    let (claim, _) = call_derived(
        &cluster,
        package_id,
        "claim_owned",
        vec![
            owned_arg(output_ref(&create_parent, parent)),
            pure_arg(&0u64),
        ],
    )
    .await;
    let object = claimed_object(&cluster, &claim);
    let (release, release_cp) = call_derived(
        &cluster,
        package_id,
        "release",
        vec![
            owned_arg(output_ref(&claim, parent)),
            owned_arg(output_ref(&claim, object)),
        ],
    )
    .await;
    cluster
        .fullnode_handle
        .sui_node
        .with_async(|node| async move {
            tokio::time::timeout(std::time::Duration::from_secs(60), async {
                while node
                    .state()
                    .get_object_cache_reader()
                    .get_highest_pruned_checkpoint()
                    .is_none_or(|pruned| pruned < release_cp)
                {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            })
            .await
            .expect("the fullnode did not prune the release's checkpoint");
        })
        .await;

    let (reclaim, _) = call_derived(
        &cluster,
        package_id,
        "claim_owned",
        vec![owned_arg(output_ref(&release, parent)), pure_arg(&1u64)],
    )
    .await;
    assert_eq!(claimed_object(&cluster, &reclaim), object);
    cluster.wait_for_rpc_index_ready().await;
    let reclaimed = reclaim.lamport_version().value();

    let listed = list(&mut client, request(object)).await;
    assert_eq!(listed.versions, [reclaimed]);
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);
    let listed = list(&mut client, descending(request(object))).await;
    assert_eq!(listed.versions, [reclaimed]);
    assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);
}

/// An object wrapped at or before the floor and unwrapped after it is listed from the unwrap.
#[sim_test]
async fn list_object_versions_unwrapped_after_the_floor() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        // Only the explicit prune below moves the floor.
        .disable_fullnode_pruning()
        .with_rpc_config(two_version_chunks())
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let unwrapped = wrap_then_unwrap(&cluster).await;
    prune_through(&cluster, unwrapped.floor, false).await;
    assert!(!is_stored(&cluster, unwrapped.object, unwrapped.created));
    assert_listed_from(
        &mut client,
        request(unwrapped.object),
        unwrapped.floor,
        &unwrapped.versions,
    )
    .await;
    // A range that ends at the floor checkpoint, where the object is wrapped, ends at or below
    // the lowest `start_checkpoint` the node accepts, so without a lower bound of its own it is
    // refused, reporting that checkpoint.
    let mut at_floor = request(unwrapped.object);
    at_floor.end_checkpoint = Some(unwrapped.floor + 1);
    for listing in [at_floor.clone(), descending(at_floor.clone())] {
        let listed = list(&mut client, listing).await;
        assert!(listed.versions.is_empty());
        assert_below_earliest_available(&listed.end.unwrap_err(), unwrapped.floor + 1);
    }
    // With an `after` cursor the cursor governs instead: a client holding the creation from
    // before the prune would miss the wrap.
    at_floor.options.get_or_insert_default().after =
        Some(cursor(unwrapped.object, unwrapped.created));
    let listed = list(&mut client, at_floor).await;
    assert!(listed.versions.is_empty());
    assert_below_earliest_available(&listed.end.unwrap_err(), unwrapped.floor + 1);
}

/// A derived object released at or before the floor, then claimed again after it straight into a
/// wrapper and released from it, is listed from that last release, passing over the first
/// release's tombstone, which the pruner keeps as with `killswitch_tombstone_pruning`: the
/// version index records the last release as the object's first version after the floor.
#[sim_test]
async fn list_object_versions_reclaimed_into_a_wrapper_after_the_floor() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        // Only the explicit prune below moves the floor.
        .disable_fullnode_pruning()
        .with_rpc_config(two_version_chunks())
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let package_id = publish_derived_object_release(&cluster).await;
    let (create_parent, _) = call_derived(&cluster, package_id, "create_parent", vec![]).await;
    let parent = created_object(&create_parent);
    let (claim, _) = call_derived(
        &cluster,
        package_id,
        "claim_owned",
        vec![
            owned_arg(output_ref(&create_parent, parent)),
            pure_arg(&0u64),
        ],
    )
    .await;
    let object = claimed_object(&cluster, &claim);
    let (release, floor) = call_derived(
        &cluster,
        package_id,
        "release",
        vec![
            owned_arg(output_ref(&claim, parent)),
            owned_arg(output_ref(&claim, object)),
        ],
    )
    .await;
    prune_through(&cluster, floor, true).await;
    assert!(is_tombstone_stored(
        &cluster,
        object,
        release.lamport_version().value()
    ));

    let (claim_wrapped, _) = call_derived(
        &cluster,
        package_id,
        "claim_wrapped",
        vec![owned_arg(output_ref(&release, parent)), pure_arg(&1u64)],
    )
    .await;
    let wrapper = claimed_object(&cluster, &claim_wrapped);
    let (release_wrapped, _) = call_derived(
        &cluster,
        package_id,
        "release_wrapped",
        vec![
            owned_arg(output_ref(&claim_wrapped, parent)),
            owned_arg(output_ref(&claim_wrapped, wrapper)),
        ],
    )
    .await;
    cluster.wait_for_rpc_index_ready().await;
    let released = release_wrapped.lamport_version().value();

    assert_listed_from(&mut client, request(object), floor, &[released]).await;
    let listed = list(&mut client, descending(request(object))).await;
    assert_eq!(
        listed.removals,
        [(
            released,
            Kind::Deleted,
            Some(release_wrapped.transaction_digest().to_string())
        )]
    );
}

/// On a node that retains checkpoints for fewer epochs than objects, a derived object released at
/// or before the object floor, then claimed again after it straight into a wrapper and released
/// from it, at or before the checkpoint floor. A listing that leaves out removal transactions
/// places the last release, whose effects are pruned, through the version index, and lists the
/// object from it, passing over the first release's tombstone when the pruner keeps it, as with
/// `killswitch_tombstone_pruning`. A listing that reports removal transactions has the
/// checkpoint floor, at or after the last release, so it lists nothing, until the object is
/// claimed again after that floor, from which it is then listed.
#[sim_test]
async fn list_object_versions_deleted_with_pruned_effects() {
    for keep_tombstones in [false, true] {
        let cluster = TestClusterBuilder::new()
            .with_num_validators(1)
            // Only the explicit prunes below move the floors.
            .disable_fullnode_pruning()
            .with_rpc_config(two_version_chunks())
            .build()
            .await;
        let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
            .await
            .unwrap();

        let package_id = publish_derived_object_release(&cluster).await;
        let (create_parent, _) = call_derived(&cluster, package_id, "create_parent", vec![]).await;
        let parent = created_object(&create_parent);
        let (claim, _) = call_derived(
            &cluster,
            package_id,
            "claim_owned",
            vec![
                owned_arg(output_ref(&create_parent, parent)),
                pure_arg(&0u64),
            ],
        )
        .await;
        let object = claimed_object(&cluster, &claim);
        let (release, floor) = call_derived(
            &cluster,
            package_id,
            "release",
            vec![
                owned_arg(output_ref(&claim, parent)),
                owned_arg(output_ref(&claim, object)),
            ],
        )
        .await;
        prune_through(&cluster, floor, keep_tombstones).await;
        let first_released = release.lamport_version().value();
        assert_eq!(
            is_tombstone_stored(&cluster, object, first_released),
            keep_tombstones
        );

        let (claim_wrapped, _) = call_derived(
            &cluster,
            package_id,
            "claim_wrapped",
            vec![owned_arg(output_ref(&release, parent)), pure_arg(&1u64)],
        )
        .await;
        let wrapper = claimed_object(&cluster, &claim_wrapped);
        let (release_wrapped, release_wrapped_cp) = call_derived(
            &cluster,
            package_id,
            "release_wrapped",
            vec![
                owned_arg(output_ref(&claim_wrapped, parent)),
                owned_arg(output_ref(&claim_wrapped, wrapper)),
            ],
        )
        .await;
        cluster.trigger_reconfiguration().await;
        cluster.trigger_reconfiguration().await;
        let checkpoint_floor = cluster.prune_fullnode_checkpoints_for_testing().await;
        assert!(checkpoint_floor >= release_wrapped_cp);
        cluster.fullnode_handle.sui_node.with(|node| {
            let state = node.state();
            for transaction in [&claim_wrapped, &release_wrapped] {
                let digest = transaction.transaction_digest();
                // Pruning deletes the effects from the store, not from the cache.
                state
                    .cache_for_testing()
                    .evict_executed_effects_from_cache_for_testing(digest);
                assert!(
                    state
                        .get_transaction_cache_reader()
                        .get_executed_effects(digest)
                        .is_none(),
                    "the re-claim's and last release's effects are pruned"
                );
            }
        });
        let released = release_wrapped.lamport_version().value();

        let without_transactions = without_removal_transactions(request(object));
        assert_listed_from(
            &mut client,
            without_transactions.clone(),
            floor,
            &[released],
        )
        .await;
        let listed = list(&mut client, descending(without_transactions)).await;
        assert_eq!(listed.removals, [(released, Kind::Deleted, None)]);

        let listed = list(&mut client, request(object)).await;
        assert!(listed.versions.is_empty());
        assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);
        let listed = list(&mut client, descending(request(object))).await;
        assert!(listed.versions.is_empty());
        assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);

        // Claimed and released again after the checkpoint floor: a listing that reports removal
        // transactions starts at the first item after the last release, the floor's removal.
        let (reclaim, _) = call_derived(
            &cluster,
            package_id,
            "claim_owned",
            vec![
                owned_arg(output_ref(&release_wrapped, parent)),
                pure_arg(&2u64),
            ],
        )
        .await;
        assert_eq!(claimed_object(&cluster, &reclaim), object);
        let (release_again, _) = call_derived(
            &cluster,
            package_id,
            "release",
            vec![
                owned_arg(output_ref(&reclaim, parent)),
                owned_arg(output_ref(&reclaim, object)),
            ],
        )
        .await;
        cluster.wait_for_rpc_index_ready().await;
        let reclaimed = reclaim.lamport_version().value();
        let released_again = release_again.lamport_version().value();

        assert_listed_from(
            &mut client,
            request(object),
            checkpoint_floor,
            &[reclaimed, released_again],
        )
        .await;
        let listed = list(&mut client, descending(request(object))).await;
        assert_eq!(
            listed.removals,
            [(
                released_again,
                Kind::Deleted,
                Some(release_again.transaction_digest().to_string())
            )]
        );
    }
}

/// While the pruner has pruned the rpc-store but not yet deleted the object versions it prunes,
/// objects are listed from their first item after the floor, passing over the versions and
/// tombstones the pruner has yet to delete: one wrapped at or before the floor and unwrapped
/// after it, and a derived object released, claimed again and released again at or before the
/// floor, and claimed again after it. The pruned transaction behind a leftover version cannot be
/// placed, and pruning dropped the releases' rows from the version index.
///
/// Simulator only: the prune pauses at a fail point.
#[cfg(msim)]
#[sim_test]
async fn list_object_versions_leftovers_while_pruning() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        // Only the explicit prune below moves the floor.
        .disable_fullnode_pruning()
        .with_rpc_config(two_version_chunks())
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let released = claim_and_release_twice(&cluster).await;
    let unwrapped = wrap_then_unwrap(&cluster).await;
    let (last_claim, _) = call_derived(
        &cluster,
        released.package_id,
        "claim_owned",
        vec![
            owned_arg(output_ref(&released.second_release, released.parent)),
            pure_arg(&2u64),
        ],
    )
    .await;
    let ((), ()) = while_paused_at(
        "prune-objects-before-perpetual-commit",
        prune_through(&cluster, unwrapped.floor, false),
        async {
            // The rpc-store is pruned through the floor, the object floor is not committed yet,
            // and the versions the prune deletes are still readable for the listing to pass over.
            let highest_pruned = cluster.fullnode_handle.sui_node.with(|node| {
                node.state()
                    .get_object_cache_reader()
                    .get_highest_pruned_checkpoint()
            });
            assert!(highest_pruned.is_none_or(|pruned| pruned < unwrapped.floor));
            assert!(is_stored(&cluster, unwrapped.object, unwrapped.created));
            assert!(is_stored(&cluster, released.object, released.second_claim));
            assert_listed_from(
                &mut client,
                request(unwrapped.object),
                unwrapped.floor,
                &unwrapped.versions,
            )
            .await;
            assert_listed_from(
                &mut client,
                request(released.object),
                unwrapped.floor,
                &[last_claim.lamport_version().value()],
            )
            .await;
        },
    )
    .await;
}

/// Pruning that runs after a listing is planned and before it reads anything, and removes every
/// item of the object, leaves the listing nothing to list: it passes over the deletion's
/// tombstone, which the pruner keeps as with `killswitch_tombstone_pruning`.
///
/// Simulator only: the listing pauses at a fail point.
#[cfg(msim)]
#[sim_test]
async fn list_object_versions_pruned_while_listing() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        // Only the explicit prune below moves the floor.
        .disable_fullnode_pruning()
        .with_rpc_config(two_version_chunks())
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let package_id = publish_basics(&cluster).await;
    let sender = cluster.get_address_0();
    let (create, _) = call_object_basics(
        &cluster,
        package_id,
        "create",
        vec![pure_arg(&7u64), pure_arg(&sender)],
    )
    .await;
    let object = created_object(&create);
    let (set_value, _) = call_object_basics(
        &cluster,
        package_id,
        "set_value",
        vec![owned_arg(output_ref(&create, object)), pure_arg(&8u64)],
    )
    .await;
    let (_, delete_cp) = call_object_basics(
        &cluster,
        package_id,
        "delete",
        vec![owned_arg(output_ref(&set_value, object))],
    )
    .await;
    cluster.wait_for_rpc_index_ready().await;

    let (listed, ()) = while_paused_at(
        "list-object-versions-after-plan",
        list(&mut client, request(object)),
        prune_through(&cluster, delete_cp, true),
    )
    .await;
    assert!(listed.versions.is_empty());
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);
}

/// A removal lookup that starts from the removal found before it can walk through rows pruning
/// deletes in the meantime, and miss a removal pruning keeps; the chunk is then read again under
/// the new floor. A derived object claimed and released, then claimed again straight into a
/// wrapper and released from it twice, leaves a run of three tombstones. A listing from the first
/// pauses before the second's lookup, which walks from the first release through the first
/// wrapped re-claim, while pruning through that re-claim's checkpoint deletes both transactions'
/// rows and keeps the tombstones. The listing lists the two releases the floor keeps, each with
/// its transaction.
///
/// Simulator only: the listing pauses at a fail point inside a chunk.
#[cfg(msim)]
#[sim_test]
async fn list_object_versions_pruned_between_removal_lookups() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        // Only the explicit prune below moves the floor.
        .disable_fullnode_pruning()
        .with_rpc_config(two_version_chunks())
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let package_id = publish_derived_object_release(&cluster).await;
    let (create_parent, _) = call_derived(&cluster, package_id, "create_parent", vec![]).await;
    let parent = created_object(&create_parent);
    let (claim, _) = call_derived(
        &cluster,
        package_id,
        "claim_owned",
        vec![
            owned_arg(output_ref(&create_parent, parent)),
            pure_arg(&0u64),
        ],
    )
    .await;
    let object = claimed_object(&cluster, &claim);
    let (release, _) = call_derived(
        &cluster,
        package_id,
        "release",
        vec![
            owned_arg(output_ref(&claim, parent)),
            owned_arg(output_ref(&claim, object)),
        ],
    )
    .await;
    let mut releases = vec![release];
    let mut wrapped_claim_checkpoints = vec![];
    for value in 1..=2u64 {
        let last_release = releases.last().unwrap();
        let (claim_wrapped, claim_wrapped_cp) = call_derived(
            &cluster,
            package_id,
            "claim_wrapped",
            vec![
                owned_arg(output_ref(last_release, parent)),
                pure_arg(&value),
            ],
        )
        .await;
        let wrapper = claimed_object(&cluster, &claim_wrapped);
        let (release_wrapped, _) = call_derived(
            &cluster,
            package_id,
            "release_wrapped",
            vec![
                owned_arg(output_ref(&claim_wrapped, parent)),
                owned_arg(output_ref(&claim_wrapped, wrapper)),
            ],
        )
        .await;
        wrapped_claim_checkpoints.push(claim_wrapped_cp);
        releases.push(release_wrapped);
    }
    cluster.wait_for_rpc_index_ready().await;

    let version =
        |effects: &sui_types::effects::TransactionEffects| effects.lamport_version().value();
    let mut from_first_release = request(object);
    from_first_release.start_version = Some(version(&releases[0]));
    // The first lookup is the first release's, the second the next release's.
    let (listed, ()) = while_blocking_task_paused_at(
        "list-object-versions-before-removal-lookup",
        2,
        list(&mut client, from_first_release),
        prune_through(&cluster, wrapped_claim_checkpoints[0], true),
    )
    .await;
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);
    let kept = &releases[1..];
    assert_eq!(
        listed.versions,
        kept.iter().map(version).collect::<Vec<_>>()
    );
    assert_eq!(
        listed.removals,
        kept.iter()
            .map(|release| (
                version(release),
                Kind::Deleted,
                Some(release.transaction_digest().to_string())
            ))
            .collect::<Vec<_>>()
    );
}

/// Pruning that runs after a listing is planned and before it reads anything, and raises the
/// object's oldest retained version above where the listing starts, makes it list from the new
/// oldest retained version, in either order, including the versions of a chunk that reaches
/// below the new floor; a descending listing ends with the cursor of the last item it sent, so
/// a client resuming from it on a node that has pruned less lists what this one took, unless it
/// ends at its far cursor, which it hands back. A listing that resumes from a cursor below the
/// new oldest retained version fails, since pruned versions follow the history the client
/// holds: an item's cursor, or a boundary whose checkpoint the prune passed.
///
/// Simulator only: the listing pauses at a fail point.
#[cfg(msim)]
#[sim_test]
async fn list_object_versions_floor_rises_while_listing() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        // Only the explicit prune below moves the floor.
        .disable_fullnode_pruning()
        .with_rpc_config(two_version_chunks())
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let package_id = publish_basics(&cluster).await;
    let sender = cluster.get_address_0();
    let (create, _) = call_object_basics(
        &cluster,
        package_id,
        "create",
        vec![pure_arg(&7u64), pure_arg(&sender)],
    )
    .await;
    let object = created_object(&create);
    let (set_value, floor) = call_object_basics(
        &cluster,
        package_id,
        "set_value",
        vec![owned_arg(output_ref(&create, object)), pure_arg(&8u64)],
    )
    .await;
    let (set_again, _) = call_object_basics(
        &cluster,
        package_id,
        "set_value",
        vec![owned_arg(output_ref(&set_value, object)), pure_arg(&9u64)],
    )
    .await;
    cluster.wait_for_rpc_index_ready().await;

    let (listed, ()) = while_paused_at(
        "list-object-versions-after-plan",
        list(&mut client, request(object)),
        prune_through(&cluster, floor, false),
    )
    .await;
    assert_eq!(
        listed.versions,
        [
            set_value.lamport_version().value(),
            set_again.lamport_version().value()
        ]
    );
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);

    // Pruning through `new_floor` makes `changed` the oldest retained version and deletes the
    // two below it, so a descending listing paused after planning reads a full chunk above
    // `changed` and then a short one holding only `changed`, which reaches below the new floor.
    let (changed, new_floor) = call_object_basics(
        &cluster,
        package_id,
        "set_value",
        vec![owned_arg(output_ref(&set_again, object)), pure_arg(&10u64)],
    )
    .await;
    let mut previous = changed.clone();
    let mut newer = Vec::new();
    let mut newer_checkpoints = Vec::new();
    for value in [11u64, 12] {
        let (effects, checkpoint) = call_object_basics(
            &cluster,
            package_id,
            "set_value",
            vec![owned_arg(output_ref(&previous, object)), pure_arg(&value)],
        )
        .await;
        assert!(checkpoint > new_floor);
        newer.push(effects.lamport_version().value());
        newer_checkpoints.push(checkpoint);
        previous = effects;
    }
    cluster.wait_for_rpc_index_ready().await;

    let oldest = changed.lamport_version().value();
    let (listed, ()) = while_paused_at(
        "list-object-versions-after-plan",
        list(&mut client, descending(request(object))),
        prune_through(&cluster, new_floor, false),
    )
    .await;
    assert_eq!(listed.versions, [newer[1], newer[0], oldest]);
    assert_eq!(listed.end_position(), (oldest, false));
    assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);

    // The client holds `oldest`, and pruning through the checkpoint of `newer[1]` deletes
    // `newer[0]`, which follows it. A later checkpoint lets the prune reach that one.
    let (thirteenth, _) = call_object_basics(
        &cluster,
        package_id,
        "set_value",
        vec![owned_arg(output_ref(&previous, object)), pure_arg(&13u64)],
    )
    .await;
    cluster.wait_for_rpc_index_ready().await;
    let mut resumed = request(object);
    resumed.options.get_or_insert_default().after = Some(cursor(object, oldest));
    let (listed, ()) = while_paused_at(
        "list-object-versions-after-plan",
        list(&mut client, resumed),
        prune_through(&cluster, newer_checkpoints[1], false),
    )
    .await;
    assert!(listed.versions.is_empty());
    assert_below_earliest_available(&listed.end.unwrap_err(), newer_checkpoints[1] + 1);

    // Three more versions, each in a later checkpoint.
    let mut previous = thirteenth;
    let mut later = Vec::new();
    let mut later_checkpoints = Vec::new();
    for value in [14u64, 15, 16] {
        let (effects, checkpoint) = call_object_basics(
            &cluster,
            package_id,
            "set_value",
            vec![owned_arg(output_ref(&previous, object)), pure_arg(&value)],
        )
        .await;
        later.push(effects.lamport_version().value());
        later_checkpoints.push(checkpoint);
        previous = effects;
    }
    assert!(later_checkpoints.windows(2).all(|pair| pair[0] < pair[1]));
    cluster.wait_for_rpc_index_ready().await;

    // The client resumes from the boundary below every version, bounded at the floor
    // checkpoint, which vouches for what was written through it, so the listing is planned
    // from the oldest retained version. Pruning through a later checkpoint before the first
    // chunk passes the boundary's checkpoint and takes versions written after it, so the
    // listing fails rather than skip them.
    let mut from_boundary = request(object);
    from_boundary.options.get_or_insert_default().after =
        Some(boundary_bounded_by(object, 0, newer_checkpoints[1]));
    let (listed, ()) = while_paused_at(
        "list-object-versions-after-plan",
        list(&mut client, from_boundary),
        prune_through(&cluster, later_checkpoints[0], false),
    )
    .await;
    assert!(listed.versions.is_empty());
    assert_below_earliest_available(&listed.end.unwrap_err(), later_checkpoints[0] + 1);

    // A descending listing to a boundary bounded at the newest version's checkpoint ends at
    // that cursor, with `CURSOR_BOUND`. Pruning through the checkpoint before it, which the
    // boundary vouches for, takes the oldest version the listing was planned to reach, so a
    // full chunk of the two newest versions is followed by one that finds the cut, and the
    // listing still hands the far cursor back.
    let far = boundary_bounded_by(object, 0, later_checkpoints[2]);
    let mut to_boundary = descending(request(object));
    to_boundary.options.get_or_insert_default().after = Some(far.clone());
    let (listed, ()) = while_paused_at(
        "list-object-versions-after-plan",
        list(&mut client, to_boundary),
        prune_through(&cluster, later_checkpoints[1], false),
    )
    .await;
    assert_eq!(listed.versions, [later[2], later[1]]);
    assert_eq!(listed.end_cursor, Some(far));
    assert_eq!(listed.end.unwrap(), QueryEndReason::CursorBound);
}

/// A derived object, claimed and released twice.
#[cfg(msim)]
struct ReleasedTwice {
    package_id: ObjectID,
    parent: ObjectID,
    object: ObjectID,
    /// The second claim's version.
    second_claim: u64,
    /// The second release, which last changed the parent.
    second_release: TransactionEffects,
}

/// Publish `derived_object_release`, then claim and release a derived object twice.
#[cfg(msim)]
async fn claim_and_release_twice(cluster: &TestCluster) -> ReleasedTwice {
    let package_id = publish_derived_object_release(cluster).await;
    let (create_parent, _) = call_derived(cluster, package_id, "create_parent", vec![]).await;
    let parent = created_object(&create_parent);
    let (first_claim, _) = call_derived(
        cluster,
        package_id,
        "claim_owned",
        vec![
            owned_arg(output_ref(&create_parent, parent)),
            pure_arg(&0u64),
        ],
    )
    .await;
    let object = claimed_object(cluster, &first_claim);
    let (first_release, _) = call_derived(
        cluster,
        package_id,
        "release",
        vec![
            owned_arg(output_ref(&first_claim, parent)),
            owned_arg(output_ref(&first_claim, object)),
        ],
    )
    .await;
    let (second_claim, _) = call_derived(
        cluster,
        package_id,
        "claim_owned",
        vec![
            owned_arg(output_ref(&first_release, parent)),
            pure_arg(&1u64),
        ],
    )
    .await;
    assert_eq!(claimed_object(cluster, &second_claim), object);
    let (second_release, _) = call_derived(
        cluster,
        package_id,
        "release",
        vec![
            owned_arg(output_ref(&second_claim, parent)),
            owned_arg(output_ref(&second_claim, object)),
        ],
    )
    .await;
    ReleasedTwice {
        package_id,
        parent,
        object,
        second_claim: second_claim.lamport_version().value(),
        second_release,
    }
}

/// An object created and wrapped before the floor, then unwrapped and changed after it.
struct Unwrapped {
    object: ObjectID,
    /// The version the object was created at, which pruning through the floor deletes.
    created: u64,
    /// The floor checkpoint to prune through: the one before the unwrap's.
    floor: u64,
    /// The unwrap's version and the change's.
    versions: [u64; 2],
}

async fn wrap_then_unwrap(cluster: &TestCluster) -> Unwrapped {
    let package_id = publish_basics(cluster).await;
    let sender = cluster.get_address_0();
    let (create, _) = call_object_basics(
        cluster,
        package_id,
        "create",
        vec![pure_arg(&7u64), pure_arg(&sender)],
    )
    .await;
    let object = created_object(&create);
    let (wrap, wrap_cp) = call_object_basics(
        cluster,
        package_id,
        "wrap",
        vec![owned_arg(output_ref(&create, object))],
    )
    .await;
    let wrapper = created_object(&wrap);
    let (unwrap, unwrap_cp) = call_object_basics(
        cluster,
        package_id,
        "unwrap",
        vec![owned_arg(output_ref(&wrap, wrapper))],
    )
    .await;
    assert!(unwrap_cp > wrap_cp);
    let (set_value, _) = call_object_basics(
        cluster,
        package_id,
        "set_value",
        vec![owned_arg(output_ref(&unwrap, object)), pure_arg(&8u64)],
    )
    .await;
    Unwrapped {
        object,
        created: create.lamport_version().value(),
        floor: unwrap_cp - 1,
        versions: [
            unwrap.lamport_version().value(),
            set_value.lamport_version().value(),
        ],
    }
}

/// The oldest retained version of the object `listing` lists is `retained[0]`, the first of the
/// items `retained` lists in ascending order, under the floor checkpoint `floor`: listings whose
/// range reaches below it or starts at it list the retained items in either order, a descending
/// one stopping there with a cursor at it, or at the boundary below it for a window that starts
/// there, as do ones bounded below by an `after` cursor right below it, while ones bounded by a
/// cursor further below fail, reporting the checkpoint after the floor, since pruned items may
/// follow it.
async fn assert_listed_from(
    client: &mut LedgerServiceClient<Channel>,
    listing: ListObjectVersionsRequest,
    floor: u64,
    retained: &[u64],
) {
    let object_id: ObjectID = listing.object_id().parse().unwrap();
    let oldest = retained[0];
    let retained_descending: Vec<u64> = retained.iter().rev().copied().collect();
    for start_version in [None, Some(oldest - 1), Some(oldest)] {
        let mut ascending = listing.clone();
        ascending.start_version = start_version;
        let listed = list(client, ascending.clone()).await;
        assert_eq!(listed.versions, retained, "start {start_version:?}");
        assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);
        let listed = list(client, descending(ascending)).await;
        assert_eq!(
            listed.versions, retained_descending,
            "start {start_version:?}"
        );
        let end = if start_version == Some(oldest) {
            (oldest - 1, true)
        } else {
            (oldest, false)
        };
        assert_eq!(listed.end_position(), end, "start {start_version:?}");
        assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);
    }

    let mut resumed = listing.clone();
    resumed.options.get_or_insert_default().after = Some(cursor(object_id, oldest - 1));
    let listed = list(client, resumed.clone()).await;
    assert_eq!(listed.versions, retained);
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);
    let listed = list(client, descending(resumed)).await;
    assert_eq!(listed.versions, retained_descending);
    assert_eq!(listed.end.unwrap(), QueryEndReason::CursorBound);
    let mut resumed_below = listing;
    resumed_below.options.get_or_insert_default().after = Some(cursor(object_id, oldest - 2));
    for listing in [resumed_below.clone(), descending(resumed_below)] {
        let listed = list(client, listing.clone()).await;
        assert!(listed.versions.is_empty(), "{listing:?}");
        assert_below_earliest_available(&listed.end.unwrap_err(), floor + 1);
    }
}

/// Whether the fullnode's object store holds `object_id` at `version`.
fn is_stored(cluster: &TestCluster, object_id: ObjectID, version: u64) -> bool {
    cluster.fullnode_handle.sui_node.with(|node| {
        node.state()
            .get_object_cache_reader()
            .get_object_by_key(&object_id, SequenceNumber::from_u64(version))
            .is_some()
    })
}

/// Whether the fullnode's object store holds a tombstone of `object_id` at `version`.
fn is_tombstone_stored(cluster: &TestCluster, object_id: ObjectID, version: u64) -> bool {
    let version = SequenceNumber::from_u64(version);
    cluster.fullnode_handle.sui_node.with(|node| {
        node.state()
            .get_object_cache_reader()
            .object_versions_iter(object_id, version..=version, false)
            .unwrap()
            .any(|item| matches!(item.unwrap(), ObjectOrTombstone::Tombstone(_)))
    })
}

/// Prune the fullnode through `checkpoint` as its pruner does, rpc indexes included, keeping the
/// object tombstones if `keep_tombstones`, then compact the object table, once the fullnode has
/// executed a later checkpoint and its rpc indexes have committed it. The fullnode must run with
/// background pruning off, so that the floor stays at `checkpoint`.
async fn prune_through(cluster: &TestCluster, checkpoint: u64, keep_tombstones: bool) {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
    while fullnode_highest_executed(cluster) <= checkpoint {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for the fullnode to execute past checkpoint {checkpoint}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    cluster.wait_for_rpc_index_ready().await;
    let pruned = cluster
        .fullnode_handle
        .sui_node
        .with_async(|node| async move {
            let state = node.state();
            let rpc_store = node
                .embedded_rpc_store()
                .expect("the fullnode runs the rpc indexes")
                .store();
            state
                .database_for_testing()
                .prune_objects_and_rpc_store_for_testing(
                    state.get_checkpoint_store(),
                    &rpc_store,
                    checkpoint,
                    keep_tombstones,
                )
                .await
                .unwrap();
            state
                .get_object_cache_reader()
                .get_highest_pruned_checkpoint()
        })
        .await;
    assert_eq!(pruned, Some(checkpoint));
}

/// Run `paused` until it reaches the fail point `name`, run `meanwhile` while it waits there,
/// then let it go on, and return what both returned.
#[cfg(msim)]
async fn while_paused_at<P, M>(
    name: &'static str,
    paused: impl Future<Output = P>,
    meanwhile: impl Future<Output = M>,
) -> (P, M) {
    static PAUSED: tokio::sync::Notify = tokio::sync::Notify::const_new();
    static RESUME: tokio::sync::Notify = tokio::sync::Notify::const_new();
    sui_macros::register_fail_point_async(name, || async {
        PAUSED.notify_one();
        RESUME.notified().await;
    });
    let outputs = tokio::join!(paused, async {
        tokio::time::timeout(std::time::Duration::from_secs(60), PAUSED.notified())
            .await
            .expect("the fail point was not reached");
        let output = meanwhile.await;
        RESUME.notify_one();
        output
    });
    sui_macros::clear_fail_point(name);
    outputs
}

/// Run `paused` until a blocking task reaches the fail point `name` for the `hit`th time, run
/// `meanwhile` while that task waits there, then let it go on, and return what both returned. The
/// task waits on a channel that yields to the simulator, which runs `meanwhile` in the meantime.
#[cfg(msim)]
async fn while_blocking_task_paused_at<P, M>(
    name: &'static str,
    hit: usize,
    paused: impl Future<Output = P>,
    meanwhile: impl Future<Output = M>,
) -> (P, M) {
    static PAUSED: tokio::sync::Notify = tokio::sync::Notify::const_new();
    let (resume_tx, resume_rx) = mysten_common::sync::oneshot::channel::<()>();
    let resume_rx = std::sync::Mutex::new(Some(resume_rx));
    let hits = std::sync::atomic::AtomicUsize::new(0);
    sui_macros::register_fail_point(name, move || {
        if hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1 != hit {
            return;
        }
        PAUSED.notify_one();
        let resume_rx = resume_rx.lock().unwrap().take().unwrap();
        resume_rx.blocking_recv().unwrap();
    });
    let outputs = tokio::join!(paused, async {
        tokio::time::timeout(std::time::Duration::from_secs(60), PAUSED.notified())
            .await
            .expect("the fail point was not reached");
        let output = meanwhile.await;
        resume_tx.send(()).unwrap();
        output
    });
    sui_macros::clear_fail_point(name);
    outputs
}

/// Publish the `basics` example package, returning its package ID.
async fn publish_basics(cluster: &TestCluster) -> ObjectID {
    let publish_tx = tx_builder(cluster)
        .await
        .publish_examples("basics")
        .await
        .build();
    let (publish, _) = execute(cluster, publish_tx).await;
    publish
        .created()
        .into_iter()
        .find(|(_, owner)| *owner == Owner::Immutable)
        .expect("publish created no package")
        .0
        .0
}

/// Publish `tests/data/derived_object_release`, returning its package ID.
async fn publish_derived_object_release(cluster: &TestCluster) -> ObjectID {
    let publish_tx = tx_builder(cluster)
        .await
        .publish_async(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/derived_object_release"),
        )
        .await
        .build();
    let (publish, _) = execute(cluster, publish_tx).await;
    publish
        .created()
        .into_iter()
        .find(|(_, owner)| *owner == Owner::Immutable)
        .expect("publish created no package")
        .0
        .0
}

/// Call `derived_object_release::m::<function>` as address 0.
async fn call_derived(
    cluster: &TestCluster,
    package_id: ObjectID,
    function: &'static str,
    args: Vec<CallArg>,
) -> (TransactionEffects, u64) {
    let tx_data = tx_builder(cluster)
        .await
        .move_call(package_id, "m", function, args)
        .build();
    execute(cluster, tx_data).await
}

/// The derived object a claim sent to address 0.
fn claimed_object(cluster: &TestCluster, claim: &TransactionEffects) -> ObjectID {
    let sender = cluster.get_address_0();
    claim
        .created()
        .into_iter()
        .find(|(_, owner)| *owner == Owner::AddressOwner(sender))
        .expect("the claim created no object for the sender")
        .0
        .0
}

/// The items a listing streamed, the cursor of each, the removals among them, the last frame's
/// cursor, and the `QueryEnd` reason it ended with, or the error that failed it.
struct Listed {
    /// The version of each item, live or a removal.
    versions: Vec<u64>,
    /// Each item frame's cursor.
    cursors: Vec<Bytes>,
    /// The version, kind and transaction of each removal.
    removals: Vec<(u64, Kind, Option<String>)>,
    /// The last frame's cursor, once the listing has ended.
    end_cursor: Option<Bytes>,
    end: Result<QueryEndReason, tonic::Status>,
}

impl Listed {
    /// The position the last frame's cursor names (see [`decoded`]).
    fn end_position(&self) -> (u64, bool) {
        decoded(
            self.end_cursor
                .as_ref()
                .expect("the listing ended with a cursor"),
        )
    }
}

/// Run a listing to its end, checking the frame shape: a cursor on every frame, one item, a live
/// version or a removal, per item frame, `ITEM_LIMIT` only on an item frame, every other reason
/// on a final frame with no item, and nothing after the frame that ends the listing.
async fn list(
    client: &mut LedgerServiceClient<Channel>,
    request: ListObjectVersionsRequest,
) -> Listed {
    let mut listed = Listed {
        versions: Vec::new(),
        cursors: Vec::new(),
        removals: Vec::new(),
        end_cursor: None,
        end: Err(tonic::Status::unknown("listing has not ended")),
    };
    let mut stream = match client.list_object_versions(request).await {
        Ok(response) => response.into_inner(),
        Err(status) => {
            listed.end = Err(status);
            return listed;
        }
    };
    loop {
        let frame = match stream.message().await {
            Ok(Some(frame)) => frame,
            Ok(None) => panic!("listing ended without a QueryEnd"),
            Err(status) => {
                listed.end = Err(status);
                return listed;
            }
        };
        let end = frame.end.as_ref().map(|end| end.reason());
        let cursor = frame.cursor.clone().expect("every frame carries a cursor");
        let version = match &frame.item {
            Some(Item::Object(object)) => {
                Some(object.version.expect("listed object has a version"))
            }
            Some(Item::Removal(removal)) => {
                let version = removal.version.expect("a removal has a version");
                listed
                    .removals
                    .push((version, removal.kind(), removal.transaction.clone()));
                Some(version)
            }
            Some(item) => panic!("a frame carries an unknown item {item:?}"),
            None => None,
        };
        match version {
            Some(version) => {
                listed.versions.push(version);
                listed.cursors.push(cursor.clone());
                if end.is_none() {
                    continue;
                }
                assert_eq!(end, Some(QueryEndReason::ItemLimit));
            }
            None => assert_ne!(end, Some(QueryEndReason::ItemLimit)),
        }
        assert!(stream.message().await.unwrap().is_none());
        listed.end_cursor = Some(cursor);
        listed.end = Ok(end.expect("a frame without an item ends the listing"));
        return listed;
    }
}

/// The fullnode's rpc config, reading two versions per chunk so listings cross chunk boundaries.
fn two_version_chunks() -> RpcConfig {
    RpcConfig {
        enable_indexing: Some(true),
        ledger_history: Some(LedgerHistoryConfig {
            list_object_versions: Some(LedgerHistoryMethodConfig {
                chunk_max: Some(2),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// The version of `object_id` at the end of `checkpoint`, from `GetObject`.
async fn version_at_checkpoint(
    client: &mut LedgerServiceClient<Channel>,
    object_id: ObjectID,
    checkpoint: u64,
) -> u64 {
    let mut request = GetObjectRequest::new(&Address::from(object_id));
    request.at_checkpoint = Some(checkpoint);
    request.read_mask = Some(FieldMask::from_paths(["version"]));
    client
        .get_object(request)
        .await
        .unwrap()
        .into_inner()
        .object
        .unwrap()
        .version
        .unwrap()
}

/// An ascending listing of every version of `object_id`, reading only versions, and each removal
/// with its transaction.
fn request(object_id: ObjectID) -> ListObjectVersionsRequest {
    let mut request = ListObjectVersionsRequest::default();
    request.object_id = Some(object_id.to_canonical_string(true));
    request.read_mask = Some(FieldMask::from_paths(["version"]));
    request.removal_read_mask = Some(FieldMask::from_paths(["version", "kind", "transaction"]));
    request
}

/// `request` with the default removal mask, which leaves out each removal's transaction.
fn without_removal_transactions(
    mut request: ListObjectVersionsRequest,
) -> ListObjectVersionsRequest {
    request.removal_read_mask = None;
    request
}

/// `request` in descending order.
fn descending(mut request: ListObjectVersionsRequest) -> ListObjectVersionsRequest {
    request.options.get_or_insert_default().ordering = Some(Ordering::Descending as i32);
    request
}

/// The cursor of `version` minted by a listing bounded by the genesis checkpoint, which every
/// server has indexed.
fn cursor(object_id: ObjectID, version: u64) -> Bytes {
    cursor_bounded_by(object_id, version, 0)
}

/// The cursor of `version` minted by a listing bounded by `bounding_checkpoint`.
fn cursor_bounded_by(object_id: ObjectID, version: u64, bounding_checkpoint: u64) -> Bytes {
    ObjectVersionCursor::Version {
        object_id,
        version,
        bounding_checkpoint,
    }
    .encode()
}

/// The cursor of the boundary between `version` and the version after it, minted by a listing
/// bounded by `bounding_checkpoint`.
fn boundary_bounded_by(object_id: ObjectID, version: u64, bounding_checkpoint: u64) -> Bytes {
    ObjectVersionCursor::Boundary {
        object_id,
        version,
        bounding_checkpoint,
    }
    .encode()
}

/// The position `cursor` names: its version, and whether it is a boundary.
fn decoded(cursor: &Bytes) -> (u64, bool) {
    match bcs::from_bytes(cursor).unwrap() {
        ObjectVersionCursor::Version { version, .. } => (version, false),
        ObjectVersionCursor::Boundary { version, .. } => (version, true),
    }
}

/// Turning back from each of `cursors`, in either order, as `after` or as `before`, lists
/// exactly the items of `object`, whose versions are `versions` in ascending order, on the
/// cursor's side of it: above it as `after`, and as `before` below it, or at or below it for a
/// boundary.
async fn assert_turns_back_from(
    client: &mut LedgerServiceClient<Channel>,
    object: ObjectID,
    versions: &[u64],
    cursors: &[Bytes],
) {
    for cursor in cursors {
        let (position, boundary) = decoded(cursor);
        for listing in [request(object), descending(request(object))] {
            for as_after in [true, false] {
                let mut turned = listing.clone();
                let options = turned.options.get_or_insert_default();
                if as_after {
                    options.after = Some(cursor.clone());
                } else {
                    options.before = Some(cursor.clone());
                }
                let is_descending = turned.options().ordering() == Ordering::Descending;
                let mut expected: Vec<u64> = versions
                    .iter()
                    .copied()
                    .filter(|&listed| {
                        if as_after {
                            listed > position
                        } else {
                            listed < position || (boundary && listed == position)
                        }
                    })
                    .collect();
                if is_descending {
                    expected.reverse();
                }
                let listed = list(client, turned).await;
                assert_eq!(
                    listed.versions, expected,
                    "from {position} (boundary {boundary}), descending {is_descending}, as \
                     after {as_after}"
                );
            }
        }
    }
}

/// Asserts `status` is `OutOfRange` with an `ABOVE_LATEST_AVAILABLE` ErrorInfo for the bound
/// "cursor". The chain keeps running, so the checkpoints it names are not pinned.
fn assert_cursor_above_latest_available(status: &tonic::Status) {
    let status = status.rpc_status();
    assert_eq!(status.code, tonic::Code::OutOfRange as i32, "{status:?}");
    let error_info = error_info(&status);
    assert_eq!(
        error_info.reason,
        ErrorReason::AboveLatestAvailable.as_str_name()
    );
    assert_eq!(error_info.metadata["bound"], "cursor");
}

/// A derived object claimed, released, and then claimed again straight into a wrapper and
/// released from it three times lists every removal, each with the transaction that made it. The
/// wrapped re-claims change the object without writing an item of it, so its releases are a run
/// of four tombstones, which two-version chunks split. The run is listed in both orders at the
/// top of the object's history.
#[sim_test]
async fn list_object_versions_reclaimed_into_a_wrapper() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
        .with_rpc_config(two_version_chunks())
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let package_id = publish_derived_object_release(&cluster).await;
    let (create_parent, _) = call_derived(&cluster, package_id, "create_parent", vec![]).await;
    let parent = created_object(&create_parent);
    let (claim, _) = call_derived(
        &cluster,
        package_id,
        "claim_owned",
        vec![
            owned_arg(output_ref(&create_parent, parent)),
            pure_arg(&0u64),
        ],
    )
    .await;
    let object = claimed_object(&cluster, &claim);
    let (release, _) = call_derived(
        &cluster,
        package_id,
        "release",
        vec![
            owned_arg(output_ref(&claim, parent)),
            owned_arg(output_ref(&claim, object)),
        ],
    )
    .await;
    let mut releases = vec![release];
    for value in 1..=3u64 {
        let last_release = releases.last().unwrap();
        let (claim_wrapped, _) = call_derived(
            &cluster,
            package_id,
            "claim_wrapped",
            vec![
                owned_arg(output_ref(last_release, parent)),
                pure_arg(&value),
            ],
        )
        .await;
        let wrapper = claimed_object(&cluster, &claim_wrapped);
        assert!(
            claim_wrapped
                .object_changes()
                .iter()
                .any(|change| change.id == object
                    && change.input_version.is_none()
                    && change.output_version.is_none()
                    && change.id_operation == IDOperation::Created),
            "the re-claim creates the object without writing an item of it"
        );
        let (release_wrapped, _) = call_derived(
            &cluster,
            package_id,
            "release_wrapped",
            vec![
                owned_arg(output_ref(&claim_wrapped, parent)),
                owned_arg(output_ref(&claim_wrapped, wrapper)),
            ],
        )
        .await;
        releases.push(release_wrapped);
    }
    cluster.wait_for_rpc_index_ready().await;

    let version =
        |effects: &sui_types::effects::TransactionEffects| effects.lamport_version().value();
    let removals: Vec<_> = releases
        .iter()
        .map(|release| {
            (
                version(release),
                Kind::Deleted,
                Some(release.transaction_digest().to_string()),
            )
        })
        .collect();
    let versions: Vec<_> = std::iter::once(&claim)
        .chain(&releases)
        .map(version)
        .collect();
    assert_lists_in_both_orders(&mut client, object, &versions, &removals).await;
}

/// A derived object that transactions of one checkpoint release, claim again straight into a
/// wrapper and release again lists every release with the transaction that made it. The version
/// index records only the last of them for that checkpoint, and each is found among the object's
/// transactions there, past those that follow it. Once pruning makes that checkpoint the first
/// after the floor, the object is listed from the first release in it: the index places only the
/// last, and the object's transactions in that checkpoint show the earlier ones.
#[sim_test]
async fn list_object_versions_released_twice_in_one_checkpoint() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        // Only the explicit prune below moves the floor.
        .disable_fullnode_pruning()
        .with_rpc_config(two_version_chunks())
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let package_id = publish_derived_object_release(&cluster).await;
    let (create_parent, _) =
        call_derived(&cluster, package_id, "create_shared_parent", vec![]).await;
    let parent = output_ref(&create_parent, created_object(&create_parent));
    let parent_arg = CallArg::Object(ObjectArg::SharedObject {
        id: parent.0,
        initial_shared_version: parent.1,
        mutability: SharedObjectMutability::Mutable,
    });

    // The checkpoint and effects of each release, and a checkpoint that holds two of them.
    let mut releases: Vec<(u64, TransactionEffects)> = Vec::new();
    let mut busy_checkpoint = None;
    for _ in 0..10 {
        // Each `toggle` releases the object, or claims it straight into the shared parent,
        // whichever the one before it did not. One transaction per gas coin, submitted together
        // so that consensus tends to sequence them into one checkpoint.
        let mut toggles = Vec::new();
        for sender in cluster.get_addresses().into_iter().take(3) {
            let gas_coins = cluster
                .wallet
                .get_all_gas_objects_owned_by_address(sender)
                .await
                .unwrap();
            for gas in gas_coins.into_iter().take(2) {
                let toggle = cluster
                    .test_transaction_builder_with_gas_object(sender, gas)
                    .await
                    .move_call(package_id, "m", "toggle", vec![parent_arg.clone()])
                    .build();
                toggles.push(toggle);
            }
        }
        let round =
            futures::future::join_all(toggles.into_iter().map(|toggle| execute(&cluster, toggle)))
                .await;
        releases.extend(
            round
                .into_iter()
                .filter(|(effects, _)| !effects.unwrapped_then_deleted().is_empty())
                .map(|(effects, checkpoint)| (checkpoint, effects)),
        );
        busy_checkpoint = releases
            .iter()
            .map(|&(checkpoint, _)| checkpoint)
            .find(|busy| {
                releases
                    .iter()
                    .filter(|(checkpoint, _)| checkpoint == busy)
                    .count()
                    >= 2
            });
        if busy_checkpoint.is_some() {
            break;
        }
    }
    let busy_checkpoint =
        busy_checkpoint.expect("no round of concurrent transactions shared a checkpoint");
    cluster.wait_for_rpc_index_ready().await;

    let version = |effects: &TransactionEffects| effects.lamport_version().value();
    releases.sort_by_key(|(_, effects)| version(effects));
    let object = releases[0].1.unwrapped_then_deleted()[0].0;
    let versions: Vec<u64> = releases
        .iter()
        .map(|(_, effects)| version(effects))
        .collect();
    let removals: Vec<_> = releases
        .iter()
        .map(|(_, effects)| {
            (
                version(effects),
                Kind::Deleted,
                Some(effects.transaction_digest().to_string()),
            )
        })
        .collect();
    assert_lists_in_both_orders(&mut client, object, &versions, &removals).await;

    let floor = busy_checkpoint - 1;
    prune_through(&cluster, floor, false).await;
    let released_in_busy: Vec<u64> = releases
        .iter()
        .filter(|&&(checkpoint, _)| checkpoint == busy_checkpoint)
        .map(|(_, effects)| version(effects))
        .collect();
    let oldest = released_in_busy[0];
    assert!(released_in_busy.len() >= 2);
    assert!(is_tombstone_stored(&cluster, object, oldest));
    let retained: Vec<u64> = versions
        .iter()
        .copied()
        .filter(|&version| version >= oldest)
        .collect();
    assert_listed_from(&mut client, request(object), floor, &retained).await;
}

/// `object` lists `versions`, and among them `removals`, ascending to the indexed tip, and in
/// reverse descending, down to its first version, also when the descending listing starts at the
/// second-to-last removal, below another.
async fn assert_lists_in_both_orders(
    client: &mut LedgerServiceClient<Channel>,
    object: ObjectID,
    versions: &[u64],
    removals: &[(u64, Kind, Option<String>)],
) {
    let listed = list(client, request(object)).await;
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);
    assert_eq!(listed.versions, versions);
    assert_eq!(listed.removals, removals);

    let listed = list(client, descending(request(object))).await;
    assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);
    let descending_versions: Vec<u64> = versions.iter().rev().copied().collect();
    let descending_removals: Vec<_> = removals.iter().rev().cloned().collect();
    assert_eq!(listed.versions, descending_versions);
    assert_eq!(listed.removals, descending_removals);

    let start = removals[removals.len() - 2].0;
    let mut from_inside = descending(request(object));
    from_inside.end_version = Some(start + 1);
    let listed = list(client, from_inside).await;
    assert!(listed.end.is_ok());
    let is_listed = |version: &u64| *version <= start;
    assert_eq!(
        listed.versions,
        descending_versions
            .iter()
            .copied()
            .filter(is_listed)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        listed.removals,
        descending_removals
            .iter()
            .filter(|(version, ..)| is_listed(version))
            .cloned()
            .collect::<Vec<_>>()
    );
}
