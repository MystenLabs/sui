// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_keys::keystore::AccountKeystore;
use sui_light_client::mmr::apply_stream_updates;
use sui_light_client::proof::base::{Proof, ProofContents, ProofTarget, ProofVerifier};
use sui_light_client::proof::committee::extract_new_committee_info;
use sui_light_client::proof::ocs::{OCSInclusionProof, OCSProof, OCSTarget};
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2::{Event, GetCheckpointRequest, GetEpochRequest};
use sui_rpc_api::grpc::alpha::event_service_proto::event_service_client::EventServiceClient;
use sui_rpc_api::grpc::alpha::event_service_proto::{
    AuthenticatedEvent, GetObjectInclusionProofRequest, ListAuthenticatedEventsRequest,
};
use sui_sdk_types::ValidatorCommittee;
use sui_types::accumulator_root as ar;
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::digests::{Digest, ObjectDigest};
use sui_types::dynamic_field::{DynamicFieldKey, Field};
use sui_types::{MoveTypeTagTraitGeneric, SUI_ACCUMULATOR_ROOT_OBJECT_ID, SUI_FRAMEWORK_ADDRESS};
use move_core_types::language_storage::StructTag;
use std::collections::BTreeMap;
use std::str::FromStr;
use sui_types::accumulator_root::EventCommitment;
use sui_types::object::Object;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::TransactionData;
use test_cluster::{TestCluster, TestClusterBuilder};

fn create_rpc_config_with_authenticated_events() -> sui_config::RpcConfig {
    sui_config::RpcConfig {
        authenticated_events_indexing: Some(true),
        enable_indexing: Some(true),
        ..Default::default()
    }
}

async fn publish_test_package(test_cluster: &TestCluster) -> ObjectID {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/data/auth_event");

    let (sender, gas_object) = test_cluster
        .wallet
        .get_one_gas_object()
        .await
        .unwrap()
        .unwrap();
    let txn = test_cluster
        .wallet
        .sign_transaction(
            &sui_test_transaction_builder::TestTransactionBuilder::new(sender, gas_object, 1000)
                .with_gas_budget(50_000_000_000)
                .publish(path)
                .build(),
        )
        .await;
    let resp = test_cluster
        .wallet
        .execute_transaction_must_succeed(txn)
        .await;
    resp.get_new_package_obj().unwrap().0
}

async fn emit_test_event(
    test_cluster: &TestCluster,
    package_id: ObjectID,
    sender: SuiAddress,
    value: u64,
) {
    let rgp = test_cluster.get_reference_gas_price().await;
    let mut ptb = ProgrammableTransactionBuilder::new();
    let val = ptb.pure(value).unwrap();
    ptb.programmable_move_call(
        package_id,
        move_core_types::identifier::Identifier::new("events").unwrap(),
        move_core_types::identifier::Identifier::new("emit").unwrap(),
        vec![],
        vec![val],
    );
    let gas_object = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await
        .unwrap()
        .unwrap();
    let tx_data = TransactionData::new(
        sui_types::transaction::TransactionKind::ProgrammableTransaction(ptb.finish()),
        sender,
        gas_object,
        50_000_000_000,
        rgp,
    );
    test_cluster.sign_and_execute_transaction(&tx_data).await;
}

async fn query_authenticated_events(
    rpc_url: &str,
    stream_id: &str,
    start_checkpoint: u64,
    page_size: Option<u32>,
) -> Result<
    sui_rpc_api::grpc::alpha::event_service_proto::ListAuthenticatedEventsResponse,
    tonic::Status,
> {
    let mut client = EventServiceClient::connect(rpc_url.to_owned())
        .await
        .unwrap();

    let mut req = ListAuthenticatedEventsRequest::default();
    req.stream_id = Some(stream_id.to_string());
    req.start_checkpoint = Some(start_checkpoint);
    req.page_size = page_size;
    req.page_token = None;

    client
        .list_authenticated_events(req)
        .await
        .map(|r| r.into_inner())
}

