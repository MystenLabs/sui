// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_common::ZipDebugEqIteratorExt;
use prost::Message;
use sui_macros::sim_test;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::google::rpc::BadRequest;
use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsRequest;
use sui_rpc::proto::sui::rpc::v2::BatchGetObjectsResponse;
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
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Owner;
use sui_types::transaction::CallArg;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::TransactionData;
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;

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
/// that checkpoint, through creation, mutation, wrapping, unwrapping, and deletion.
#[sim_test]
async fn get_object_at_checkpoint_lifecycle() {
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

    // Drive an object through its lifecycle, one transaction per step, recording the state each
    // step leaves behind as (checkpoint, object, version if live). Wrapping creates a wrapper
    // object, and unwrapping deletes it.
    let sender = cluster.get_address_0();
    let mut changes: Vec<(u64, ObjectID, Option<u64>)> = Vec::new();
    let mut step_checkpoints = Vec::new();

    let (create, cp) = call_object_basics(
        &cluster,
        package_id,
        "create",
        vec![pure_arg(&7u64), pure_arg(&sender)],
    )
    .await;
    let object = created_object(&create);
    changes.push((cp, object, Some(create.lamport_version().value())));
    step_checkpoints.push(cp);

    let (set_value, cp) = call_object_basics(
        &cluster,
        package_id,
        "set_value",
        vec![owned_arg(output_ref(&create, object)), pure_arg(&8u64)],
    )
    .await;
    changes.push((cp, object, Some(set_value.lamport_version().value())));
    step_checkpoints.push(cp);

    let (wrap, cp) = call_object_basics(
        &cluster,
        package_id,
        "wrap",
        vec![owned_arg(output_ref(&set_value, object))],
    )
    .await;
    let wrapper = created_object(&wrap);
    changes.push((cp, object, None));
    changes.push((cp, wrapper, Some(wrap.lamport_version().value())));
    step_checkpoints.push(cp);

    let (unwrap, cp) = call_object_basics(
        &cluster,
        package_id,
        "unwrap",
        vec![owned_arg(output_ref(&wrap, wrapper))],
    )
    .await;
    changes.push((cp, object, Some(unwrap.lamport_version().value())));
    changes.push((cp, wrapper, None));
    step_checkpoints.push(cp);

    let (_delete, cp) = call_object_basics(
        &cluster,
        package_id,
        "delete",
        vec![owned_arg(output_ref(&unwrap, object))],
    )
    .await;
    changes.push((cp, object, None));
    step_checkpoints.push(cp);

    // Each step waits for its checkpoint before the next step is built, so every step lands in
    // its own checkpoint.
    assert!(
        step_checkpoints.windows(2).all(|pair| pair[0] < pair[1]),
        "lifecycle steps share a checkpoint: {step_checkpoints:?}",
    );

    let first_cp = step_checkpoints[0];
    let last_cp = *step_checkpoints.last().unwrap();

    // `at_checkpoint` reads are bounded by the rpc index's highest indexed checkpoint.
    cluster.wait_for_rpc_index_ready().await;

    // Every checkpoint from just before creation through deletion, for both objects. Checkpoints
    // between two changes resolve to the earlier change.
    let mut cases: Vec<(Address, u64, Option<u64>)> = Vec::new();
    for checkpoint in first_cp - 1..=last_cp {
        for id in [object, wrapper] {
            let expected_version = changes
                .iter()
                .rev()
                .find(|(cp, changed, _)| *changed == id && *cp <= checkpoint)
                .and_then(|(_, _, version)| *version);
            cases.push((Address::from(id), checkpoint, expected_version));
        }
    }

    for &(id, checkpoint, expected_version) in &cases {
        let outcome = single_outcome(
            client
                .get_object(at_checkpoint_request(id, checkpoint))
                .await,
        );
        assert_at_checkpoint(outcome, id, checkpoint, expected_version);
    }

    // One batch covering every case twice: results come back in request order, and duplicate
    // items each get their own result.
    let batch_cases: Vec<_> = cases.iter().chain(cases.iter()).copied().collect();
    let BatchGetObjectsResponse { objects, .. } = client
        .batch_get_objects({
            let mut message = BatchGetObjectsRequest::default();
            message.requests = batch_cases
                .iter()
                .map(|&(id, checkpoint, _)| at_checkpoint_request(id, checkpoint))
                .collect();
            message
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(objects.len(), batch_cases.len());
    for (result, &(id, checkpoint, expected_version)) in objects.iter().zip_debug_eq(&batch_cases) {
        assert_at_checkpoint(batch_outcome(result), id, checkpoint, expected_version);
    }
}

/// A checkpoint above the rpc index's highest indexed checkpoint is rejected rather than served
/// from the latest state.
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

    let error = client
        .get_object(at_checkpoint_request(id, u64::MAX))
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::NotFound);
    assert!(
        error.message().contains("is not yet indexed"),
        "{}",
        error.message()
    );

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
            assert_eq!(status.code, tonic::Code::NotFound as i32);
            assert!(
                status.message.contains("is not yet indexed"),
                "{}",
                status.message
            );
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

/// A checkpoint below the fullnode's lowest available checkpoint is rejected as pruned.
#[sim_test]
async fn get_object_at_checkpoint_below_available_floor() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        // Only the watermark bump below moves the floor.
        .disable_fullnode_pruning()
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Wait for the embedded rpc-store to index genesis before the first index read.
    cluster.wait_for_rpc_index_ready().await;

    let id: Address = "0x5".parse().unwrap();

    // The genesis checkpoint is servable before the floor is raised.
    let outcome = single_outcome(client.get_object(at_checkpoint_request(id, 0)).await);
    assert_at_checkpoint(outcome, id, 0, Some(1));

    // Raise the serving floor by bumping the checkpoint store's pruned watermark past genesis,
    // which moves the floor `get_lowest_available_checkpoint` reports.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
    let pruned_to = loop {
        let highest_executed = cluster.fullnode_handle.sui_node.with(|node| {
            node.state()
                .get_checkpoint_store()
                .get_highest_executed_checkpoint()
                .unwrap()
        });
        if let Some(checkpoint) = highest_executed
            && *checkpoint.sequence_number() >= 1
        {
            cluster.fullnode_handle.sui_node.with(|node| {
                node.state()
                    .get_checkpoint_store()
                    .update_highest_pruned_checkpoint(&checkpoint)
                    .unwrap()
            });
            break *checkpoint.sequence_number();
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for the fullnode to execute a checkpoint above genesis"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    };

    for checkpoint in [0, pruned_to] {
        let error = client
            .get_object(at_checkpoint_request(id, checkpoint))
            .await
            .unwrap_err();
        assert_eq!(error.code(), tonic::Code::NotFound);
        assert!(
            error.message().contains("has been pruned"),
            "{}",
            error.message()
        );
    }

    // The first checkpoint above the floor still resolves once it is executed and indexed.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let highest_executed = cluster.fullnode_handle.sui_node.with(|node| {
            node.state()
                .get_checkpoint_store()
                .get_highest_executed_checkpoint_seq_number()
                .expect("db error")
                .unwrap_or(0)
        });
        if highest_executed > pruned_to {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for the fullnode to execute a checkpoint above {pruned_to}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    cluster.wait_for_rpc_index_ready().await;

    // Inside a batch, a pruned checkpoint fails only its own item.
    let BatchGetObjectsResponse { objects, .. } = client
        .batch_get_objects({
            let mut message = BatchGetObjectsRequest::default();
            message.requests = vec![
                at_checkpoint_request(id, 0),
                at_checkpoint_request(id, pruned_to + 1),
                at_checkpoint_request(id, pruned_to),
            ];
            message
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(objects.len(), 3);

    for index in [0, 2] {
        match &objects[index].result {
            Some(get_object_result::Result::Error(status)) => {
                assert_eq!(status.code, tonic::Code::NotFound as i32);
                assert!(
                    status.message.contains("has been pruned"),
                    "{}",
                    status.message
                );
            }
            other => panic!("expected a per-item error at {index}, got {other:?}"),
        }
    }

    match &objects[1].result {
        Some(get_object_result::Result::Object(object)) => {
            assert_eq!(object.object_id, Some(id.to_string()));
            assert!(object.version.is_some());
        }
        other => panic!("expected an object, got {other:?}"),
    }
}

/// `version` and `at_checkpoint` are mutually exclusive; setting both fails the whole call.
#[sim_test]
async fn get_object_version_and_at_checkpoint_conflict() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let id: Address = "0x5".parse().unwrap();
    let conflicting = {
        let mut request = at_checkpoint_request(id, 0);
        request.version = Some(1);
        request
    };

    let error = client.get_object(conflicting.clone()).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(
        error.message().contains("mutually exclusive"),
        "{}",
        error.message()
    );

    // One conflicting item fails the whole batch, even alongside valid items.
    let error = client
        .batch_get_objects({
            let mut message = BatchGetObjectsRequest::default();
            message.requests = vec![
                GetObjectRequest::new(&id).with_version(1u64),
                conflicting,
                at_checkpoint_request(id, 0),
            ];
            message
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);

    let details = sui_rpc::proto::google::rpc::Status::decode(error.details())
        .unwrap()
        .details;
    let bad_request = details
        .iter()
        .find_map(|detail| detail.to_msg::<BadRequest>().ok())
        .expect("error status should carry a BadRequest detail");
    assert_eq!(bad_request.field_violations.len(), 1);
    assert_eq!(
        bad_request.field_violations[0].field,
        "requests[1].at_checkpoint"
    );
}

/// A request for `id` as of the end of `checkpoint`, reading its id and version.
fn at_checkpoint_request(id: Address, checkpoint: u64) -> GetObjectRequest {
    let mut request = GetObjectRequest::new(&id);
    request.at_checkpoint = Some(checkpoint);
    request.read_mask = Some(FieldMask::from_paths(["object_id", "version"]));
    request
}

/// A `GetObject` outcome: the object, or the error's code and message.
type ObjectOutcome = Result<Object, (tonic::Code, String)>;

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
        .map_err(|status| (status.code(), status.message().to_owned()))
}

fn batch_outcome(result: &GetObjectResult) -> ObjectOutcome {
    match &result.result {
        Some(get_object_result::Result::Object(object)) => Ok(object.clone()),
        Some(get_object_result::Result::Error(status)) => {
            Err((tonic::Code::from(status.code), status.message.clone()))
        }
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
        (Err((code, message)), None) => {
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

/// A transaction builder for address 0, paying with one of its current gas coins.
async fn tx_builder(cluster: &TestCluster) -> TestTransactionBuilder {
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
async fn execute(cluster: &TestCluster, tx_data: TransactionData) -> (TransactionEffects, u64) {
    let executed = cluster.sign_and_execute_transaction(&tx_data).await;
    let checkpoint = executed
        .checkpoint
        .expect("executed transaction has no checkpoint");
    (executed.effects, checkpoint)
}

/// Call `basics::object_basics::<function>` as address 0.
async fn call_object_basics(
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
fn output_ref(effects: &TransactionEffects, object_id: ObjectID) -> ObjectRef {
    effects
        .all_changed_objects()
        .into_iter()
        .map(|(object_ref, _, _)| object_ref)
        .find(|object_ref| object_ref.0 == object_id)
        .unwrap_or_else(|| panic!("transaction did not write {object_id}"))
}

/// The one object `effects` created. Panics unless exactly one object was created.
fn created_object(effects: &TransactionEffects) -> ObjectID {
    let created = effects.created();
    assert_eq!(created.len(), 1, "expected exactly one created object");
    created[0].0.0
}

fn owned_arg(object_ref: ObjectRef) -> CallArg {
    CallArg::Object(ObjectArg::ImmOrOwnedObject(object_ref))
}

fn pure_arg<T: serde::Serialize>(value: &T) -> CallArg {
    CallArg::Pure(bcs::to_bytes(value).unwrap())
}
