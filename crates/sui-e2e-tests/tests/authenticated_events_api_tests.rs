// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::language_storage::StructTag;
use std::collections::BTreeMap;
use std::str::FromStr;
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
    AuthenticatedEvent, ListAuthenticatedEventsRequest,
};
use sui_rpc_api::grpc::alpha::proof_service_proto::proof_service_client::ProofServiceClient;
use sui_rpc_api::grpc::alpha::proof_service_proto::GetObjectInclusionProofRequest;
use sui_sdk_types::ValidatorCommittee;
use sui_types::accumulator_root as ar;
use sui_types::accumulator_root::EventCommitment;
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::committee::Committee;
use sui_types::digests::{Digest, ObjectDigest};
use sui_types::dynamic_field::{DynamicFieldKey, Field};
use sui_types::object::Object;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::TransactionData;
use sui_types::{MoveTypeTagTraitGeneric, SUI_ACCUMULATOR_ROOT_OBJECT_ID, SUI_FRAMEWORK_ADDRESS};
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

fn proto_object_ref_to_sui_object_ref(
    object_ref_proto: &sui_rpc::proto::sui::rpc::v2::ObjectReference,
) -> Result<(ObjectID, SequenceNumber, ObjectDigest), String> {
    let object_id_str = object_ref_proto
        .object_id
        .as_ref()
        .ok_or("Missing object_id")?;
    let object_id =
        ObjectID::from_str(object_id_str).map_err(|e| format!("Invalid object_id: {}", e))?;

    let version = SequenceNumber::from_u64(object_ref_proto.version.ok_or("Missing version")?);

    let digest_str = object_ref_proto.digest.as_ref().ok_or("Missing digest")?;
    let digest =
        ObjectDigest::from_str(digest_str).map_err(|e| format!("Invalid digest: {}", e))?;

    Ok((object_id, version, digest))
}

fn proto_bytes_to_digest(bytes: &[u8]) -> Result<Digest, String> {
    let digest: [u8; 32] = bytes
        .try_into()
        .map_err(|_| format!("Invalid digest length: expected 32, got {}", bytes.len()))?;
    Ok(Digest::new(digest))
}

fn proto_ocs_inclusion_proof_to_light_client_proof(
    grpc_proof: &sui_rpc_api::grpc::alpha::proof_service_proto::OcsInclusionProof,
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
    let transaction_idx = auth_event
        .transaction_idx
        .ok_or("Missing transaction_idx")? as u64;
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
    let type_tag: move_core_types::language_storage::StructTag = event_type
        .parse()
        .map_err(|e| format!("Failed to parse event_type: {}", e))?;

    let sui_event = sui_types::event::Event {
        package_id,
        transaction_module: module,
        sender,
        type_: type_tag,
        contents: bcs_bytes.to_vec(),
    };

    let digest = sui_event.digest();

    Ok(EventCommitment::new(
        checkpoint,
        transaction_idx,
        event_idx,
        digest,
    ))
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

    let sdk_committee = ValidatorCommittee::try_from(&proto_committee).map_err(|e| {
        format!(
            "Failed to convert proto committee to SDK committee: {:?}",
            e
        )
    })?;

    Ok(sui_types::committee::Committee::from(sdk_committee))
}

async fn get_last_checkpoint_of_epoch(
    ledger_client: &mut LedgerServiceClient<tonic::transport::Channel>,
    epoch: u64,
) -> Result<u64, String> {
    let next_epoch_response = ledger_client
        .get_epoch(
            GetEpochRequest::new(epoch + 1)
                .with_read_mask(FieldMask::from_paths(["first_checkpoint"])),
        )
        .await
        .map_err(|e| format!("Failed to get epoch {} from API: {}", epoch + 1, e))?
        .into_inner();

    let next_epoch = next_epoch_response
        .epoch
        .ok_or_else(|| format!("Missing epoch {} in response", epoch + 1))?;

    let first_checkpoint = next_epoch
        .first_checkpoint
        .ok_or_else(|| format!("Missing first_checkpoint for epoch {}", epoch + 1))?;

    Ok(first_checkpoint - 1)
}

async fn get_genesis_committee(test_cluster: &TestCluster) -> Result<Committee, String> {
    let mut ledger_client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .map_err(|e| format!("Failed to connect to ledger service: {}", e))?;

    get_committee_for_epoch_via_api(&mut ledger_client, 0).await
}

struct EpochCache {
    committees: Vec<(u64, u64, Committee)>, // (start_checkpoint, end_checkpoint, committee)
}

