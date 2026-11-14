// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
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
/// cursors and `at_checkpoint`, with the `QueryEnd` reason each bound ends a listing with. An
/// ascending `after` cursor above the object's version at the indexed tip ends at the tip; a
/// descending one is `OUT_OF_RANGE` unless `at_checkpoint` bounds the listing.
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
    for (cursor, version) in listed.cursors.iter().zip_debug_eq(&item_versions) {
        assert_eq!(
            bcs::from_bytes::<ObjectVersionCursor>(cursor).unwrap(),
            ObjectVersionCursor::Version {
                object_id: object,
                version: *version,
            },
        );
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

    // `at_checkpoint` lists the items written by the end of the checkpoint, and ends there.
    for checkpoint in create_cp - 1..=delete_cp {
        let mut bounded = request(object);
        bounded.at_checkpoint = Some(checkpoint);
        let visible: Vec<u64> = items
            .iter()
            .filter(|&&(written, _)| written <= checkpoint)
            .map(|&(_, version)| version)
            .collect();
        let listed = list(&mut client, bounded.clone()).await;
        assert_eq!(listed.versions, visible, "at checkpoint {checkpoint}");
        assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);
        let listed = list(&mut client, descending(bounded)).await;
        assert_eq!(
            listed.versions,
            visible.iter().rev().copied().collect::<Vec<_>>(),
            "descending at checkpoint {checkpoint}"
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

    // An ascending `after` cursor above the object's version at the indexed tip lists nothing
    // and ends at the tip, as the ledger List APIs do. A descending listing from such a cursor
    // would skip versions this server has not indexed, so it fails unless `at_checkpoint` bounds
    // the listing.
    let past_tip = cursor(object, version(&delete) + 1);
    let mut after_past_tip = request(object);
    after_past_tip.options.get_or_insert_default().after = Some(past_tip.clone());
    let listed = list(&mut client, after_past_tip).await;
    assert!(listed.versions.is_empty());
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);
    let mut before_past_tip = descending(request(object));
    before_past_tip.options.get_or_insert_default().before = Some(past_tip.clone());
    let listed = list(&mut client, before_past_tip.clone()).await;
    assert!(listed.versions.is_empty());
    assert_cursor_above_latest_available(&listed.end.unwrap_err());
    let mut bounded = before_past_tip;
    bounded.at_checkpoint = Some(delete_cp);
    let listed = list(&mut client, bounded).await;
    assert_eq!(listed.versions, descending_versions);
    assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);

    let mut unindexed = request(object);
    unindexed.at_checkpoint = Some(u64::MAX);
    let listed = list(&mut client, unindexed).await;
    assert_above_latest_available(&listed.end.unwrap_err(), "at_checkpoint", u64::MAX);
}