fn proto_checkpoint_to_certified_checkpoint(
    proto_checkpoint: &sui_rpc::proto::sui::rpc::v2::Checkpoint,
) -> Result<sui_types::messages_checkpoint::CertifiedCheckpointSummary, String> {
    let summary: sui_types::messages_checkpoint::CheckpointSummary = proto_checkpoint
        .summary()
        .bcs()
        .deserialize()
        .map_err(|e| format!("Failed to deserialize checkpoint summary: {}", e))?;

    let proto_signature = proto_checkpoint.signature();
    let sdk_signature: sui_sdk_types::ValidatorAggregatedSignature = proto_signature
        .try_into()
        .map_err(|e| format!("Failed to convert proto signature to sdk types: {:?}", e))?;
    let signature = sui_types::crypto::AuthorityStrongQuorumSignInfo::from(sdk_signature);

    Ok(sui_types::messages_checkpoint::CertifiedCheckpointSummary::new_from_data_and_sig(
        summary,
        signature,
    ))
}

fn proto_object_ref_to_sui_object_ref(
    object_ref_proto: &sui_rpc_api::grpc::alpha::event_service_proto::ObjectRef,
) -> Result<(ObjectID, SequenceNumber, ObjectDigest), String> {
    let object_id_bytes = object_ref_proto
        .object_id
        .as_ref()
        .ok_or("Missing object_id")?;
    let object_id = ObjectID::from_bytes(object_id_bytes)
        .map_err(|e| format!("Invalid object_id: {}", e))?;

    let version = SequenceNumber::from_u64(
        object_ref_proto.version.ok_or("Missing version")?,
    );

    let digest_bytes = object_ref_proto.digest.as_ref().ok_or("Missing digest")?;
    let digest: [u8; 32] = digest_bytes
        .as_slice()
        .try_into()
        .map_err(|_| "Invalid digest length")?;
    let digest = ObjectDigest::new(digest);

    Ok((object_id, version, digest))
}

fn proto_bytes_to_digest(bytes: &[u8]) -> Result<Digest, String> {
    let digest: [u8; 32] = bytes
        .try_into()
        .map_err(|_| format!("Invalid digest length: expected 32, got {}", bytes.len()))?;
    Ok(Digest::new(digest))
}

fn proto_ocs_inclusion_proof_to_light_client_proof(
    grpc_proof: &sui_rpc_api::grpc::alpha::event_service_proto::OcsInclusionProof,
) -> Result<OCSInclusionProof, String> {
    let merkle_proof_bytes = grpc_proof
        .merkle_proof
        .as_ref()
        .ok_or("Missing merkle_proof")?;
    let merkle_proof = bcs::from_bytes(merkle_proof_bytes)
        .map_err(|e| format!("Failed to deserialize merkle_proof: {}", e))?;

    let leaf_index = grpc_proof.leaf_index.ok_or("Missing leaf_index")? as usize;

    let tree_root_bytes = grpc_proof.tree_root.as_ref().ok_or("Missing tree_root")?;
    let tree_root = proto_bytes_to_digest(tree_root_bytes)?;

    Ok(OCSInclusionProof {
        merkle_proof,
        leaf_index,
        tree_root,
    })
}

fn convert_grpc_event_to_commitment(
    auth_event: &AuthenticatedEvent,
) -> Result<EventCommitment, String> {
    let checkpoint = auth_event.checkpoint.ok_or("Missing checkpoint")?;
    let transaction_idx = auth_event.transaction_idx.ok_or("Missing transaction_idx")? as u64;
    let event_idx = auth_event.event_idx.ok_or("Missing event_idx")? as u64;

    let event = auth_event.event.as_ref().ok_or("Missing event")?;
    let bcs_contents = event.contents.as_ref().ok_or("Missing event contents")?;
    let bcs_bytes = bcs_contents.value.as_ref().ok_or("Missing BCS value")?;

    let package_id = event.package_id.as_ref().ok_or("Missing package_id")?;
    let module = event.module.as_ref().ok_or("Missing module")?;
    let sender = event.sender.as_ref().ok_or("Missing sender")?;
    let event_type = event.event_type.as_ref().ok_or("Missing event_type")?;

    let package_id = sui_types::base_types::ObjectID::from_hex_literal(package_id)
        .map_err(|e| format!("Failed to parse package_id: {}", e))?;
    let module = move_core_types::identifier::Identifier::new(module.as_str())
        .map_err(|e| format!("Failed to parse module: {}", e))?;
    let sender = sui_types::base_types::SuiAddress::from_str(sender)
        .map_err(|e| format!("Failed to parse sender: {}", e))?;
    let type_tag: move_core_types::language_storage::StructTag = event_type.parse()
        .map_err(|e| format!("Failed to parse event_type: {}", e))?;

    let sui_event = sui_types::event::Event {
        package_id,
        transaction_module: module,
        sender,
        type_: type_tag,
        contents: bcs_bytes.to_vec(),
    };

    let digest = sui_event.digest();

    Ok(EventCommitment::new(checkpoint, transaction_idx, event_idx, digest))
}