impl EpochCache {
    fn get_committee_for_checkpoint(&self, checkpoint_seq: u64) -> Result<&Committee, String> {
        self.committees
            .iter()
            .find(|(start, end, _)| checkpoint_seq >= *start && checkpoint_seq <= *end)
            .map(|(_, _, committee)| committee)
            .ok_or_else(|| {
                format!(
                    "No committee found for checkpoint {}. Available ranges: {:?}",
                    checkpoint_seq,
                    self.committees
                        .iter()
                        .map(|(start, end, _)| (start, end))
                        .collect::<Vec<_>>()
                )
            })
    }
}

async fn build_epoch_cache(
    ledger_client: &mut LedgerServiceClient<tonic::transport::Channel>,
    genesis_committee: Committee,
    current_epoch: u64,
) -> Result<EpochCache, String> {
    let mut committees = Vec::new();
    let mut current_committee = genesis_committee;
    let mut prev_epoch_end_checkpoint = 0u64;

    for epoch in 0..current_epoch {
        let end_of_epoch_checkpoint_seq = get_last_checkpoint_of_epoch(ledger_client, epoch)
            .await
            .map_err(|e| format!("Failed to get last checkpoint of epoch {}: {}", epoch, e))?;

        committees.push((
            prev_epoch_end_checkpoint,
            end_of_epoch_checkpoint_seq,
            current_committee.clone(),
        ));

        let checkpoint_response = ledger_client
            .get_checkpoint(
                GetCheckpointRequest::by_sequence_number(end_of_epoch_checkpoint_seq)
                    .with_read_mask(FieldMask::from_paths(["summary", "signature", "contents"])),
            )
            .await
            .map_err(|e| {
                format!(
                    "Failed to fetch checkpoint {}: {}",
                    end_of_epoch_checkpoint_seq, e
                )
            })?
            .into_inner();

        let proto_checkpoint = checkpoint_response
            .checkpoint
            .ok_or("Missing checkpoint in response")?;

        let checkpoint: sui_types::full_checkpoint_content::Checkpoint = (&proto_checkpoint)
            .try_into()
            .map_err(|e| format!("Failed to convert checkpoint: {:?}", e))?;

        checkpoint
            .summary
            .verify_with_contents(&current_committee, None)
            .map_err(|e| {
                format!(
                    "Failed to verify checkpoint {}: {}",
                    end_of_epoch_checkpoint_seq, e
                )
            })?;

        let next_committee = extract_new_committee_info(&checkpoint.summary).map_err(|e| {
            format!(
                "Failed to extract committee from checkpoint {}: {}",
                end_of_epoch_checkpoint_seq, e
            )
        })?;

        current_committee = next_committee;
        prev_epoch_end_checkpoint = end_of_epoch_checkpoint_seq + 1;
    }

    committees.push((prev_epoch_end_checkpoint, u64::MAX, current_committee));

    Ok(EpochCache { committees })
}

async fn verify_ocs_inclusion_proof(
    epoch_cache: &EpochCache,
    checkpoint_summary: &sui_types::messages_checkpoint::CertifiedCheckpointSummary,
    object_ref_proto: &sui_rpc::proto::sui::rpc::v2::ObjectReference,
    grpc_proof: &sui_rpc_api::grpc::alpha::proof_service_proto::OcsInclusionProof,
    checkpoint_seq: u64,
) -> Result<(), String> {
    let object_ref = proto_object_ref_to_sui_object_ref(object_ref_proto)?;
    let ocs_inclusion_proof = proto_ocs_inclusion_proof_to_light_client_proof(grpc_proof)?;

    let target = OCSTarget::new_inclusion_target(object_ref);

    let proof = Proof {
        targets: ProofTarget::ObjectCheckpointState(target),
        checkpoint_summary: checkpoint_summary.clone(),
        proof_contents: ProofContents::ObjectCheckpointStateProof(OCSProof::Inclusion(
            ocs_inclusion_proof,
        )),
    };

    let committee = epoch_cache.get_committee_for_checkpoint(checkpoint_seq)?;

    proof
        .verify(committee)
        .map_err(|e| format!("Proof verification failed: {:?}", e))?;

    Ok(())
}