/// Pruning object versions through a checkpoint keeps each object's version at its end, the
/// oldest retained version. A listing never starts or ends inside the pruned versions without
/// saying so: an ascending listing whose range reaches below the oldest retained version fails
/// before listing anything, and a descending one lists the retained versions and then fails, both
/// naming that version. A range starting at it is served normally. An object with no retained
/// versions is refused, whatever its cursors.
#[sim_test]
async fn list_object_versions_below_oldest_retained_version() {
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

    let pruned_to = tip - 1;
    prune_through(&cluster, pruned_to, false).await;
    let oldest = version_at_checkpoint(&mut client, clock, pruned_to).await;
    let retained: Vec<u64> = history
        .iter()
        .copied()
        .filter(|&version| version >= oldest)
        .collect();
    let retained_descending: Vec<u64> = retained.iter().rev().copied().collect();
    assert_eq!(retained.first(), Some(&oldest));
    assert!(
        history.len() > retained.len() + 1,
        "the prune removed fewer than two versions: {history:?}, oldest retained {oldest}",
    );

    // Ascending: a range reaching below the oldest retained version fails before listing
    // anything, whether or not the request sets its own lower bound.
    let mut below_floor = vec![request(clock)];
    let mut start_below = request(clock);
    start_below.start_version = Some(oldest - 1);
    below_floor.push(start_below.clone());
    let mut after_pruned = request(clock);
    after_pruned.options.get_or_insert_default().after = Some(cursor(clock, history[0]));
    below_floor.push(after_pruned);
    let mut ending_at_oldest = request(clock);
    ending_at_oldest.end_version = Some(oldest);
    below_floor.push(ending_at_oldest.clone());
    for ascending in below_floor {
        let listed = list(&mut client, ascending.clone()).await;
        assert!(listed.versions.is_empty(), "{ascending:?}");
        assert_below_oldest_retained_version(&listed.end.unwrap_err(), pruned_to, oldest);
    }
    let mut from_oldest = request(clock);
    from_oldest.start_version = Some(oldest);
    let listed = list(&mut client, from_oldest.clone()).await;
    assert_eq!(listed.versions, retained);
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);

    // Descending: a range reaching below it lists the retained versions, then fails.
    for descending_below in [descending(request(clock)), descending(start_below)] {
        let listed = list(&mut client, descending_below).await;
        assert_eq!(listed.versions, retained_descending);
        assert_below_oldest_retained_version(&listed.end.unwrap_err(), pruned_to, oldest);
    }
    // A range wholly below it has no retained versions to list.
    let listed = list(&mut client, descending(ending_at_oldest)).await;
    assert!(listed.versions.is_empty());
    assert_below_oldest_retained_version(&listed.end.unwrap_err(), pruned_to, oldest);
    let listed = list(&mut client, descending(from_oldest)).await;
    assert_eq!(listed.versions, retained_descending);
    assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);
    // Reaching the limit at the oldest retained version ends the page normally.
    let mut limited = descending(request(clock));
    limited.options.get_or_insert_default().limit = Some(retained.len() as u32);
    let listed = list(&mut client, limited).await;
    assert_eq!(listed.versions, retained_descending);
    assert_eq!(listed.end.unwrap(), QueryEndReason::ItemLimit);

    // `at_checkpoint` at the floor checkpoint lists its last version; below it, the object
    // state may be pruned.
    let mut at_floor = request(clock);
    at_floor.start_version = Some(oldest);
    at_floor.at_checkpoint = Some(pruned_to);
    let listed = list(&mut client, at_floor.clone()).await;
    assert_eq!(listed.versions, [oldest]);
    assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);
    at_floor.at_checkpoint = Some(pruned_to - 1);
    let listed = list(&mut client, at_floor).await;
    assert_below_earliest_available(&listed.end.unwrap_err(), pruned_to);

    // No retained version, as for an object that never existed: refused, even with a cursor above
    // the tip.
    let unknown = ObjectID::random();
    let mut past_tip = request(unknown);
    past_tip.options.get_or_insert_default().after = Some(cursor(unknown, u64::MAX - 1));
    for refused in [request(unknown), descending(request(unknown)), past_tip] {
        let listed = list(&mut client, refused).await;
        assert!(listed.versions.is_empty());
        assert_below_earliest_available(&listed.end.unwrap_err(), pruned_to);
    }
}

/// On a node that has pruned, an object created after the floor keeps its whole history,
/// removals included: the node reads the effects of the transaction that wrote its first version
/// to tell it apart from an object whose earlier history was pruned, and finds each removal's
/// transaction above the floor.
#[sim_test]
async fn list_object_versions_created_after_the_floor() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        // Only the explicit prune below moves the floor.
        .disable_fullnode_pruning()
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let package_id = publish_basics(&cluster).await;
    let floor = fullnode_highest_executed(&cluster);
    prune_through(&cluster, floor, false).await;

    let sender = cluster.get_address_0();
    let (create, create_cp) = call_object_basics(
        &cluster,
        package_id,
        "create",
        vec![pure_arg(&7u64), pure_arg(&sender)],
    )
    .await;
    assert!(
        create_cp > floor,
        "created at {create_cp}, at or below the floor {floor}"
    );
    let object = created_object(&create);
    let (set_value, _) = call_object_basics(
        &cluster,
        package_id,
        "set_value",
        vec![owned_arg(output_ref(&create, object)), pure_arg(&8u64)],
    )
    .await;
    let (wrap, _) = call_object_basics(
        &cluster,
        package_id,
        "wrap",
        vec![owned_arg(output_ref(&set_value, object))],
    )
    .await;
    let wrapper = created_object(&wrap);
    let (unwrap, _) = call_object_basics(
        &cluster,
        package_id,
        "unwrap",
        vec![owned_arg(output_ref(&wrap, wrapper))],
    )
    .await;
    let (delete, _) = call_object_basics(
        &cluster,
        package_id,
        "delete",
        vec![owned_arg(output_ref(&unwrap, object))],
    )
    .await;
    cluster.wait_for_rpc_index_ready().await;

    let version =
        |effects: &sui_types::effects::TransactionEffects| effects.lamport_version().value();
    let digest = |effects: &sui_types::effects::TransactionEffects| {
        Some(effects.transaction_digest().to_string())
    };
    let versions = [
        version(&create),
        version(&set_value),
        version(&wrap),
        version(&unwrap),
        version(&delete),
    ];
    let removals = [
        (version(&wrap), Kind::Wrapped, digest(&wrap)),
        (version(&delete), Kind::Deleted, digest(&delete)),
    ];
    let listed = list(&mut client, request(object)).await;
    assert_eq!(listed.versions, versions);
    assert_eq!(listed.removals, removals);
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);
    let listed = list(&mut client, descending(request(object))).await;
    assert_eq!(
        listed.versions,
        versions.iter().rev().copied().collect::<Vec<_>>()
    );
    assert_eq!(
        listed.removals,
        removals.iter().rev().cloned().collect::<Vec<_>>()
    );
    assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);
}