fn get_event_stream_head_object_id(
    stream_id: sui_types::base_types::SuiAddress,
) -> Result<sui_types::base_types::ObjectID, String> {
    let key = ar::AccumulatorKey { owner: stream_id };
    let type_tag = move_core_types::language_storage::TypeTag::Struct(Box::new(StructTag {
        address: SUI_FRAMEWORK_ADDRESS,
        module: ar::ACCUMULATOR_SETTLEMENT_MODULE.to_owned(),
        name: ar::ACCUMULATOR_SETTLEMENT_EVENT_STREAM_HEAD.to_owned(),
        type_params: vec![],
    }));
    let key_type_tag = ar::AccumulatorKey::get_type_tag(&[type_tag]);

    let field_id = DynamicFieldKey(SUI_ACCUMULATOR_ROOT_OBJECT_ID, key, key_type_tag)
        .into_unbounded_id()
        .map_err(|e| e.to_string())?
        .as_object_id();

    Ok(field_id)
}

async fn get_committee_for_epoch_via_api(
    ledger_client: &mut LedgerServiceClient<tonic::transport::Channel>,
    epoch: u64,
) -> Result<sui_types::committee::Committee, String> {
    let response = ledger_client
        .get_epoch(GetEpochRequest::new(epoch).with_read_mask(FieldMask::from_paths(["committee"])))
        .await
        .map_err(|e| format!("Failed to get epoch {} from API: {}", epoch, e))?
        .into_inner();

    let proto_committee = response
        .epoch
        .ok_or("Missing epoch in response")?
        .committee
        .ok_or("Missing committee in epoch response")?;

    let sdk_committee = ValidatorCommittee::try_from(&proto_committee)
        .map_err(|e| format!("Failed to convert proto committee to SDK committee: {:?}", e))?;

    Ok(sui_types::committee::Committee::from(sdk_committee))
}

