// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::language_storage::StructTag;
use std::str::FromStr;
use std::time::Duration;
use sui_keys::keystore::AccountKeystore;
use sui_light_client::authenticated_events::AuthenticatedEvent;
use sui_light_client::authenticated_events::mmr::apply_stream_updates;
use sui_light_client::proof::base::{Proof, ProofContents, ProofTarget, ProofVerifier};
use sui_light_client::proof::committee::extract_new_committee_info;
use sui_light_client::proof::ocs::{OCSProof, OCSTarget};
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2::{GetCheckpointRequest, GetEpochRequest};
use sui_rpc::proto::sui::rpc::v2alpha::ledger_service_client::LedgerServiceClient as V2AlphaLedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2alpha::proof_service_client::ProofServiceClient;
use sui_rpc::proto::sui::rpc::v2alpha::{
    AffectedObjectFilter, EventFilter, EventLiteral, EventStreamHeadFilter, EventTerm,
    GetCheckpointObjectProofRequest, GetCheckpointObjectProofResponse, ListEventsRequest,
    ListTransactionsRequest, QueryEndReason, QueryOptions, TransactionFilter, TransactionLiteral,
    TransactionTerm, get_checkpoint_object_proof_response, list_events_response,
    list_transactions_response,
};
use sui_rpc_api::client::ExecutedTransaction;
use sui_sdk_types::ValidatorCommittee;
use sui_types::accumulator_root as ar;
use sui_types::accumulator_root::EventCommitment;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::committee::Committee;
use sui_types::dynamic_field::{DynamicFieldKey, Field};
use sui_types::object::Object;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::TransactionData;
use sui_types::{MoveTypeTagTraitGeneric, SUI_ACCUMULATOR_ROOT_OBJECT_ID, SUI_FRAMEWORK_ADDRESS};
use test_cluster::{TestCluster, TestClusterBuilder};