/// A derived object claimed, released and claimed again lists both incarnations under its one ID,
/// the release as a `DELETED` removal with its own transaction. On a node that has pruned, an
/// object claimed after the floor is listed from its first claim: a derived object's ID may have
/// been used before, so its creation does not show that it began the ID's history.
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

    let listed = list(&mut client, request(object)).await;
    assert!(listed.versions.is_empty());
    assert_below_oldest_retained_version(&listed.end.unwrap_err(), floor, versions[0]);

    let mut from_first_claim = request(object);
    from_first_claim.start_version = Some(versions[0]);
    let listed = list(&mut client, from_first_claim).await;
    assert_eq!(listed.versions, versions);
    assert_eq!(listed.removals, removals);
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);

    let listed = list(&mut client, descending(request(object))).await;
    assert_eq!(
        listed.versions,
        versions.iter().rev().copied().collect::<Vec<_>>()
    );
    assert_eq!(listed.removals, removals);
    assert_below_oldest_retained_version(&listed.end.unwrap_err(), floor, versions[0]);
}

/// A derived object released at or before the floor and claimed again after it is listed from
/// the re-claim, passing over the release's tombstone, which the pruner keeps as with
/// `killswitch_tombstone_pruning`: the rpc indexes no longer place the release's transaction
/// after the floor, and the re-claim's creation does not show that it began the ID's history.
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

    let listed = list(&mut client, request(object)).await;
    assert!(listed.versions.is_empty());
    assert_below_oldest_retained_version(&listed.end.unwrap_err(), floor, reclaimed);

    let mut from_reclaim = request(object);
    from_reclaim.start_version = Some(reclaimed);
    let listed = list(&mut client, from_reclaim).await;
    assert_eq!(listed.versions, [reclaimed]);
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);

    let listed = list(&mut client, descending(request(object))).await;
    assert_eq!(listed.versions, [reclaimed]);
    assert_below_oldest_retained_version(&listed.end.unwrap_err(), floor, reclaimed);
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

    // The pruner keeps running, so the floor checkpoint the errors name is not pinned.
    let listed = list(&mut client, request(object)).await;
    assert!(listed.versions.is_empty());
    assert_oldest_retained_version(&listed.end.unwrap_err(), reclaimed);

    let mut from_reclaim = request(object);
    from_reclaim.start_version = Some(reclaimed);
    let listed = list(&mut client, from_reclaim).await;
    assert_eq!(listed.versions, [reclaimed]);
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);

    let listed = list(&mut client, descending(request(object))).await;
    assert_eq!(listed.versions, [reclaimed]);
    assert_oldest_retained_version(&listed.end.unwrap_err(), reclaimed);
}

/// An object wrapped at or before the floor and unwrapped after it is listed from the unwrap,
/// whether the pruner deleted the wrap's tombstone or kept it: its unwrap did not create it, so
/// its earlier versions are pruned history.
#[sim_test]
async fn list_object_versions_unwrapped_after_the_floor() {
    for keep_tombstones in [false, true] {
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
        prune_through(&cluster, unwrapped.floor, keep_tombstones).await;
        assert!(!is_stored(&cluster, unwrapped.object, unwrapped.created));
        assert_listed_from(
            &mut client,
            request(unwrapped.object),
            unwrapped.floor,
            &unwrapped.versions,
        )
        .await;
    }
}