async fn get_committee_for_checkpoint(
    test_cluster: &test_cluster::TestCluster,
    target_checkpoint_seq: u64,
) -> Result<sui_types::committee::Committee, String> {
    let mut ledger_client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .map_err(|e| format!("Failed to connect to LedgerService: {}", e))?;

    let state = test_cluster.fullnode_handle.sui_node.state();
    let target_checkpoint = state
        .get_checkpoint_by_sequence_number(target_checkpoint_seq)
        .map_err(|e| format!("Failed to get target checkpoint {}: {}", target_checkpoint_seq, e))?
        .ok_or_else(|| format!("Target checkpoint {} not found", target_checkpoint_seq))?;

    let target_epoch = target_checkpoint.epoch();

    tracing::info!("========================================");
    tracing::info!("LIGHT CLIENT TRUST RATCHETING DEMO");
    tracing::info!("========================================");
    tracing::info!("Target checkpoint: {}", target_checkpoint_seq);
    tracing::info!("Target epoch: {}", target_epoch);
    tracing::info!("");

    tracing::info!("Step 1: Fetching genesis committee (epoch 0) via get_epoch API...");
    let mut current_committee = get_committee_for_epoch_via_api(&mut ledger_client, 0).await?;
    tracing::info!("  Genesis committee loaded:");
    tracing::info!("    Epoch: {}", current_committee.epoch);
    tracing::info!("    Validators: {}", current_committee.voting_rights.len());
    tracing::info!("    Total stake: {}", current_committee.total_votes());
    tracing::info!("");

    if current_committee.epoch >= target_epoch {
        tracing::info!("Target epoch is genesis epoch, no trust ratcheting needed");
        return Ok(current_committee);
    }

    tracing::info!("Step 2: Trust ratcheting through epochs {} to {}...", current_committee.epoch, target_epoch - 1);
    tracing::info!("");

    for epoch in current_committee.epoch..target_epoch {
        tracing::info!("  Epoch {} -> {} transition:", epoch, epoch + 1);

        tracing::info!("    Fetching end-of-epoch checkpoint via get_epoch API...");
        let epoch_response = ledger_client
            .get_epoch(GetEpochRequest::new(epoch).with_read_mask(FieldMask::from_paths(["last_checkpoint"])))
            .await
            .map_err(|e| format!("Failed to get epoch {} info: {}", epoch, e))?
            .into_inner();

        let end_of_epoch_checkpoint_seq = epoch_response
            .epoch
            .ok_or("Missing epoch in response")?
            .last_checkpoint
            .ok_or_else(|| format!("Epoch {} has no last_checkpoint", epoch))?;

        tracing::info!("    End-of-epoch checkpoint: {}", end_of_epoch_checkpoint_seq);

        tracing::info!("    Fetching checkpoint summary and signature via get_checkpoint API...");
        let checkpoint_response = ledger_client
            .get_checkpoint(
                GetCheckpointRequest::by_sequence_number(end_of_epoch_checkpoint_seq)
                    .with_read_mask(FieldMask::from_paths(["summary", "signature"]))
            )
            .await
            .map_err(|e| format!("Failed to get checkpoint {}: {}", end_of_epoch_checkpoint_seq, e))?
            .into_inner();

        let proto_checkpoint = checkpoint_response
            .checkpoint
            .ok_or("Missing checkpoint in response")?;

        let checkpoint = proto_checkpoint_to_certified_checkpoint(&proto_checkpoint)?;

        tracing::info!("    Verifying checkpoint signatures with epoch {} committee...", epoch);
        checkpoint
            .verify_with_contents(&current_committee, None)
            .map_err(|e| format!("Failed to verify checkpoint {}: {}", end_of_epoch_checkpoint_seq, e))?;

        tracing::info!("    Signature verification PASSED");

        tracing::info!("    Extracting epoch {} committee from verified checkpoint...", epoch + 1);
        let next_committee = extract_new_committee_info(&checkpoint)
            .map_err(|e| format!("Failed to extract committee from checkpoint {}: {}", end_of_epoch_checkpoint_seq, e))?;

        tracing::info!("    New committee extracted:");
        tracing::info!("      Epoch: {}", next_committee.epoch);
        tracing::info!("      Validators: {}", next_committee.voting_rights.len());
        tracing::info!("      Total stake: {}", next_committee.total_votes());

        current_committee = next_committee;

        tracing::info!("    Trust ratchet COMPLETE for epoch {} -> {}", epoch, epoch + 1);
        tracing::info!("");
    }

    tracing::info!("Trust ratcheting complete! Final committee:");
    tracing::info!("  Epoch: {}", current_committee.epoch);
    tracing::info!("  Validators: {}", current_committee.voting_rights.len());
    tracing::info!("========================================");
    tracing::info!("");

    Ok(current_committee)
}

async fn verify_ocs_inclusion_proof_with_committee(
    test_cluster: &test_cluster::TestCluster,
    grpc_proof: &sui_rpc_api::grpc::alpha::event_service_proto::OcsInclusionProof,
    checkpoint_seq: u64,
) -> Result<(), String> {
    let state = test_cluster.fullnode_handle.sui_node.state();

    let checkpoint = state
        .get_checkpoint_by_sequence_number(checkpoint_seq)
        .map_err(|e| format!("Failed to get checkpoint {}: {}", checkpoint_seq, e))?
        .ok_or_else(|| format!("Checkpoint {} not found", checkpoint_seq))?;

    let object_ref_proto = grpc_proof
        .object_ref
        .as_ref()
        .ok_or("Missing object_ref in proof")?;

    let object_ref = proto_object_ref_to_sui_object_ref(object_ref_proto)?;
    let (object_id, version, _digest) = object_ref;
    let ocs_inclusion_proof = proto_ocs_inclusion_proof_to_light_client_proof(grpc_proof)?;

    let target = OCSTarget::new_inclusion_target(object_ref);

    let proof = Proof {
        targets: ProofTarget::ObjectCheckpointState(target),
        checkpoint_summary: checkpoint.into(),
        proof_contents: ProofContents::ObjectCheckpointStateProof(OCSProof::Inclusion(
            ocs_inclusion_proof,
        )),
    };

    tracing::info!("========================================");
    tracing::info!("INCLUSION PROOF VERIFICATION");
    tracing::info!("========================================");
    tracing::info!("Verifying EventStreamHead inclusion proof:");
    tracing::info!("  Checkpoint: {}", checkpoint_seq);
    tracing::info!("  Object ID: {}", object_id);
    tracing::info!("  Version: {}", version);
    tracing::info!("");

    let committee = get_committee_for_checkpoint(test_cluster, checkpoint_seq).await?;

    tracing::info!("Verifying inclusion proof with trust-ratcheted committee...");
    proof
        .verify(&committee)
        .map_err(|e| format!("Proof verification failed: {:?}", e))?;

    tracing::info!("Inclusion proof verification PASSED");
    tracing::info!("EventStreamHead authenticity cryptographically verified!");
    tracing::info!("========================================");
    tracing::info!("");

    Ok(())
}

