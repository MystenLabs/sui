// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use mysten_common::ZipDebugEqIteratorExt;
use prost::Message;
use sui_macros::sim_test;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::google::rpc::BadRequest;
use sui_rpc::proto::google::rpc::ErrorInfo;
use sui_rpc::proto::google::rpc::Status as RpcStatus;
use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsResponse;
use sui_rpc::proto::sui::rpc::v2::ErrorReason;
use sui_rpc::proto::sui::rpc::v2::GetObjectRequest;
use sui_rpc::proto::sui::rpc::v2::GetObjectResponse;
use sui_rpc::proto::sui::rpc::v2::GetObjectResult;
use sui_rpc::proto::sui::rpc::v2::Object;
use sui_rpc::proto::sui::rpc::v2::get_object_result;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_sdk_types::Address;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Owner;
use sui_types::transaction::CallArg;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::TransactionData;
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;

use crate::v2::RpcStatusExt;
use crate::v2::assert_above_latest_available;
use crate::v2::assert_below_earliest_available;
use crate::v2::halt_chain;
use crate::v2::prune_objects_and_rpc_store_below_tip;

#[sim_test]
async fn get_object() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;

    let id: Address = "0x5".parse().unwrap();

    let mut client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Request with no provided read_mask
    let Object {
        bcs,
        object_id,
        version,
        digest,
        owner,
        object_type,
        has_public_transfer,
        contents,
        package,
        previous_transaction,
        storage_rebate,
        json,
        ..
    } = client
        .get_object(GetObjectRequest::new(&id))
        .await
        .unwrap()
        .into_inner()
        .object
        .unwrap();

    // These fields default to being read
    assert_eq!(object_id, Some(id.to_string()));
    assert!(version.is_some());
    assert!(digest.is_some());

    // while these fields default to not being read
    assert!(owner.is_none());
    assert!(bcs.is_none());
    assert!(object_type.is_none());
    assert!(has_public_transfer.is_none());
    assert!(contents.is_none());
    assert!(package.is_none());
    assert!(previous_transaction.is_none());
    assert!(storage_rebate.is_none());
    assert!(json.is_none());

    // Request with provided read_mask
    let Object {
        bcs,
        object_id,
        version,
        digest,
        owner,
        object_type,
        has_public_transfer,
        contents,
        package,
        previous_transaction,
        storage_rebate,
        json,
        ..
    } = client
        .get_object(
            GetObjectRequest::new(&id)
                .with_version(1u64)
                .with_read_mask(FieldMask::from_str("object_id,version")),
        )
        .await
        .unwrap()
        .into_inner()
        .object
        .unwrap();

    assert_eq!(object_id, Some(id.to_string()));
    assert_eq!(version, Some(1));

    // These fields were not requested
    assert!(digest.is_none());
    assert!(owner.is_none());
    assert!(bcs.is_none());
    assert!(object_type.is_none());
    assert!(has_public_transfer.is_none());
    assert!(contents.is_none());
    assert!(package.is_none());
    assert!(previous_transaction.is_none());
    assert!(storage_rebate.is_none());
    assert!(json.is_none());

    let response = client
        .get_object(
            GetObjectRequest::new(&id).with_read_mask(FieldMask::from_paths([
                "object_id",
                "version",
                "digest",
                "bcs",
                "json",
            ])),
        )
        .await
        .unwrap()
        .into_inner()
        .object
        .unwrap();

    let Object {
        bcs,
        object_id,
        version,
        digest,
        owner,
        object_type,
        has_public_transfer,
        contents,
        package,
        previous_transaction,
        storage_rebate,
        json,
        ..
    } = &response;

    assert!(object_id.is_some());
    assert!(version.is_some());
    assert!(digest.is_some());
    assert!(bcs.is_some());

    assert!(owner.is_none());
    assert!(object_type.is_none());
    assert!(has_public_transfer.is_none());
    assert!(contents.is_none());
    assert!(package.is_none());
    assert!(previous_transaction.is_none());
    assert!(storage_rebate.is_none());
    assert!(json.is_some());
}