async fn fetch_and_verify_event_stream_head(
    proof_client: &mut ProofServiceClient<tonic::transport::Channel>,
    ledger_client: &mut LedgerServiceClient<tonic::transport::Channel>,
    epoch_cache: &EpochCache,
    object_id: ObjectID,
    checkpoint: u64,
) -> Field<ar::AccumulatorKey, ar::EventStreamHead> {
    let mut req = GetObjectInclusionProofRequest::default();
    req.object_id = Some(object_id.to_string());
    req.checkpoint = Some(checkpoint);

    let response = proof_client
        .get_object_inclusion_proof(req)
        .await
        .unwrap()
        .into_inner();

    let object_ref = response.object_ref.expect("object_ref should be present");

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

    let inclusion_proof = response
        .inclusion_proof
        .expect("inclusion_proof should be present");

    assert!(
        inclusion_proof.merkle_proof.is_some(),
        "merkle_proof should be present"
    );
    assert!(
        inclusion_proof.tree_root.is_some(),
        "tree_root should be present"
    );

    let object_data_bytes = response.object_data.expect("object_data should be present");

    let object: Object =
        bcs::from_bytes(&object_data_bytes).expect("should deserialize object from BCS");

    let move_obj = object.data.try_as_move().expect("should be move object");
    let stream_head: Field<ar::AccumulatorKey, ar::EventStreamHead> = move_obj
        .to_rust()
        .expect("should deserialize to EventStreamHead");

    assert_eq!(
        stream_head.value.checkpoint_seq, checkpoint,
        "EventStreamHead checkpoint_seq should match requested checkpoint"
    );

    let checkpoint_response = ledger_client
        .get_checkpoint(
            GetCheckpointRequest::by_sequence_number(checkpoint)
                .with_read_mask(FieldMask::from_paths(["summary", "signature", "contents"])),
        )
        .await
        .expect("Failed to fetch checkpoint")
        .into_inner();

    let proto_checkpoint = checkpoint_response
        .checkpoint
        .expect("Missing checkpoint in response");

    let checkpoint_data: sui_types::full_checkpoint_content::Checkpoint = (&proto_checkpoint)
        .try_into()
        .expect("Failed to convert checkpoint");

    verify_ocs_inclusion_proof(
        epoch_cache,
        &checkpoint_data.summary,
        &object_ref,
        &inclusion_proof,
        checkpoint,
    )
    .await
    .expect("EventStreamHead inclusion proof should verify with committee");

    stream_head
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

    let package_id = publish_test_package(&test_cluster).await;

    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    // we want to emit events across epochs to exercise trust ratcheting / inclusion proof committee validation
    emit_test_event(&test_cluster, package_id, sender, 100).await;

    test_cluster.wait_for_epoch(None).await;

    for i in 1..10 {
        emit_test_event(&test_cluster, package_id, sender, 100 + i).await;
    }

    let mut event_client = EventServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut req = ListAuthenticatedEventsRequest::default();
    req.stream_id = Some(package_id.to_string());
    req.start_checkpoint = Some(0);
    req.page_size = None;
    req.page_token = None;
    let response = event_client
        .list_authenticated_events(req)
        .await
        .unwrap()
        .into_inner();

    let count = response.events.len();
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

    let stream_id = sui_types::base_types::SuiAddress::from(package_id);
    let event_stream_head_id = get_event_stream_head_object_id(stream_id).unwrap();

    let mut proof_client = ProofServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut ledger_client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Build committee cache once by doing trust ratcheting
    let current_epoch = test_cluster
        .fullnode_handle
        .sui_node
        .state()
        .epoch_store_for_testing()
        .epoch();
    let genesis_committee = get_genesis_committee(&test_cluster).await.unwrap();
    let epoch_cache = build_epoch_cache(&mut ledger_client, genesis_committee, current_epoch)
        .await
        .expect("Failed to build epoch cache");

    let stream_head = fetch_and_verify_event_stream_head(
        &mut proof_client,
        &mut ledger_client,
        &epoch_cache,
        event_stream_head_id,
        first_event_checkpoint,
    )
    .await;

    assert!(!stream_head.value.mmr.is_empty(), "MMR should not be empty");

    let last_stream_head = fetch_and_verify_event_stream_head(
        &mut proof_client,
        &mut ledger_client,
        &epoch_cache,
        event_stream_head_id,
        last_event_checkpoint,
    )
    .await;

    assert_eq!(
        last_stream_head.value.num_events, 10,
        "expected 10 events in final stream head"
    );

    let events_by_checkpoint: BTreeMap<u64, Vec<EventCommitment>> =
        response
            .events
            .iter()
            .fold(BTreeMap::new(), |mut map, event| {
                let commitment = convert_grpc_event_to_commitment(event)
                    .expect("should convert event to commitment");
                map.entry(commitment.checkpoint_seq)
                    .or_default()
                    .push(commitment);
                map
            });

    let checkpoints_with_events: Vec<Vec<EventCommitment>> = events_by_checkpoint
        .iter()
        .filter(|(cp, _)| **cp > first_event_checkpoint)
        .map(|(_cp, events)| events.clone())
        .collect();

    let calculated_stream_head = apply_stream_updates(&stream_head.value, checkpoints_with_events);

    assert_eq!(
        calculated_stream_head.num_events, last_stream_head.value.num_events,
        "Calculated event count should match actual event count"
    );

    assert_eq!(
        calculated_stream_head.mmr, last_stream_head.value.mmr,
        "Calculated MMR should match actual MMR from EventStreamHead"
    );
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