/// Test cluster config that enables ledger history indexing, which is what
/// backs the v2alpha ListEvents and ProofService endpoints.
fn create_rpc_config_with_ledger_history() -> sui_config::RpcConfig {
    sui_config::RpcConfig {
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
                .publish_async(path)
                .await
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

async fn emit_multiple_test_events(
    test_cluster: &TestCluster,
    package_id: ObjectID,
    sender: SuiAddress,
    start_value: u64,
    count: u64,
) -> ExecutedTransaction {
    let rgp = test_cluster.get_reference_gas_price().await;
    let mut ptb = ProgrammableTransactionBuilder::new();
    let start = ptb.pure(start_value).unwrap();
    let cnt = ptb.pure(count).unwrap();
    ptb.programmable_move_call(
        package_id,
        move_core_types::identifier::Identifier::new("events").unwrap(),
        move_core_types::identifier::Identifier::new("emit_multiple").unwrap(),
        vec![],
        vec![start, cnt],
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
    test_cluster.sign_and_execute_transaction(&tx_data).await
}

/// Connect to an rpc client with timeout and retry logic.
///
/// gRPC connection establishment can hang indefinitely if the remote peer is unable to complete
/// connection establishment. This helper ensures we always have bounded connection times and
/// can retry on transient failures.
async fn connect_with_retry<T, F, Fut>(connect_fn: F) -> T
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, tonic::transport::Error>>,
{
    const MAX_RETRIES: u32 = 10;
    const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);

    for attempt in 0..MAX_RETRIES {
        match tokio::time::timeout(CONNECT_TIMEOUT, connect_fn()).await {
            Ok(Ok(client)) => return client,
            Ok(Err(e)) if attempt + 1 < MAX_RETRIES => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Ok(Err(e)) => panic!("failed to connect after {MAX_RETRIES} attempts: {e}"),
            Err(_) if attempt + 1 < MAX_RETRIES => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(_) => panic!("connection timed out after {MAX_RETRIES} attempts"),
        }
    }
    unreachable!()
}

fn build_event_stream_head_filter(stream_id: SuiAddress) -> EventFilter {
    let head_filter = EventStreamHeadFilter::default().with_stream_id(stream_id.to_string());
    let literal = EventLiteral::default().with_event_stream_head(head_filter);
    let term = EventTerm::default().with_literals(vec![literal]);
    EventFilter::default().with_terms(vec![term])
}

fn build_affected_object_filter(object_id: ObjectID) -> TransactionFilter {
    let literal = TransactionLiteral::default().with_affected_object(
        AffectedObjectFilter::default().with_object_id(object_id.to_string()),
    );
    let term = TransactionTerm::default().with_literals(vec![literal]);
    TransactionFilter::default().with_terms(vec![term])
}

/// List every settle_events transaction that touched the given
/// `EventStreamHead` object in `[start_checkpoint, end_checkpoint)`. Each
/// settle_events call mutates the stream head, so filtering by
/// `affected_object = event_stream_head_object_id` returns exactly the
/// per-stream settlement boundaries. Returns ascending
/// `(checkpoint, transaction_offset)` pairs.
async fn fetch_settlements_for_range(
    client: &mut V2AlphaLedgerServiceClient<tonic::transport::Channel>,
    stream_head_object_id: ObjectID,
    start_checkpoint: u64,
    end_checkpoint_exclusive: u64,
) -> Result<Vec<(u64, u64)>, tonic::Status> {
    use futures::StreamExt;

    let filter = build_affected_object_filter(stream_head_object_id);
    let read_mask = FieldMask::from_paths(["checkpoint", "transaction_index"]);
    let mut all = Vec::new();
    let mut cursor: Option<Vec<u8>> = None;

    loop {
        let mut options = QueryOptions::default().with_limit(1000);
        if let Some(c) = cursor.clone() {
            options.set_after(c);
        }
        let mut request = ListTransactionsRequest::default()
            .with_read_mask(read_mask.clone())
            .with_start_checkpoint(start_checkpoint)
            .with_filter(filter.clone())
            .with_options(options);
        if end_checkpoint_exclusive > start_checkpoint {
            request = request.with_end_checkpoint(end_checkpoint_exclusive);
        }

        let mut response = client.list_transactions(request).await?.into_inner();
        let mut end_reason: Option<QueryEndReason> = None;
        let mut last_cursor: Option<Vec<u8>> = None;

        while let Some(frame) = response.next().await {
            let frame = frame?;
            match frame.response {
                Some(list_transactions_response::Response::Item(item)) => {
                    if let Some(c) = item.watermark.as_ref().and_then(|w| w.cursor.as_ref()) {
                        last_cursor = Some(c.to_vec());
                    }
                    let checkpoint = item
                        .transaction
                        .as_ref()
                        .and_then(|tx| tx.checkpoint)
                        .ok_or_else(|| {
                            tonic::Status::internal("settlement tx missing checkpoint")
                        })?;
                    let tx_offset = item
                        .transaction
                        .as_ref()
                        .and_then(|tx| tx.transaction_index)
                        .ok_or_else(|| {
                            tonic::Status::internal("settlement tx missing transaction_index")
                        })?;
                    all.push((checkpoint, tx_offset));
                }
                Some(list_transactions_response::Response::Watermark(w)) => {
                    if let Some(c) = w.cursor.as_ref() {
                        last_cursor = Some(c.to_vec());
                    }
                }
                Some(list_transactions_response::Response::End(end)) => {
                    end_reason = Some(end.reason());
                }
                Some(_) | None => {}
            }
        }

        if matches!(
            end_reason,
            Some(QueryEndReason::ItemLimit) | Some(QueryEndReason::ScanLimit),
        ) {
            cursor = last_cursor;
            continue;
        }
        break;
    }

    Ok(all)
}

/// Bucket events into per-settlement MMR-fold batches. Each event maps to
/// the next settlement transaction with `(cp == event.cp, tx_offset >=
/// event.tx_offset)`; events sharing a settlement key form one batch.
/// Events at checkpoints `<= floor_checkpoint` are dropped (the caller
/// uses this to skip already-verified initial state).
fn bucket_events_by_settlement(
    events: &[AuthenticatedEvent],
    settlements: &[(u64, u64)],
    floor_checkpoint: u64,
) -> Vec<Vec<EventCommitment>> {
    let mut batches: Vec<Vec<EventCommitment>> = Vec::new();
    let mut current_key: Option<(u64, u64)> = None;
    let mut current_batch: Vec<EventCommitment> = Vec::new();
    let mut settlement_idx: usize = 0;

    for event in events {
        if event.checkpoint <= floor_checkpoint {
            continue;
        }

        while settlement_idx < settlements.len()
            && (settlements[settlement_idx].0 < event.checkpoint
                || (settlements[settlement_idx].0 == event.checkpoint
                    && settlements[settlement_idx].1 < event.transaction_offset))
        {
            settlement_idx += 1;
        }
        assert!(
            settlement_idx < settlements.len() && settlements[settlement_idx].0 == event.checkpoint,
            "no settlement transaction covering event at (cp={}, tx_offset={})",
            event.checkpoint,
            event.transaction_offset,
        );

        let settlement_key = settlements[settlement_idx];
        let commitment = EventCommitment::new(
            event.checkpoint,
            event.transaction_offset,
            event.event_index as u64,
            event.event.digest(),
        );

        match current_key {
            Some(key) if key == settlement_key => current_batch.push(commitment),
            _ => {
                if !current_batch.is_empty() {
                    batches.push(std::mem::take(&mut current_batch));
                }
                current_batch.push(commitment);
                current_key = Some(settlement_key);
            }
        }
    }

    if !current_batch.is_empty() {
        batches.push(current_batch);
    }
    batches
}

/// Drive a single `ListEvents` server-streaming request, accumulating every
/// emitted `EventItem` into `AuthenticatedEvent`s.
async fn fetch_list_events_page(
    client: &mut V2AlphaLedgerServiceClient<tonic::transport::Channel>,
    request: ListEventsRequest,
) -> Result<(Vec<AuthenticatedEvent>, Option<Vec<u8>>), tonic::Status> {
    use futures::StreamExt;

    let mut stream = client.list_events(request).await?.into_inner();
    let mut events = Vec::new();
    let mut last_cursor: Option<Vec<u8>> = None;

    while let Some(frame) = stream.next().await {
        let frame = frame?;
        match frame.response {
            Some(list_events_response::Response::Item(item)) => {
                if let Some(cursor) = item.watermark.as_ref().and_then(|w| w.cursor.as_ref()) {
                    last_cursor = Some(cursor.to_vec());
                }
                let event = AuthenticatedEvent::try_from(item).map_err(|e| {
                    tonic::Status::internal(format!("failed to convert event: {e}"))
                })?;
                events.push(event);
            }
            Some(list_events_response::Response::Watermark(w)) => {
                if let Some(cursor) = w.cursor.as_ref() {
                    last_cursor = Some(cursor.to_vec());
                }
            }
            Some(list_events_response::Response::End(_)) | Some(_) | None => {}
        }
    }

    Ok((events, last_cursor))
}

/// Read mask covering everything the in-tree `AuthenticatedEvent` converter
/// needs from a v2alpha `EventItem`: the event body plus its ledger-position
/// fields (`checkpoint`, `transaction_index`, `event_index`), which the list
/// endpoint only populates when requested.
fn full_event_read_mask() -> FieldMask {
    FieldMask::from_paths([
        "package_id",
        "module",
        "sender",
        "event_type",
        "contents",
        "checkpoint",
        "transaction_index",
        "event_index",
    ])
}

/// Issue one `ListEvents` call for the given stream and return the events
/// it produces. Errors propagate so tests can assert specific status codes.
async fn query_authenticated_events(
    rpc_url: &str,
    stream_id: SuiAddress,
    start_checkpoint: u64,
    page_size: Option<u32>,
) -> Result<Vec<AuthenticatedEvent>, tonic::Status> {
    let mut client =
        connect_with_retry(|| V2AlphaLedgerServiceClient::connect(rpc_url.to_owned())).await;

    let mut options = QueryOptions::default();
    if let Some(size) = page_size {
        options.set_limit(size);
    }
    let request = ListEventsRequest::default()
        .with_read_mask(full_event_read_mask())
        .with_start_checkpoint(start_checkpoint)
        .with_filter(build_event_stream_head_filter(stream_id))
        .with_options(options);

    fetch_list_events_page(&mut client, request)
        .await
        .map(|(events, _)| events)
}

/// Page through `ListEvents` until the server reports no remaining matches,
/// returning the full sequence.
async fn list_authenticated_events(
    rpc_url: &str,
    stream_id: SuiAddress,
    start_checkpoint: u64,
    page_size: Option<u32>,
) -> Vec<AuthenticatedEvent> {
    let mut client =
        connect_with_retry(|| V2AlphaLedgerServiceClient::connect(rpc_url.to_owned())).await;

    let filter = build_event_stream_head_filter(stream_id);
    let mut all_events = Vec::new();
    let mut next_cursor: Option<Vec<u8>> = None;
    let mut next_checkpoint = start_checkpoint;

    loop {
        let mut options = QueryOptions::default();
        if let Some(size) = page_size {
            options.set_limit(size);
        }
        if let Some(cursor) = next_cursor.clone() {
            options.set_after(cursor);
        }
        let request = ListEventsRequest::default()
            .with_read_mask(full_event_read_mask())
            .with_start_checkpoint(next_checkpoint)
            .with_filter(filter.clone())
            .with_options(options);

        let (events, last_cursor) = match fetch_list_events_page(&mut client, request).await {
            Ok((events, last_cursor)) => (events, last_cursor),
            Err(status) if status.code() == tonic::Code::Unavailable => return vec![],
            Err(status) => panic!("{status}"),
        };
        let event_count = events.len();
        let last_checkpoint = events.last().map(|e| e.checkpoint);
        all_events.extend(events);

        match (last_cursor, last_checkpoint) {
            (Some(cursor), Some(_)) if event_count > 0 => {
                next_cursor = Some(cursor);
            }
            _ => break,
        }

        // Cursor-bounded paging: advance the lower checkpoint so the next
        // request scans strictly past the last returned event.
        if let Some(cp) = last_checkpoint {
            next_checkpoint = cp;
        }
    }

    all_events
}

async fn verify_events_with_stream_head(
    test_cluster: &TestCluster,
    package_id: ObjectID,
    events: &[AuthenticatedEvent],
    expected_event_count: u64,
) {
    let stream_id = SuiAddress::from(package_id);
    let event_stream_head_id = get_event_stream_head_object_id(stream_id).unwrap();

    let mut proof_client =
        connect_with_retry(|| ProofServiceClient::connect(test_cluster.rpc_url().to_owned())).await;

    let mut ledger_client =
        connect_with_retry(|| LedgerServiceClient::connect(test_cluster.rpc_url().to_owned()))
            .await;

    let current_epoch = test_cluster
        .fullnode_handle
        .sui_node
        .state()
        .epoch_store_for_testing()
        .epoch();
    let genesis_committee = get_genesis_committee(test_cluster).await.unwrap();
    let epoch_cache = build_epoch_cache(&mut ledger_client, genesis_committee, current_epoch)
        .await
        .expect("Failed to build epoch cache");

    let first_event_checkpoint = events[0].checkpoint;
    let last_event_checkpoint = events.last().unwrap().checkpoint;

    let first_stream_head = fetch_and_verify_event_stream_head(
        &mut proof_client,
        &mut ledger_client,
        &epoch_cache,
        event_stream_head_id,
        first_event_checkpoint,
    )
    .await;

    let last_stream_head = fetch_and_verify_event_stream_head(
        &mut proof_client,
        &mut ledger_client,
        &epoch_cache,
        event_stream_head_id,
        last_event_checkpoint,
    )
    .await;

    assert_eq!(
        last_stream_head.value.num_events, expected_event_count,
        "expected {} events in final stream head",
        expected_event_count
    );

    // Bucket events into per-settlement MMR-fold batches. The framework
    // runs one `settle_events` per consensus commit per stream, so a
    // checkpoint that aggregates multiple commits has multiple folds at
    // the same `checkpoint_seq`. We need the corresponding settlement
    // boundaries to reconstruct each fold separately — fetch them via
    // `ListTransactions` filtered to the stream's `EventStreamHead`.
    let mut ledger_v2alpha = V2AlphaLedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .expect("connect v2alpha ledger");
    let settlements = fetch_settlements_for_range(
        &mut ledger_v2alpha,
        event_stream_head_id,
        first_event_checkpoint,
        last_event_checkpoint.saturating_add(1),
    )
    .await
    .expect("fetch settlement transactions");

    let events_by_settlement =
        bucket_events_by_settlement(events, &settlements, first_event_checkpoint);

    let calculated_stream_head =
        apply_stream_updates(&first_stream_head.value, events_by_settlement);

    assert_eq!(
        calculated_stream_head.num_events, last_stream_head.value.num_events,
        "Calculated event count should match actual event count"
    );

    assert_eq!(
        calculated_stream_head.mmr, last_stream_head.value.mmr,
        "Calculated MMR should match actual MMR from EventStreamHead"
    );
}

fn get_event_stream_head_object_id(stream_id: SuiAddress) -> Result<ObjectID, String> {
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
) -> Result<Committee, String> {
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

    Ok(Committee::from(sdk_committee))
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
    let mut ledger_client =
        connect_with_retry(|| LedgerServiceClient::connect(test_cluster.rpc_url().to_owned()))
            .await;

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

fn proto_node_to_local(
    proto: &sui_rpc::proto::sui::rpc::v2alpha::MerkleNode,
) -> Result<fastcrypto::merkle::Node, String> {
    use sui_rpc::proto::sui::rpc::v2alpha::merkle_node;
    let node = proto.node.as_ref().ok_or("MerkleNode missing oneof")?;
    match node {
        merkle_node::Node::Empty(_) => Ok(fastcrypto::merkle::Node::Empty),
        merkle_node::Node::Digest(bytes) => {
            let arr: [u8; 32] = bytes
                .as_ref()
                .try_into()
                .map_err(|_| format!("Invalid digest length: {}", bytes.len()))?;
            Ok(fastcrypto::merkle::Node::Digest(arr))
        }
        _ => Err("Unknown MerkleNode variant".to_string()),
    }
}

fn proto_inclusion_proof_to_local(
    proto: &sui_rpc::proto::sui::rpc::v2alpha::OcsInclusionProof,
) -> Result<sui_light_client::proof::ocs::OCSInclusionProof, String> {
    let merkle_proof_proto = proto.merkle_proof.as_ref().ok_or("Missing merkle_proof")?;
    let nodes = merkle_proof_proto
        .path
        .iter()
        .map(proto_node_to_local)
        .collect::<Result<Vec<_>, _>>()?;
    let merkle_proof = fastcrypto::merkle::MerkleProof::new(&nodes);
    let leaf_index = proto.leaf_index.ok_or("Missing leaf_index")? as usize;
    let tree_root_bytes = proto.tree_root.as_ref().ok_or("Missing tree_root")?;
    let tree_root_arr: [u8; 32] = tree_root_bytes
        .as_ref()
        .try_into()
        .map_err(|_| format!("Invalid tree_root length: {}", tree_root_bytes.len()))?;
    let tree_root = sui_types::digests::Digest::new(tree_root_arr);

    Ok(sui_light_client::proof::ocs::OCSInclusionProof {
        merkle_proof,
        leaf_index,
        tree_root,
    })
}

fn proto_object_ref_to_sui_object_ref(
    object_ref_proto: &sui_rpc::proto::sui::rpc::v2::ObjectReference,
) -> Result<sui_types::base_types::ObjectRef, String> {
    let object_id_str = object_ref_proto
        .object_id
        .as_ref()
        .ok_or("Missing object_id")?;
    let object_id =
        ObjectID::from_str(object_id_str).map_err(|e| format!("Invalid object_id: {}", e))?;

    let version = sui_types::base_types::SequenceNumber::from_u64(
        object_ref_proto.version.ok_or("Missing version")?,
    );

    let digest_str = object_ref_proto.digest.as_ref().ok_or("Missing digest")?;
    let digest = sui_types::digests::ObjectDigest::from_str(digest_str)
        .map_err(|e| format!("Invalid digest: {}", e))?;

    Ok((object_id, version, digest))
}

fn verify_ocs_inclusion_proof(
    epoch_cache: &EpochCache,
    checkpoint_summary: &sui_types::messages_checkpoint::CertifiedCheckpointSummary,
    response: &GetCheckpointObjectProofResponse,
    checkpoint_seq: u64,
) -> Result<(), String> {
    let proof = response.proof.as_ref().ok_or("Missing proof")?;
    let inclusion_proof = match proof {
        get_checkpoint_object_proof_response::Proof::Inclusion(p) => p,
        _ => return Err("Expected inclusion proof".to_string()),
    };
    let object_ref_proto = inclusion_proof
        .object_ref
        .as_ref()
        .ok_or("Missing object_ref")?;
    let object_ref = proto_object_ref_to_sui_object_ref(object_ref_proto)?;
    let ocs_inclusion_proof = proto_inclusion_proof_to_local(inclusion_proof)?;

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
    let request = GetCheckpointObjectProofRequest::default()
        .with_object_id(object_id.to_string())
        .with_checkpoint(checkpoint);

    let response = proof_client
        .get_checkpoint_object_proof(request)
        .await
        .unwrap()
        .into_inner();

    let proof = response.proof.as_ref().expect("proof should be present");
    let inclusion_proof = match proof {
        get_checkpoint_object_proof_response::Proof::Inclusion(p) => p,
        get_checkpoint_object_proof_response::Proof::NonInclusion(_) => {
            panic!("expected inclusion proof at checkpoint {checkpoint}");
        }
        _ => panic!("unknown proof variant"),
    };

    let object_ref = inclusion_proof
        .object_ref
        .as_ref()
        .expect("object_ref should be present");

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

    assert!(
        inclusion_proof.merkle_proof.is_some(),
        "merkle_proof should be present"
    );
    assert!(
        inclusion_proof.tree_root.is_some(),
        "tree_root should be present"
    );

    let object_data_bytes = inclusion_proof
        .object_data
        .as_ref()
        .expect("object_data should be present");

    let object: Object =
        bcs::from_bytes(object_data_bytes).expect("should deserialize object from BCS");

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

    verify_ocs_inclusion_proof(epoch_cache, &checkpoint_data.summary, &response, checkpoint)
        .expect("OCS inclusion proof verification failed");

    stream_head
}

#[sim_test]
async fn list_authenticated_events_end_to_end() {
    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_authenticated_event_streams_for_testing();
            cfg
        });

    let rpc_config = create_rpc_config_with_ledger_history();

    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(rpc_config)
        .with_epoch_duration_ms(10000)
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

    let stream_id = SuiAddress::from(package_id);
    let all_events = list_authenticated_events(test_cluster.rpc_url(), stream_id, 0, None).await;

    let count = all_events.len();
    assert_eq!(count, 10, "expected 10 authenticated events, got {count}");

    assert!(
        all_events
            .iter()
            .any(|event| !event.event.contents.is_empty()),
        "expected non-empty event contents"
    );

    verify_events_with_stream_head(&test_cluster, package_id, &all_events, 10).await;
}

#[sim_test]
async fn list_authenticated_events_start_beyond_highest() {
    let rpc_config = create_rpc_config_with_ledger_history();

    let test_cluster = TestClusterBuilder::new()
        .with_rpc_config(rpc_config)
        .build()
        .await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    // Probe the ledger so we have some indexed history to bound against,
    // then request a far-future checkpoint. The server should report no
    // matching events without erroring.
    let _ = query_authenticated_events(test_cluster.rpc_url(), sender, 0, Some(1))
        .await
        .unwrap();

    let response =
        query_authenticated_events(test_cluster.rpc_url(), sender, u64::MAX / 2, Some(10))
            .await
            .unwrap();

    assert!(response.is_empty());
}

#[sim_test]
async fn list_authenticated_events_no_events_for_stream() {
    let rpc_config = create_rpc_config_with_ledger_history();

    let test_cluster = TestClusterBuilder::new()
        .with_rpc_config(rpc_config)
        .build()
        .await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    let response = query_authenticated_events(test_cluster.rpc_url(), sender, 0, Some(10))
        .await
        .unwrap();

    assert!(response.is_empty());
}

#[sim_test]
async fn authenticated_events_backfill_test() {
    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_authenticated_event_streams_for_testing();
            cfg
        });

    let rpc_config = sui_config::RpcConfig {
        enable_indexing: Some(true),
        ..Default::default()
    };

    let mut test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(rpc_config)
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
        }

        let new_fullnode_handle = test_cluster
            .start_fullnode_from_config(new_fullnode_config)
            .await;

        new_fullnode_handle.rpc_url.clone()
    };

    let stream_id = SuiAddress::from(package_id);
    let start = tokio::time::Instant::now();
    let events = loop {
        let events = list_authenticated_events(&rpc_url_with_indexing, stream_id, 0, None).await;

        if events.len() == 5 {
            break events;
        }

        if start.elapsed() > tokio::time::Duration::from_secs(30) {
            panic!(
                "Timeout waiting for backfill to complete. Found {} events, expected 5",
                events.len()
            );
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    };

    assert_eq!(
        events.len(),
        5,
        "expected 5 authenticated events after backfill, got {}",
        events.len()
    );
}