#[sim_test]
async fn list_authenticated_events_end_to_end() {
    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_authenticated_event_streams_for_testing();
            cfg
        });

    let rpc_config = create_rpc_config_with_authenticated_events();

    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(rpc_config)
        .with_epoch_duration_ms(5000)
        .build()
        .await;
    let rgp = test_cluster.get_reference_gas_price().await;

    let package_id = publish_test_package(&test_cluster).await;

    tracing::info!("========================================");
    tracing::info!("AUTHENTICATED EVENTS E2E TEST");
    tracing::info!("========================================");
    tracing::info!("Published event package: {}", package_id);
    tracing::info!("");

    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    tracing::info!("Step 1: Emitting first event in epoch 0...");
    // Emit first event in epoch 0
    for i in 0..1 {
        let emit_value = 100 + i;
        let mut ptb_i = ProgrammableTransactionBuilder::new();
        let val_i = ptb_i.pure(emit_value as u64).unwrap();
        ptb_i.programmable_move_call(
            package_id,
            move_core_types::identifier::Identifier::new("events").unwrap(),
            move_core_types::identifier::Identifier::new("emit").unwrap(),
            vec![],
            vec![val_i],
        );
        let gas_object = test_cluster
            .wallet
            .get_one_gas_object_owned_by_address(sender)
            .await
            .unwrap()
            .unwrap();
        let tx_data_i = TransactionData::new(
            sui_types::transaction::TransactionKind::ProgrammableTransaction(ptb_i.finish()),
            sender,
            gas_object,
            50_000_000_000,
            rgp,
        );
        test_cluster.sign_and_execute_transaction(&tx_data_i).await;
    }
    tracing::info!("First event emitted successfully");
    tracing::info!("");

    tracing::info!("Step 2: Waiting for epoch change to demonstrate trust ratcheting...");
    test_cluster.wait_for_epoch(None).await;
    tracing::info!("Epoch changed to epoch 1");
    tracing::info!("");

    tracing::info!("Step 3: Emitting remaining 9 events in epoch 1...");
    // Emit remaining 9 events in epoch 1
    for i in 1..10 {
        let emit_value = 100 + i;
        let mut ptb_i = ProgrammableTransactionBuilder::new();
        let val_i = ptb_i.pure(emit_value as u64).unwrap();
        ptb_i.programmable_move_call(
            package_id,
            move_core_types::identifier::Identifier::new("events").unwrap(),
            move_core_types::identifier::Identifier::new("emit").unwrap(),
            vec![],
            vec![val_i],
        );
        let gas_object = test_cluster
            .wallet
            .get_one_gas_object_owned_by_address(sender)
            .await
            .unwrap()
            .unwrap();
        let tx_data_i = TransactionData::new(
            sui_types::transaction::TransactionKind::ProgrammableTransaction(ptb_i.finish()),
            sender,
            gas_object,
            50_000_000_000,
            rgp,
        );
        test_cluster.sign_and_execute_transaction(&tx_data_i).await;
    }
    tracing::info!("All 9 events emitted successfully in epoch 1");
    tracing::info!("");

    tracing::info!("Step 4: Querying authenticated events via ListAuthenticatedEvents API...");
    let mut client = EventServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut req = ListAuthenticatedEventsRequest::default();
    req.stream_id = Some(package_id.to_string());
    req.start_checkpoint = Some(0);
    req.page_size = None;
    req.page_token = None;
    let response = client
        .list_authenticated_events(req)
        .await
        .unwrap()
        .into_inner();

    let count = response.events.len();
    tracing::info!("Received {} authenticated events", count);
    assert_eq!(count, 10, "expected 10 authenticated events, got {count}");

    let found = response.events.iter().any(|event| match &event.event {
        Some(Event {
            contents: Some(bcs),
            ..
        }) => bcs.value.as_ref().is_some_and(|v| !v.is_empty()),
        _ => false,
    });
    assert!(found, "expected authenticated event for the stream");

    let first_event_checkpoint = response.events[0].checkpoint.unwrap();
    let last_event_checkpoint = response.events.last().unwrap().checkpoint.unwrap();
    tracing::info!("");

    tracing::info!("Step 5: Getting EventStreamHead inclusion proofs and verifying with committee...");
    tracing::info!(
        "First event checkpoint: {}, Last event checkpoint: {}",
        first_event_checkpoint,
        last_event_checkpoint
    );
    tracing::info!("");

    let stream_id = sui_types::base_types::SuiAddress::from(package_id);
    let event_stream_head_id = get_event_stream_head_object_id(stream_id).unwrap();

    tracing::info!("Step 5a: Requesting inclusion proof for first EventStreamHead...");
    tracing::info!("  EventStreamHead object ID: {}", event_stream_head_id);
    tracing::info!("  Checkpoint: {}", first_event_checkpoint);

    let mut proof_req = GetObjectInclusionProofRequest::default();
    proof_req.object_id = Some(event_stream_head_id.to_string());
    proof_req.checkpoint = Some(first_event_checkpoint);

    let proof_response = client
        .get_object_inclusion_proof(proof_req)
        .await
        .unwrap()
        .into_inner();

    let inclusion_proof = proof_response
        .inclusion_proof
        .expect("inclusion_proof should be present");

    tracing::info!("Received inclusion proof for first EventStreamHead");
    tracing::info!("");

    assert!(
        inclusion_proof.merkle_proof.is_some(),
        "merkle_proof should be present"
    );
    assert!(
        inclusion_proof.tree_root.is_some(),
        "tree_root should be present"
    );
    assert!(
        inclusion_proof.object_ref.is_some(),
        "object_ref should be present"
    );

    let object_ref = inclusion_proof.object_ref.as_ref().unwrap();
    assert!(
        object_ref.object_id.is_some(),
        "object_id should be present in object_ref"
    );
    assert!(
        object_ref.version.is_some(),
        "version should be present in object_ref"
    );
    assert!(
        object_ref.digest.is_some(),
        "digest should be present in object_ref"
    );

    let object_data_bytes = proof_response
        .object_data
        .expect("object_data should be present");

    let object: Object = bcs::from_bytes(&object_data_bytes)
        .expect("should deserialize object from BCS");

    let move_obj = object.data.try_as_move().expect("should be move object");
    let stream_head: Field<ar::AccumulatorKey, ar::EventStreamHead> = move_obj
        .to_rust()
        .expect("should deserialize to EventStreamHead");

    tracing::info!("Step 5b: Verifying first EventStreamHead with trust-ratcheted committee...");
    tracing::info!(
        "  EventStreamHead - checkpoint: {}, num_events: {}, mmr_len: {}",
        stream_head.value.checkpoint_seq,
        stream_head.value.num_events,
        stream_head.value.mmr.len()
    );

    assert_eq!(
        stream_head.value.checkpoint_seq, first_event_checkpoint,
        "EventStreamHead checkpoint_seq should match requested checkpoint"
    );
    assert!(!stream_head.value.mmr.is_empty(), "MMR should not be empty");
    verify_ocs_inclusion_proof_with_committee(&test_cluster, &inclusion_proof, first_event_checkpoint)
        .await
        .expect("First EventStreamHead inclusion proof should verify with committee");
    tracing::info!("");

    tracing::info!("Step 5c: Requesting inclusion proof for last EventStreamHead...");
    tracing::info!("  EventStreamHead object ID: {}", event_stream_head_id);
    tracing::info!("  Checkpoint: {}", last_event_checkpoint);

    let mut last_proof_req = GetObjectInclusionProofRequest::default();
    last_proof_req.object_id = Some(event_stream_head_id.to_string());
    last_proof_req.checkpoint = Some(last_event_checkpoint);

    let last_proof_response = client
        .get_object_inclusion_proof(last_proof_req)
        .await
        .unwrap()
        .into_inner();

    let last_inclusion_proof = last_proof_response
        .inclusion_proof
        .expect("inclusion_proof should be present");

    tracing::info!("Received inclusion proof for last EventStreamHead");
    tracing::info!("");

    assert!(
        last_inclusion_proof.merkle_proof.is_some(),
        "merkle_proof should be present"
    );
    assert!(
        last_inclusion_proof.tree_root.is_some(),
        "tree_root should be present"
    );
    assert!(
        last_inclusion_proof.object_ref.is_some(),
        "object_ref should be present"
    );

    let last_object_data_bytes = last_proof_response
        .object_data
        .expect("object_data should be present");

    let last_sui_object: Object = bcs::from_bytes(&last_object_data_bytes)
        .expect("should deserialize object from BCS");

    let last_move_obj = last_sui_object
        .data
        .try_as_move()
        .expect("should be move object");
    let last_stream_head: Field<ar::AccumulatorKey, ar::EventStreamHead> = last_move_obj
        .to_rust()
        .expect("should deserialize to EventStreamHead");

    tracing::info!("Step 5d: Verifying last EventStreamHead with trust-ratcheted committee...");
    tracing::info!(
        "  EventStreamHead - checkpoint: {}, num_events: {}, mmr_len: {}",
        last_stream_head.value.checkpoint_seq,
        last_stream_head.value.num_events,
        last_stream_head.value.mmr.len()
    );

    assert_eq!(
        last_stream_head.value.num_events, 10,
        "expected 10 events in final stream head"
    );
    assert_eq!(
        last_stream_head.value.checkpoint_seq, last_event_checkpoint,
        "EventStreamHead checkpoint_seq should match requested checkpoint"
    );
    verify_ocs_inclusion_proof_with_committee(&test_cluster, &last_inclusion_proof, last_event_checkpoint)
        .await
        .expect("Last EventStreamHead inclusion proof should verify with committee");
    tracing::info!("");

    tracing::info!("Step 6: Validating MMR computation from first to last checkpoint...");
    tracing::info!("  Starting from first EventStreamHead state:");
    tracing::info!("    Checkpoint: {}", stream_head.value.checkpoint_seq);
    tracing::info!("    Events: {}", stream_head.value.num_events);
    tracing::info!("    MMR length: {}", stream_head.value.mmr.len());
    tracing::info!("");

    tracing::info!("  Converting {} events to commitments...", response.events.len());
    let mut event_commitments = Vec::new();
    for event in &response.events {
        let commitment = convert_grpc_event_to_commitment(event)
            .expect("should convert event to commitment");
        event_commitments.push(commitment);
    }

    tracing::info!("  Grouping events by checkpoint...");
    let mut events_by_checkpoint: BTreeMap<u64, Vec<EventCommitment>> = BTreeMap::new();
    for commitment in event_commitments {
        events_by_checkpoint
            .entry(commitment.checkpoint_seq)
            .or_insert_with(Vec::new)
            .push(commitment);
    }

    let checkpoints_with_events: Vec<Vec<EventCommitment>> = events_by_checkpoint
        .iter()
        .filter(|(cp, _)| **cp > first_event_checkpoint)
        .map(|(cp, events)| {
            tracing::info!("    Checkpoint {}: {} events", cp, events.len());
            events.clone()
        })
        .collect();
    tracing::info!("");

    tracing::info!("  Applying {} checkpoint updates to MMR...", checkpoints_with_events.len());
    let calculated_stream_head = apply_stream_updates(&stream_head.value, checkpoints_with_events);
    tracing::info!("");

    tracing::info!("  Calculated EventStreamHead:");
    tracing::info!("    Checkpoint: {}", calculated_stream_head.checkpoint_seq);
    tracing::info!("    Events: {}", calculated_stream_head.num_events);
    tracing::info!("    MMR length: {}", calculated_stream_head.mmr.len());
    tracing::info!("");

    tracing::info!("  Actual EventStreamHead from chain:");
    tracing::info!("    Checkpoint: {}", last_stream_head.value.checkpoint_seq);
    tracing::info!("    Events: {}", last_stream_head.value.num_events);
    tracing::info!("    MMR length: {}", last_stream_head.value.mmr.len());
    tracing::info!("");

    tracing::info!("  Comparing calculated vs actual...");
    assert_eq!(
        calculated_stream_head.num_events, last_stream_head.value.num_events,
        "Calculated event count should match actual event count"
    );
    tracing::info!("    ✓ Event count matches: {}", calculated_stream_head.num_events);

    assert_eq!(
        calculated_stream_head.mmr, last_stream_head.value.mmr,
        "Calculated MMR should match actual MMR from EventStreamHead"
    );
    tracing::info!("    ✓ MMR matches!");
    tracing::info!("");

    tracing::info!("MMR validation successful!");
    tracing::info!("");
    tracing::info!("========================================");
    tracing::info!("ALL TESTS PASSED!");
    tracing::info!("========================================");
}