/// A derived object released at or before the floor, then claimed again after it straight into a
/// wrapper and released from it, is listed from that last release, passing over the first
/// release's tombstone, which the pruner keeps as with `killswitch_tombstone_pruning`: the
/// wrapped re-claim, after the floor, is not the first release's transaction.
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
/// places the last release, whose effects are pruned, through the rpc indexes, and lists the
/// object from it. One that reports them has the checkpoint floor, at or after the last release,
/// so the object keeps nothing, until it is claimed again after that floor, from which it is
/// then listed.
#[sim_test]
async fn list_object_versions_deleted_with_pruned_effects() {
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
    prune_through(&cluster, floor, false).await;

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
    let release_digest = *release_wrapped.transaction_digest();
    cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        // Pruning deletes the effects from the store, not from the cache.
        state
            .cache_for_testing()
            .evict_executed_effects_from_cache_for_testing(&release_digest);
        assert!(
            state
                .get_transaction_cache_reader()
                .get_executed_effects(&release_digest)
                .is_none(),
            "the last release's effects are pruned"
        );
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

    for listing in [request(object), descending(request(object))] {
        let listed = list(&mut client, listing).await;
        assert!(listed.versions.is_empty());
        assert_below_earliest_available(&listed.end.unwrap_err(), checkpoint_floor);
    }

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

/// While the pruner has pruned the rpc-store but not yet deleted the object versions it prunes,
/// objects are listed from their first item after the floor, passing over the versions and
/// tombstones the pruner has yet to delete: one wrapped at or before the floor and unwrapped
/// after it, and a derived object released, claimed again and released again at or before the
/// floor, and claimed again after it. The pruned transaction behind a leftover version cannot be
/// placed, so neither can the release before it.
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
/// item of the object, fails the listing with the new floor rather than ending it as if it had
/// listed everything.
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
        prune_through(&cluster, delete_cp, false),
    )
    .await;
    assert!(listed.versions.is_empty());
    assert_below_earliest_available(&listed.end.unwrap_err(), delete_cp);
}