#[sim_test]
async fn authenticated_events_multiple_events_per_transaction() {
    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_authenticated_event_streams_for_testing();
            cfg
        });

    let rpc_config = create_rpc_config_with_ledger_history();

    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(rpc_config)
        .build()
        .await;

    let package_id = publish_test_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    let _response = emit_multiple_test_events(&test_cluster, package_id, sender, 100, 3).await;

    let stream_id = SuiAddress::from(package_id);
    let events = list_authenticated_events(test_cluster.rpc_url(), stream_id, 0, None).await;

    let count = events.len();
    assert_eq!(
        count, 3,
        "expected 3 authenticated events (all from one transaction), got {count}"
    );

    #[derive(serde::Deserialize)]
    struct E {
        value: u64,
    }

    let values: Vec<u64> = events
        .iter()
        .filter_map(|event| {
            bcs::from_bytes::<E>(&event.event.contents)
                .ok()
                .map(|e| e.value)
        })
        .collect();

    assert_eq!(values.len(), 3, "should extract 3 event values");
    assert!(values.contains(&100), "should contain event with value 100");
    assert!(values.contains(&101), "should contain event with value 101");
    assert!(values.contains(&102), "should contain event with value 102");

    let tx_offsets: std::collections::HashSet<u64> = events
        .iter()
        .map(|event| event.transaction_offset)
        .collect();

    assert_eq!(
        tx_offsets.len(),
        1,
        "all events should be from the same transaction"
    );
}