#[sim_test]
async fn list_authenticated_events_page_size_validation() {
    let rpc_config = create_rpc_config_with_authenticated_events();

    let test_cluster = test_cluster::TestClusterBuilder::new()
        .with_rpc_config(rpc_config)
        .build()
        .await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    let response =
        query_authenticated_events(test_cluster.rpc_url(), &sender.to_string(), 0, Some(1500))
            .await
            .unwrap();

    assert!(response.events.is_empty());
}

#[sim_test]
async fn list_authenticated_events_start_beyond_highest() {
    let rpc_config = create_rpc_config_with_authenticated_events();

    let test_cluster = test_cluster::TestClusterBuilder::new()
        .with_rpc_config(rpc_config)
        .build()
        .await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    let probe_response =
        query_authenticated_events(test_cluster.rpc_url(), &sender.to_string(), 0, Some(1))
            .await
            .unwrap();
    let highest = probe_response.highest_indexed_checkpoint.unwrap_or(0);

    let response = query_authenticated_events(
        test_cluster.rpc_url(),
        &sender.to_string(),
        highest + 1000,
        Some(10),
    )
    .await
    .unwrap();

    assert!(response.events.is_empty());
}

#[sim_test]
async fn list_authenticated_events_pruned_checkpoint_error() {
    let rpc_config = create_rpc_config_with_authenticated_events();

    let test_cluster = test_cluster::TestClusterBuilder::new()
        .with_rpc_config(rpc_config)
        .build()
        .await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    let response =
        query_authenticated_events(test_cluster.rpc_url(), &sender.to_string(), 0, Some(10))
            .await
            .unwrap();

    assert!(response.events.is_empty());
}