/// Pruning that runs after a listing is planned and before it reads anything, and raises the
/// object's oldest retained version above where the listing starts, fails the listing with the
/// new floor rather than skipping the pruned versions.
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
    call_object_basics(
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
    assert!(listed.versions.is_empty());
    assert_below_oldest_retained_version(
        &listed.end.unwrap_err(),
        floor,
        set_value.lamport_version().value(),
    );
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
/// items `retained` lists in ascending order: listings reaching below it fail, after listing the
/// retained items if descending, and one starting at it is served in either order.
async fn assert_listed_from(
    client: &mut LedgerServiceClient<Channel>,
    listing: ListObjectVersionsRequest,
    floor: u64,
    retained: &[u64],
) {
    let oldest = retained[0];
    let retained_descending: Vec<u64> = retained.iter().rev().copied().collect();
    for start_version in [None, Some(oldest - 1)] {
        let mut ascending = listing.clone();
        ascending.start_version = start_version;
        let listed = list(client, ascending).await;
        assert!(listed.versions.is_empty());
        assert_below_oldest_retained_version(&listed.end.unwrap_err(), floor, oldest);
    }

    let listed = list(client, descending(listing.clone())).await;
    assert_eq!(listed.versions, retained_descending);
    assert_below_oldest_retained_version(&listed.end.unwrap_err(), floor, oldest);

    let mut from_oldest = listing;
    from_oldest.start_version = Some(oldest);
    let listed = list(client, from_oldest.clone()).await;
    assert_eq!(listed.versions, retained);
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);
    let listed = list(client, descending(from_oldest)).await;
    assert_eq!(listed.versions, retained_descending);
    assert_eq!(listed.end.unwrap(), QueryEndReason::CheckpointBound);
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

/// The items a listing streamed, the cursor of each, the removals among them, and the `QueryEnd`
/// reason it ended with, or the error that failed it.
struct Listed {
    /// The version of each item, live or a removal.
    versions: Vec<u64>,
    cursors: Vec<Bytes>,
    /// The version, kind and transaction of each removal.
    removals: Vec<(u64, Kind, Option<String>)>,
    end: Result<QueryEndReason, tonic::Status>,
}

/// Run a listing to its end, checking the frame shape: one item, a live version or a removal, and
/// its cursor per item frame, `ITEM_LIMIT` only on an item frame, every other reason on a final
/// frame with no item, and nothing after the frame that ends the listing.
async fn list(
    client: &mut LedgerServiceClient<Channel>,
    request: ListObjectVersionsRequest,
) -> Listed {
    let mut listed = Listed {
        versions: Vec::new(),
        cursors: Vec::new(),
        removals: Vec::new(),
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
        let version = match (&frame.object, &frame.removal) {
            (Some(object), None) => Some(object.version.expect("listed object has a version")),
            (None, Some(removal)) => {
                let version = removal.version.expect("a removal has a version");
                listed
                    .removals
                    .push((version, removal.kind(), removal.transaction.clone()));
                Some(version)
            }
            (None, None) => None,
            (Some(_), Some(_)) => panic!("a frame carries both a live version and a removal"),
        };
        match version {
            Some(version) => {
                listed.versions.push(version);
                listed
                    .cursors
                    .push(frame.cursor.expect("an item frame carries its cursor"));
                if end.is_none() {
                    continue;
                }
                assert_eq!(end, Some(QueryEndReason::ItemLimit));
            }
            None => {
                assert!(frame.cursor.is_none(), "an end frame carries no cursor");
                assert_ne!(end, Some(QueryEndReason::ItemLimit));
            }
        }
        assert!(stream.message().await.unwrap().is_none());
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

fn cursor(object_id: ObjectID, version: u64) -> Bytes {
    ObjectVersionCursor::Version { object_id, version }.encode()
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

/// Asserts `status` is `OutOfRange` with a `BELOW_EARLIEST_AVAILABLE` ErrorInfo naming the floor
/// checkpoint and the object's oldest retained version.
fn assert_below_oldest_retained_version(
    status: &tonic::Status,
    floor_checkpoint: u64,
    version: u64,
) {
    let status = status.rpc_status();
    assert_eq!(status.code, tonic::Code::OutOfRange as i32, "{status:?}");
    let error_info = error_info(&status);
    assert_eq!(
        error_info.reason,
        ErrorReason::BelowEarliestAvailable.as_str_name()
    );
    assert_eq!(error_info.domain, ErrorReason::DOMAIN);
    assert_eq!(
        error_info.metadata,
        BTreeMap::from([
            (
                "lowestAvailableCheckpoint".to_owned(),
                floor_checkpoint.to_string()
            ),
            ("lowestAvailableVersion".to_owned(), version.to_string()),
        ])
    );
}

/// Asserts `status` is `OutOfRange` with a `BELOW_EARLIEST_AVAILABLE` ErrorInfo naming the
/// object's oldest retained version, whatever floor checkpoint it names.
#[cfg(msim)]
fn assert_oldest_retained_version(status: &tonic::Status, version: u64) {
    let status = status.rpc_status();
    assert_eq!(status.code, tonic::Code::OutOfRange as i32, "{status:?}");
    let error_info = error_info(&status);
    assert_eq!(
        error_info.reason,
        ErrorReason::BelowEarliestAvailable.as_str_name()
    );
    assert!(
        error_info
            .metadata
            .contains_key("lowestAvailableCheckpoint"),
        "{error_info:?}"
    );
    assert_eq!(
        error_info.metadata.get("lowestAvailableVersion"),
        Some(&version.to_string())
    );
}

/// A derived object claimed, released, claimed again straight into a wrapper and released from
/// it lists both removals, each with the transaction that made it. The wrapped re-claim changes
/// the object without writing an item of it.
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
    let (claim_wrapped, _) = call_derived(
        &cluster,
        package_id,
        "claim_wrapped",
        vec![owned_arg(output_ref(&release, parent)), pure_arg(&1u64)],
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
    cluster.wait_for_rpc_index_ready().await;

    let version =
        |effects: &sui_types::effects::TransactionEffects| effects.lamport_version().value();
    let digest = |effects: &sui_types::effects::TransactionEffects| {
        Some(effects.transaction_digest().to_string())
    };
    let listed = list(&mut client, request(object)).await;
    assert_eq!(listed.end.unwrap(), QueryEndReason::LedgerTip);
    assert_eq!(
        listed.versions,
        [
            version(&claim),
            version(&release),
            version(&release_wrapped)
        ]
    );
    assert_eq!(
        listed.removals,
        [
            (version(&release), Kind::Deleted, digest(&release)),
            (
                version(&release_wrapped),
                Kind::Deleted,
                digest(&release_wrapped)
            ),
        ]
    );
}