#[sim_test]
async fn batch_get_objects() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;

    let mut client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let BatchGetObjectsResponse { objects, .. } = client
        .batch_get_objects({
            let mut message = BatchGetObjectsRequest::default();
            message.requests = vec![
                GetObjectRequest::new(&Address::from_hex("0x1").unwrap()),
                GetObjectRequest::new(&Address::from_hex("0x2").unwrap()),
                GetObjectRequest::new(&Address::from_hex("0x3").unwrap()),
            ];
            message
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(
        objects[0].object().object_id,
        Some("0x1".parse::<Address>().unwrap().to_string())
    );
    assert_eq!(
        objects[1].object().object_id,
        Some("0x2".parse::<Address>().unwrap().to_string())
    );
    assert_eq!(
        objects[2].object().object_id,
        Some("0x3".parse::<Address>().unwrap().to_string())
    );
}

/// Reads pinned to a checkpoint see the state left by the object's latest change at or before
/// that checkpoint, through creation, mutation, wrapping, unwrapping, and deletion, and at a
/// checkpoint in which the live object did not change. `max_version` reads see the newest
/// version at or below the bound, and `NotFound` from a deletion or wrap until the object is
/// unwrapped. `at_checkpoint` with `version` keeps that version only if it was written by the
/// end of the checkpoint, and with `max_version` fails if the version it selects was written
/// after the checkpoint, as Postgres does.
#[sim_test]
async fn get_object_lifecycle() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        // Keep every checkpoint servable; the fullnode prunes old checkpoints by default.
        .disable_fullnode_pruning()
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let publish_tx = tx_builder(&cluster)
        .await
        .publish_examples("basics")
        .await
        .build();
    let (publish, _) = execute(&cluster, publish_tx).await;
    let package_id = publish
        .created()
        .into_iter()
        .find(|(_, owner)| *owner == Owner::Immutable)
        .expect("publish created no package")
        .0
        .0;

    // Drive an object through its lifecycle, one transaction per step, recording each change as
    // (checkpoint, object, version, live). `live` is false for a change that deleted or wrapped
    // the object. Wrapping creates a wrapper object, and unwrapping deletes it. Each removal is
    // also recorded with its kind and transaction.
    let sender = cluster.get_address_0();
    let mut changes: Vec<(u64, ObjectID, u64, bool)> = Vec::new();
    let mut removals: HashMap<(ObjectID, u64), (&str, TransactionDigest)> = HashMap::new();
    let mut step_checkpoints = Vec::new();

    let (create, cp) = call_object_basics(
        &cluster,
        package_id,
        "create",
        vec![pure_arg(&7u64), pure_arg(&sender)],
    )
    .await;
    let object = created_object(&create);
    changes.push((cp, object, create.lamport_version().value(), true));
    step_checkpoints.push(cp);
    // Bump the gas coin `create` paid with in a checkpoint of its own, in which the object is live
    // but unchanged, and pay for `set_value` with that coin, so the object skips a version between
    // creation and the mutation.
    let bump_tx = cluster
        .test_transaction_builder_with_gas_object(sender, create.gas_object().unwrap().0)
        .await
        .transfer_sui(None, sender)
        .build();
    let (bump, _) = execute(&cluster, bump_tx).await;

    let set_value_tx = cluster
        .test_transaction_builder_with_gas_object(sender, bump.gas_object().unwrap().0)
        .await
        .move_call(
            package_id,
            "object_basics",
            "set_value",
            vec![owned_arg(output_ref(&create, object)), pure_arg(&8u64)],
        )
        .build();
    let (set_value, cp) = execute(&cluster, set_value_tx).await;
    changes.push((cp, object, set_value.lamport_version().value(), true));
    step_checkpoints.push(cp);

    let (wrap, cp) = call_object_basics(
        &cluster,
        package_id,
        "wrap",
        vec![owned_arg(output_ref(&set_value, object))],
    )
    .await;
    let wrapper = created_object(&wrap);
    changes.push((cp, object, wrap.lamport_version().value(), false));
    changes.push((cp, wrapper, wrap.lamport_version().value(), true));
    removals.insert(
        (object, wrap.lamport_version().value()),
        ("WRAPPED", *wrap.transaction_digest()),
    );
    step_checkpoints.push(cp);

    let (unwrap, cp) = call_object_basics(
        &cluster,
        package_id,
        "unwrap",
        vec![owned_arg(output_ref(&wrap, wrapper))],
    )
    .await;
    changes.push((cp, object, unwrap.lamport_version().value(), true));
    changes.push((cp, wrapper, unwrap.lamport_version().value(), false));
    removals.insert(
        (wrapper, unwrap.lamport_version().value()),
        ("DELETED", *unwrap.transaction_digest()),
    );
    step_checkpoints.push(cp);

    let (delete, cp) = call_object_basics(
        &cluster,
        package_id,
        "delete",
        vec![owned_arg(output_ref(&unwrap, object))],
    )
    .await;
    changes.push((cp, object, delete.lamport_version().value(), false));
    removals.insert(
        (object, delete.lamport_version().value()),
        ("DELETED", *delete.transaction_digest()),
    );
    step_checkpoints.push(cp);

    // Each step waits for its checkpoint before the next step is built, so every step lands in
    // its own checkpoint.
    assert!(
        step_checkpoints.windows(2).all(|pair| pair[0] < pair[1]),
        "lifecycle steps share a checkpoint: {step_checkpoints:?}",
    );
    assert!(
        step_checkpoints[1] > step_checkpoints[0] + 1,
        "no checkpoint passed between creation and mutation: {step_checkpoints:?}",
    );

    let first_cp = step_checkpoints[0];
    let last_cp = *step_checkpoints.last().unwrap();

    // `at_checkpoint` reads are bounded by the rpc index's highest indexed checkpoint.
    cluster.wait_for_rpc_index_ready().await;

    // The expected outcome of each read, from the changes: the change a read selects, kept only
    // if it was made by the end of the bound. A live change answers with its version; a removal
    // answers `NotFound` describing it, with the transaction the bitmap finds for it, since the
    // fullnode keeps every checkpoint.
    let changes_of = |id: ObjectID| {
        changes
            .iter()
            .filter(move |(_, changed, _, _)| *changed == id)
    };
    let bounded =
        |selected: Option<&(u64, ObjectID, u64, bool)>, at_checkpoint: Option<u64>| match selected
            .filter(|&&(changed_at, _, _, _)| at_checkpoint.is_none_or(|bound| changed_at <= bound))
        {
            Some(&(_, _, version, true)) => Expected::Found(version),
            Some(&(_, id, version, false)) => {
                let (kind, transaction) = removals[&(id, version)];
                Expected::Removed((version, kind.to_owned(), Some(transaction.to_string())))
            }
            None => Expected::NotFound,
        };

    // The latest state of both objects: each was last deleted.
    let mut cases: Vec<(GetObjectRequest, Expected, String)> = Vec::new();
    for id in [object, wrapper] {
        let mut request = GetObjectRequest::new(&Address::from(id));
        request.read_mask = Some(FieldMask::from_paths(["object_id", "version"]));
        let expected = bounded(changes_of(id).next_back(), None);
        assert!(matches!(expected, Expected::Removed(_)), "{expected:?}");
        cases.push((
            request,
            expected,
            format!("Object {} not found", Address::from(id)),
        ));
    }

    // Every checkpoint from just before creation through deletion, for both objects. Checkpoints
    // between two changes resolve to the earlier change.
    for checkpoint in first_cp - 1..=last_cp {
        for id in [object, wrapper] {
            let latest_change =
                changes_of(id).rfind(|(changed_at, _, _, _)| *changed_at <= checkpoint);
            cases.push((
                at_checkpoint_request(Address::from(id), checkpoint),
                bounded(latest_change, None),
                format!(
                    "Object {} at checkpoint {checkpoint} not found",
                    Address::from(id)
                ),
            ));
        }
    }

    // Every version from just before creation to just after deletion, read exactly and as a
    // `max_version`, unbounded and bounded by each of those checkpoints, for both objects.
    let versions = changes.iter().map(|&(_, _, version, _)| version);
    let first_version = versions.clone().min().unwrap() - 1;
    let last_version = versions.max().unwrap() + 1;
    let bounds = std::iter::once(None).chain((first_cp - 1..=last_cp).map(Some));
    for id in [object, wrapper] {
        let address = Address::from(id);
        for version in first_version..=last_version {
            for at_checkpoint in bounds.clone() {
                let at = at_checkpoint
                    .map(|checkpoint| format!(" at checkpoint {checkpoint}"))
                    .unwrap_or_default();
                let exact = changes_of(id).find(|(_, _, changed, _)| *changed == version);
                cases.push((
                    selector_request(address, Some(version), None, at_checkpoint),
                    bounded(exact, at_checkpoint),
                    format!("Object {address} with version {version}{at} not found"),
                ));
                // The newest change at or below `version`, kept only if it was made by the bound.
                let at_most = changes_of(id).rfind(|(_, _, changed, _)| *changed <= version);
                cases.push((
                    selector_request(address, None, Some(version), at_checkpoint),
                    bounded(at_most, at_checkpoint),
                    format!("Object {address} with version at most {version}{at} not found"),
                ));
            }
        }
    }
    // Every removal is described by some read.
    for &(_, version) in removals.keys() {
        assert!(
            cases.iter().any(
                |(_, expected, _)| matches!(expected, Expected::Removed((v, ..)) if *v == version)
            ),
            "no read selects the removal at {version}"
        );
    }

    for (request, expected, not_found) in &cases {
        let outcome = single_outcome(client.get_object(request.clone()).await);
        assert_outcome(outcome, request, expected, not_found);
    }

    // Batches covering every case twice: results come back in request order, and duplicate
    // items each get their own result.
    let batch_cases: Vec<_> = cases.iter().chain(cases.iter()).collect();
    for chunk in batch_cases.chunks(500) {
        let BatchGetObjectsResponse { objects, .. } = client
            .batch_get_objects({
                let mut message = BatchGetObjectsRequest::default();
                message.requests = chunk
                    .iter()
                    .map(|(request, _, _)| request.clone())
                    .collect();
                message
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(objects.len(), chunk.len());
        for (result, (request, expected, not_found)) in objects.iter().zip_debug_eq(chunk) {
            assert_outcome(batch_outcome(result), request, expected, not_found);
        }
    }

    // At the checkpoint that created the object, `max_version` set to the created version reads
    // it, and set to the mutation's version selects the mutation, written after that checkpoint,
    // so the read fails rather than fall back to the created version. At the mutation's
    // checkpoint, the mutation answers.
    let (create_cp, _, create_version, _) = changes[0];
    let (set_value_cp, _, set_value_version, _) = changes[1];
    for (max_version, at_checkpoint, expected) in [
        (create_version, create_cp, Expected::Found(create_version)),
        (set_value_version, create_cp, Expected::NotFound),
        (
            set_value_version,
            set_value_cp,
            Expected::Found(set_value_version),
        ),
    ] {
        let request = selector_request(
            Address::from(object),
            None,
            Some(max_version),
            Some(at_checkpoint),
        );
        let not_found = format!(
            "Object {} with version at most {max_version} at checkpoint {at_checkpoint} not found",
            Address::from(object),
        );
        let outcome = single_outcome(client.get_object(request.clone()).await);
        assert_outcome(outcome, &request, &expected, &not_found);
    }

    // The object skips a version between creation and the mutation, so the cases above include
    // a `max_version` between two of its versions. One in that gap reads the created version,
    // unbounded and bounded by the mutation's checkpoint.
    let in_gap = create_version + 1;
    assert!(in_gap < set_value_version, "no version gap: {changes:?}");
    for at_checkpoint in [None, Some(set_value_cp)] {
        let request = selector_request(Address::from(object), None, Some(in_gap), at_checkpoint);
        let outcome = single_outcome(client.get_object(request.clone()).await);
        assert_outcome(outcome, &request, &Expected::Found(create_version), "");
    }

    // Setting both `version` and `max_version` is invalid, and fails the whole batch.
    let both = selector_request(
        Address::from(object),
        Some(set_value_version),
        Some(set_value_version),
        None,
    );
    let error = client.get_object(both.clone()).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(
        error.message().contains("mutually exclusive"),
        "{}",
        error.message()
    );
    assert_eq!(bad_request_fields(&error), ["requests[0].max_version"]);

    let error = client
        .batch_get_objects({
            let mut message = BatchGetObjectsRequest::default();
            message.requests = vec![
                GetObjectRequest::new(&Address::from(object)),
                selector_request(Address::from(object), Some(set_value_version), None, None),
                both,
            ];
            message
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert_eq!(bad_request_fields(&error), ["requests[2].max_version"]);
}

/// A checkpoint above the rpc index's highest indexed checkpoint is rejected rather than served
/// from the latest state, including as the bound of a `version` or `max_version` read.
#[sim_test]
async fn get_object_at_checkpoint_not_yet_indexed() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Wait for the embedded rpc-store to index genesis before the first index read.
    cluster.wait_for_rpc_index_ready().await;

    let id: Address = "0x5".parse().unwrap();

    for request in [
        at_checkpoint_request(id, u64::MAX),
        selector_request(id, Some(1), None, Some(u64::MAX)),
        selector_request(id, None, Some(1), Some(u64::MAX)),
    ] {
        let error = client.get_object(request.clone()).await.unwrap_err();
        assert_above_latest_available(&error, "at_checkpoint", u64::MAX);
    }

    // So is an object that does not exist, rather than `NotFound`.
    let unknown = Address::from(ObjectID::random());
    let error = client
        .get_object(at_checkpoint_request(unknown, u64::MAX))
        .await
        .unwrap_err();
    assert_above_latest_available(&error, "at_checkpoint", u64::MAX);

    // Inside a batch, the out-of-range checkpoint fails only its own item.
    let BatchGetObjectsResponse { objects, .. } = client
        .batch_get_objects({
            let mut message = BatchGetObjectsRequest::default();
            message.requests = vec![
                at_checkpoint_request(id, 0),
                at_checkpoint_request(id, u64::MAX),
                GetObjectRequest::new(&id)
                    .with_read_mask(FieldMask::from_paths(["object_id", "version"])),
            ];
            message
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(objects.len(), 3);

    assert_at_checkpoint(batch_outcome(&objects[0]), id, 0, Some(1));

    match &objects[1].result {
        Some(get_object_result::Result::Error(status)) => {
            assert_above_latest_available(status, "at_checkpoint", u64::MAX);
        }
        other => panic!("expected a per-item error, got {other:?}"),
    }

    match &objects[2].result {
        Some(get_object_result::Result::Object(object)) => {
            assert_eq!(object.object_id, Some(id.to_string()));
            assert!(object.version.is_some());
        }
        other => panic!("expected an object, got {other:?}"),
    }
}

/// Pruning object versions through a checkpoint leaves the state at its end readable and rejects
/// every checkpoint below it as pruned, including as the bound of a `version` or `max_version`
/// read. A `version` or `max_version` read of a version the prune deleted is rejected at the
/// object lower bound, the checkpoint after the last pruned one, with or without `at_checkpoint`;
/// a version above the object's latest is `NOT_FOUND`.
#[sim_test]
async fn get_object_at_checkpoint_below_available_floor() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        // Only the explicit prune below moves the floor.
        .disable_fullnode_pruning()
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // The clock changes in every checkpoint, so the prune deletes the version each checkpoint
    // below the last pruned one ended with.
    let id: Address = "0x6".parse().unwrap();
    let tip = halt_chain(&cluster).await;
    let pruned_to = tip - 1;

    let version_at = |outcome: ObjectOutcome| outcome.unwrap().version.unwrap();
    let before_end = version_at(single_outcome(
        client
            .get_object(at_checkpoint_request(id, pruned_to - 1))
            .await,
    ));
    let at_end = version_at(single_outcome(
        client
            .get_object(at_checkpoint_request(id, pruned_to))
            .await,
    ));
    assert!(before_end < at_end, "the clock changes in every checkpoint");

    assert_eq!(
        prune_objects_and_rpc_store_below_tip(&cluster).await,
        pruned_to
    );

    // The last pruned checkpoint still reads the version it ended with; the version it replaced
    // is gone, so every checkpoint below is rejected.
    let outcome = single_outcome(
        client
            .get_object(at_checkpoint_request(id, pruned_to))
            .await,
    );
    assert_at_checkpoint(outcome, id, pruned_to, Some(at_end));
    for checkpoint in [0, pruned_to - 1] {
        for request in [
            at_checkpoint_request(id, checkpoint),
            selector_request(id, Some(1), None, Some(checkpoint)),
            selector_request(id, None, Some(1), Some(checkpoint)),
        ] {
            let error = client.get_object(request.clone()).await.unwrap_err();
            assert_below_earliest_available(&error, pruned_to);
        }
    }

    // The version the checkpoint before the last pruned one ended with is gone, so reading it
    // exactly or as a `max_version` fails at the object lower bound, also bounded by a checkpoint
    // the at_checkpoint lower bound accepts. A version above the latest does not exist yet.
    let objects_lower_bound = pruned_to + 1;
    for request in [
        selector_request(id, Some(before_end), None, None),
        selector_request(id, None, Some(before_end), None),
        selector_request(id, Some(before_end), None, Some(pruned_to)),
        selector_request(id, None, Some(before_end), Some(pruned_to)),
    ] {
        let error = client.get_object(request.clone()).await.unwrap_err();
        assert_below_earliest_available(&error, objects_lower_bound);
    }
    let outcome = single_outcome(
        client
            .get_object(selector_request(id, Some(u64::MAX), None, None))
            .await,
    );
    assert_eq!(
        outcome.unwrap_err(),
        (
            tonic::Code::NotFound,
            format!("Object {id} with version {} not found", u64::MAX),
            None
        )
    );

    // Inside a batch, a pruned checkpoint or version fails only its own item.
    let BatchGetObjectsResponse { objects, .. } = client
        .batch_get_objects({
            let mut message = BatchGetObjectsRequest::default();
            message.requests = vec![
                at_checkpoint_request(id, pruned_to - 1),
                at_checkpoint_request(id, pruned_to),
                selector_request(id, Some(before_end), None, None),
                selector_request(id, Some(at_end), None, None),
            ];
            message
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(objects.len(), 4);
    for (item, floor) in [(0, pruned_to), (2, objects_lower_bound)] {
        match &objects[item].result {
            Some(get_object_result::Result::Error(status)) => {
                assert_below_earliest_available(status, floor);
            }
            other => panic!("expected a per-item error, got {other:?}"),
        }
    }
    assert_at_checkpoint(batch_outcome(&objects[1]), id, pruned_to, Some(at_end));
    assert_eq!(batch_outcome(&objects[3]).unwrap().version, Some(at_end));
}

/// A request for `id` as of the end of `checkpoint`, reading its id and version.
fn at_checkpoint_request(id: Address, checkpoint: u64) -> GetObjectRequest {
    let mut request = GetObjectRequest::new(&id);
    request.at_checkpoint = Some(checkpoint);
    request.read_mask = Some(FieldMask::from_paths(["object_id", "version"]));
    request
}

/// A request for `id` with the given `version`, `max_version` and `at_checkpoint`, reading its id
/// and version.
fn selector_request(
    id: Address,
    version: Option<u64>,
    max_version: Option<u64>,
    at_checkpoint: Option<u64>,
) -> GetObjectRequest {
    let mut request = GetObjectRequest::new(&id);
    request.version = version;
    request.max_version = max_version;
    request.at_checkpoint = at_checkpoint;
    request.read_mask = Some(FieldMask::from_paths(["object_id", "version"]));
    request
}

/// A removal an `OBJECT_REMOVED` detail describes: `(removedVersion, removalKind, transaction)`.
type Removal = (u64, String, Option<String>);

/// A `GetObject` outcome: the object, or the error's code and message, and the removal its
/// `OBJECT_REMOVED` detail describes, if it has one.
type ObjectOutcome = Result<Object, (tonic::Code, String, Option<Removal>)>;

/// What a read is expected to answer.
#[derive(Debug)]
enum Expected {
    /// The object at this version.
    Found(u64),
    /// `NotFound` with no `OBJECT_REMOVED` detail.
    NotFound,
    /// `NotFound` with an `OBJECT_REMOVED` detail describing this removal.
    Removed(Removal),
}

/// The removal `status`'s `OBJECT_REMOVED` detail describes, if it has one.
fn removal(status: &RpcStatus) -> Option<Removal> {
    let info = status
        .details
        .iter()
        .find_map(|detail| detail.to_msg::<ErrorInfo>().ok())?;
    assert_eq!(
        info.reason,
        ErrorReason::ObjectRemoved.as_str_name(),
        "{info:?}"
    );
    assert_eq!(info.domain, ErrorReason::DOMAIN, "{info:?}");
    Some((
        info.metadata["removedVersion"].parse().unwrap(),
        info.metadata["removalKind"].clone(),
        info.metadata.get("transaction").cloned(),
    ))
}

fn single_outcome(
    response: Result<tonic::Response<GetObjectResponse>, tonic::Status>,
) -> ObjectOutcome {
    response
        .map(|response| {
            response
                .into_inner()
                .object
                .expect("response has no object")
        })
        .map_err(|status| {
            let removal = removal(&status.rpc_status());
            (status.code(), status.message().to_owned(), removal)
        })
}

fn batch_outcome(result: &GetObjectResult) -> ObjectOutcome {
    match &result.result {
        Some(get_object_result::Result::Object(object)) => Ok(object.clone()),
        Some(get_object_result::Result::Error(status)) => Err((
            tonic::Code::from(status.code),
            status.message.clone(),
            removal(status),
        )),
        other => panic!("unexpected batch result {other:?}"),
    }
}

/// Assert that reading `id` at `checkpoint` returned `expected_version`, or `NotFound` when the
/// object was not live at the end of `checkpoint`.
fn assert_at_checkpoint(
    outcome: ObjectOutcome,
    id: Address,
    checkpoint: u64,
    expected_version: Option<u64>,
) {
    match (outcome, expected_version) {
        (Ok(object), Some(version)) => {
            assert_eq!(object.object_id, Some(id.to_string()));
            assert_eq!(
                object.version,
                Some(version),
                "{id} at checkpoint {checkpoint}"
            );
        }
        (Err((code, message, _)), None) => {
            assert_eq!(
                code,
                tonic::Code::NotFound,
                "{id} at checkpoint {checkpoint}"
            );
            assert_eq!(
                message,
                format!("Object {id} at checkpoint {checkpoint} not found")
            );
        }
        (outcome, expected_version) => panic!(
            "{id} at checkpoint {checkpoint}: expected version {expected_version:?}, \
             got {outcome:?}"
        ),
    }
}

/// Assert that `request` answered as `expected`, with the message `not_found` when it is not
/// found.
fn assert_outcome(
    outcome: ObjectOutcome,
    request: &GetObjectRequest,
    expected: &Expected,
    not_found: &str,
) {
    let context = format!(
        "{:?}: version {:?}, max_version {:?}, at_checkpoint {:?}",
        request.object_id, request.version, request.max_version, request.at_checkpoint,
    );
    match (outcome, expected) {
        (Ok(object), Expected::Found(version)) => {
            assert_eq!(object.object_id, request.object_id, "{context}");
            assert_eq!(object.version, Some(*version), "{context}");
        }
        (Err((code, message, removal)), Expected::NotFound | Expected::Removed(_)) => {
            assert_eq!(code, tonic::Code::NotFound, "{context}: {message}");
            assert_eq!(message, not_found, "{context}");
            let expected_removal = match expected {
                Expected::Removed(removal) => Some(removal),
                _ => None,
            };
            assert_eq!(removal.as_ref(), expected_removal, "{context}");
        }
        (outcome, expected) => panic!("{context}: expected {expected:?}, got {outcome:?}"),
    }
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

/// A transaction builder for address 0, paying with one of its current gas coins.
pub(super) async fn tx_builder(cluster: &TestCluster) -> TestTransactionBuilder {
    let sender = cluster.get_address_0();
    let gas = cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await
        .unwrap()
        .expect("no gas coin for address 0");
    cluster
        .test_transaction_builder_with_gas_object(sender, gas)
        .await
}

/// Sign and execute `tx_data` as address 0, wait for it to be checkpointed, and return its
/// effects and checkpoint.
pub(super) async fn execute(
    cluster: &TestCluster,
    tx_data: TransactionData,
) -> (TransactionEffects, u64) {
    let executed = cluster.sign_and_execute_transaction(&tx_data).await;
    let checkpoint = executed
        .checkpoint
        .expect("executed transaction has no checkpoint");
    (executed.effects, checkpoint)
}

/// Call `basics::object_basics::<function>` as address 0.
pub(super) async fn call_object_basics(
    cluster: &TestCluster,
    package_id: ObjectID,
    function: &'static str,
    args: Vec<CallArg>,
) -> (TransactionEffects, u64) {
    let tx_data = tx_builder(cluster)
        .await
        .move_call(package_id, "object_basics", function, args)
        .build();
    execute(cluster, tx_data).await
}

/// The reference `effects` wrote for `object_id`.
pub(super) fn output_ref(effects: &TransactionEffects, object_id: ObjectID) -> ObjectRef {
    effects
        .all_changed_objects()
        .into_iter()
        .map(|(object_ref, _, _)| object_ref)
        .find(|object_ref| object_ref.0 == object_id)
        .unwrap_or_else(|| panic!("transaction did not write {object_id}"))
}

/// The one object `effects` created. Panics unless exactly one object was created.
pub(super) fn created_object(effects: &TransactionEffects) -> ObjectID {
    let created = effects.created();
    assert_eq!(created.len(), 1, "expected exactly one created object");
    created[0].0.0
}

pub(super) fn owned_arg(object_ref: ObjectRef) -> CallArg {
    CallArg::Object(ObjectArg::ImmOrOwnedObject(object_ref))
}

pub(super) fn pure_arg<T: serde::Serialize>(value: &T) -> CallArg {
    CallArg::Pure(bcs::to_bytes(value).unwrap())
}