#[sim_test]
async fn test_pagination() {
    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_authenticated_event_streams_for_testing();
            cfg
        });

    let rpc_config = create_rpc_config_with_ledger_history();

    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(rpc_config)
        .build()
        .await;

    let package_id = publish_test_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];

    for i in 0..5 {
        emit_multiple_test_events(&test_cluster, package_id, sender, i * 5, 5).await;
    }

    let stream_id = SuiAddress::from(package_id);
    let all_events = list_authenticated_events(test_cluster.rpc_url(), stream_id, 0, Some(7)).await;

    assert_eq!(
        all_events.len(),
        25,
        "expected 25 total events across all pages, got {}",
        all_events.len()
    );

    verify_events_with_stream_head(&test_cluster, package_id, &all_events, 25).await;
}

/// When the requested checkpoint did not modify the EventStreamHead, the
/// server returns a non-inclusion proof rather than an inclusion proof.
#[sim_test]
async fn test_object_inclusion_proof_returns_non_inclusion() {
    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_authenticated_event_streams_for_testing();
            cfg
        });

    let rpc_config = create_rpc_config_with_ledger_history();

    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(rpc_config)
        .build()
        .await;

    let package_id = publish_test_package(&test_cluster).await;
    let stream_id = SuiAddress::from(package_id);
    let event_stream_head_id = get_event_stream_head_object_id(stream_id).unwrap();

    let highest_checkpoint = test_cluster
        .grpc_client()
        .get_latest_checkpoint()
        .await
        .unwrap()
        .sequence_number;

    // The proof service serves from the embedded rpc-store, which indexes the
    // tip asynchronously; wait for it to catch up before requesting a proof at
    // the highest executed checkpoint.
    test_cluster.wait_for_rpc_index_ready().await;

    let mut proof_client =
        connect_with_retry(|| ProofServiceClient::connect(test_cluster.rpc_url().to_owned())).await;

    let request = GetCheckpointObjectProofRequest::default()
        .with_object_id(event_stream_head_id.to_string())
        .with_checkpoint(highest_checkpoint);

    let response = proof_client
        .get_checkpoint_object_proof(request)
        .await
        .expect("non-inclusion proof should succeed")
        .into_inner();

    let proof = response.proof.expect("proof should be present");
    assert!(
        matches!(
            proof,
            get_checkpoint_object_proof_response::Proof::NonInclusion(_)
        ),
        "expected non-inclusion proof at a checkpoint that did not modify the EventStreamHead"
    );
}