#[sim_test]
async fn authenticated_events_disabled_test() {
    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_authenticated_event_streams_for_testing();
            cfg
        });

    let test_cluster = test_cluster::TestClusterBuilder::new().build().await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    let result =
        query_authenticated_events(test_cluster.rpc_url(), &sender.to_string(), 0, Some(10)).await;

    assert!(
        result.is_err(),
        "Expected error when authenticated events indexing is disabled"
    );

    let error = result.unwrap_err();
    assert_eq!(error.code(), tonic::Code::Unimplemented);
    assert!(error
        .message()
        .contains("Authenticated events indexing is disabled"));
}

#[sim_test]
async fn authenticated_events_backfill_test() {
    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_authenticated_event_streams_for_testing();
            cfg
        });

    let rpc_config_without_indexing = sui_config::RpcConfig {
        authenticated_events_indexing: Some(false),
        enable_indexing: Some(false),
        ..Default::default()
    };

    let mut test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(rpc_config_without_indexing)
        .build()
        .await;

    let package_id = publish_test_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    for i in 0..5 {
        emit_test_event(&test_cluster, package_id, sender, 200 + i).await;
    }

    let rpc_url_with_indexing = {
        let mut new_fullnode_config = test_cluster
            .fullnode_config_builder()
            .build(&mut rand::rngs::OsRng, test_cluster.swarm.config());

        if let Some(ref mut rpc_config) = new_fullnode_config.rpc {
            rpc_config.enable_indexing = Some(true);
            rpc_config.authenticated_events_indexing = Some(true);
        }

        let new_fullnode_handle = test_cluster
            .start_fullnode_from_config(new_fullnode_config)
            .await;

        new_fullnode_handle.rpc_url.clone()
    };

    let start = tokio::time::Instant::now();
    let response = loop {
        let response =
            query_authenticated_events(&rpc_url_with_indexing, &package_id.to_string(), 0, None)
                .await
                .unwrap();

        if response.events.len() == 5 {
            break response;
        }

        if start.elapsed() > tokio::time::Duration::from_secs(30) {
            panic!(
                "Timeout waiting for backfill to complete. Found {} events, expected 5",
                response.events.len()
            );
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    };

    let count = response.events.len();
    assert_eq!(
        count, 5,
        "expected 5 authenticated events after backfill, got {count}"
    );
}