#[sim_test]
async fn authenticated_events_multiple_commits_per_checkpoint() {
    use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
    use sui_types::transaction::{Argument, Command};

    let _guard: sui_protocol_config::OverrideGuard =
        ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
            cfg.enable_authenticated_event_streams_for_testing();
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg.set_min_checkpoint_interval_ms_for_testing(1000);
            cfg.disable_randomize_checkpoint_tx_limit_for_testing();
            cfg
        });

    let rpc_config = create_rpc_config_with_ledger_history();

    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(rpc_config)
        .build()
        .await;

    let package_id = publish_test_package(&test_cluster).await;
    let sender = test_cluster.wallet.config.keystore.addresses()[0];
    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();

    let gas_for_deposit = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await
        .unwrap()
        .unwrap();

    let mut deposit_builder = ProgrammableTransactionBuilder::new();
    let deposit_amount = deposit_builder.pure(6_000_000_000_000u64).unwrap();
    let recipient_arg = deposit_builder.pure(sender).unwrap();
    let coin =
        deposit_builder.command(Command::SplitCoins(Argument::GasCoin, vec![deposit_amount]));
    let Argument::Result(coin_idx) = coin else {
        panic!("coin is not a result");
    };
    let coin = Argument::NestedResult(coin_idx, 0);
    deposit_builder.programmable_move_call(
        sui_types::SUI_FRAMEWORK_PACKAGE_ID,
        move_core_types::identifier::Identifier::new("coin").unwrap(),
        move_core_types::identifier::Identifier::new("send_funds").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![coin, recipient_arg],
    );
    let deposit_tx = TransactionData::new(
        sui_types::transaction::TransactionKind::ProgrammableTransaction(deposit_builder.finish()),
        sender,
        gas_for_deposit,
        10_000_000,
        rgp,
    );
    test_cluster.sign_and_execute_transaction(&deposit_tx).await;

    // The 6T deposit becomes spendable from the sender's address balance only after the
    // per-checkpoint settlement transaction creates the account object on each node. Awaiting
    // the deposit's execution above is not sufficient: the soft bundles below are submitted to
    // a randomly chosen validator, and a validator that has not yet applied the settlement
    // would see a zero balance in its pre-consensus funds check and reject the transaction
    // with InvalidWithdrawReservation. Wait for the deposit to settle on every node first.
    test_cluster
        .wait_for_tx_settlement_all_nodes(&[deposit_tx.digest()])
        .await;

    let event_count = 100;

    let tx_data_vec: Vec<_> = (0..event_count)
        .map(|i| {
            let mut ptb = ProgrammableTransactionBuilder::new();
            let val = ptb.pure(i as u64).unwrap();
            ptb.programmable_move_call(
                package_id,
                move_core_types::identifier::Identifier::new("events").unwrap(),
                move_core_types::identifier::Identifier::new("emit").unwrap(),
                vec![],
                vec![val],
            );

            sui_types::transaction::TransactionData::V1(sui_types::transaction::TransactionDataV1 {
                kind: sui_types::transaction::TransactionKind::ProgrammableTransaction(
                    ptb.finish(),
                ),
                sender,
                gas_data: sui_types::transaction::GasData {
                    payment: vec![],
                    owner: sender,
                    price: rgp,
                    budget: 50_000_000_000,
                },
                expiration: sui_types::transaction::TransactionExpiration::ValidDuring {
                    min_epoch: Some(0),
                    max_epoch: Some(0),
                    min_timestamp: None,
                    max_timestamp: None,
                    chain: chain_id,
                    nonce: i,
                },
            })
        })
        .collect();

    let tx_digests: Vec<_> = tx_data_vec.iter().map(|tx| tx.digest()).collect();
    let unique_digests: std::collections::HashSet<_> = tx_digests.iter().collect();
    assert_eq!(
        tx_digests.len(),
        unique_digests.len(),
        "Expected all transaction digests to be unique, but found {} total and {} unique",
        tx_digests.len(),
        unique_digests.len()
    );

    let bundle_tasks: Vec<_> = tx_data_vec
        .chunks(5)
        .map(|bundle| test_cluster.sign_and_execute_txns_in_soft_bundle(bundle))
        .collect();
    futures::future::try_join_all(bundle_tasks).await.unwrap();

    let stream_id = SuiAddress::from(package_id);

    // The ledger history index runs behind execution; poll until every event
    // is indexed (or fail after a timeout).
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(30);
    let all_events = loop {
        let events = list_authenticated_events(test_cluster.rpc_url(), stream_id, 0, None).await;
        if events.len() == event_count as usize {
            break events;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "Timed out waiting for {} indexed events; only saw {}",
                event_count,
                events.len()
            );
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    };

    // With many transactions packed into few checkpoints, at least one
    // checkpoint should carry events from multiple distinct transactions.
    let mut events_by_checkpoint: std::collections::HashMap<u64, std::collections::HashSet<u64>> =
        std::collections::HashMap::new();
    for event in &all_events {
        events_by_checkpoint
            .entry(event.checkpoint)
            .or_default()
            .insert(event.transaction_offset);
    }
    assert!(
        events_by_checkpoint.values().any(|txs| txs.len() > 1),
        "expected at least one checkpoint with events from multiple transactions, got: {:?}",
        events_by_checkpoint
    );

    // Events must be ordered (checkpoint asc, transaction_offset asc,
    // event_index asc) — this is what the v2alpha contract guarantees and
    // what downstream MMR consumers depend on.
    for window in all_events.windows(2) {
        let prev = (
            window[0].checkpoint,
            window[0].transaction_offset,
            window[0].event_index,
        );
        let next = (
            window[1].checkpoint,
            window[1].transaction_offset,
            window[1].event_index,
        );
        assert!(
            prev < next,
            "events must be strictly ordered; got {:?} then {:?}",
            prev,
            next,
        );
    }

    // Replay the on-chain MMR with per-settlement bucketing. Multiple
    // consensus commits in one checkpoint produce multiple `settle_events`
    // calls per stream (each its own MMR fold), so reconstructing the
    // chain head needs settlement boundaries — which
    // `verify_events_with_stream_head` pulls via
    // `ListTransactions(affected_object = event_stream_head)`.
    verify_events_with_stream_head(
        &test_cluster,
        package_id,
        &all_events,
        all_events.len() as u64,
    )
    .await;
}
