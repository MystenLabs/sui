// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::field_reassign_with_default)]

use crate::grpc::alpha::event_service_proto::{
    AuthenticatedEvent, Bcs, Event, EventStreamHead, ListAuthenticatedEventsRequest,
    ListAuthenticatedEventsResponse, Proof,
};
use crate::RpcError;
use crate::RpcService;
use bytes::Bytes;
use move_core_types::language_storage::{StructTag, TypeTag};
use move_core_types::u256::U256;
use prost::Message;
use std::str::FromStr;
use std::sync::Arc;
use sui_types::accumulator_root as ar;
use sui_types::base_types::SuiAddress;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::MoveTypeTagTraitGeneric;

const MAX_PAGE_SIZE: u32 = 1000;
const DEFAULT_PAGE_SIZE: u32 = 1000;
const MAX_PAGE_SIZE_BYTES: usize = 512 * 1024; // 512KiB

#[derive(serde::Serialize, serde::Deserialize)]
struct PageToken {
    stream_id: SuiAddress,
    start_checkpoint: u64,
    last_checkpoint: u64,
}

fn to_grpc_event(ev: &sui_types::event::Event) -> Event {
    let mut bcs = Bcs::default();
    bcs.value = Some(ev.contents.clone());

    let mut event = Event::default();
    event.package_id = Some(ev.package_id.to_canonical_string(true));
    event.module = Some(ev.transaction_module.to_string());
    event.sender = Some(ev.sender.to_string());
    event.event_type = Some(ev.type_.to_canonical_string(true));
    event.contents = Some(bcs);
    event
}

fn to_authenticated_event(
    stream_id: &str,
    cp: u64,
    transaction_idx: u32,
    idx: u32,
    ev: &sui_types::event::Event,
) -> AuthenticatedEvent {
    let mut authenticated_event = AuthenticatedEvent::default();
    authenticated_event.checkpoint = Some(cp);
    authenticated_event.transaction_idx = Some(transaction_idx);
    authenticated_event.event_index = Some(idx);
    authenticated_event.event = Some(to_grpc_event(ev));
    authenticated_event.stream_id = Some(stream_id.to_string());
    authenticated_event
}

pub(crate) fn load_event_stream_head(
    reader: &Arc<dyn sui_types::storage::RpcStateReader>,
    stream_id: &str,
    at_checkpoint: u64,
) -> Option<EventStreamHead> {
    #[derive(serde::Deserialize)]
    struct MoveEventStreamHead {
        mmr: Vec<U256>,
        checkpoint_seq: u64,
        num_events: u64,
    }
    let stream_address = sui_types::base_types::SuiAddress::from_str(stream_id).ok()?;
    let event_stream_head_object_id = {
        let module = ar::ACCUMULATOR_SETTLEMENT_MODULE.to_owned();
        let name = ar::ACCUMULATOR_SETTLEMENT_EVENT_STREAM_HEAD.to_owned();
        let tag = StructTag {
            address: sui_types::SUI_FRAMEWORK_ADDRESS,
            module,
            name,
            type_params: vec![],
        };
        let key_type_tag = ar::AccumulatorKey::get_type_tag(&[TypeTag::Struct(Box::new(tag))]);
        let df_key = sui_types::dynamic_field::DynamicFieldKey(
            sui_types::SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            ar::AccumulatorKey {
                owner: stream_address,
            },
            key_type_tag,
        );
        df_key.into_unbounded_id().ok()?.as_object_id()
    };

    let contents = reader.get_checkpoint_contents_by_sequence_number(at_checkpoint)?;

    let mut version: Option<sui_types::base_types::SequenceNumber> = None;
    for tx_digest in contents.iter().rev() {
        let tx = tx_digest.transaction;
        if let Some(effects) = reader.get_transaction_effects(&tx) {
            for (obj_id, ver, _) in effects.written() {
                if obj_id == event_stream_head_object_id {
                    version = Some(ver);
                    break;
                }
            }
            if version.is_some() {
                break;
            }
        }
    }

    let version = version?;
    let obj = reader.get_object_by_key(&event_stream_head_object_id, version)?;
    let mo = obj.data.try_as_move()?;
    let field = mo.to_rust::<sui_types::dynamic_field::Field<
        sui_types::accumulator_root::AccumulatorKey,
        MoveEventStreamHead,
    >>()?;

    let mut event_stream_head = EventStreamHead::default();
    event_stream_head.mmr = field
        .value
        .mmr
        .into_iter()
        .map(|x| x.to_le_bytes().to_vec())
        .collect();
    event_stream_head.checkpoint_seq = Some(field.value.checkpoint_seq);
    event_stream_head.num_events = Some(field.value.num_events);
    Some(event_stream_head)
}

fn decode_page_token(page_token: &[u8]) -> Result<PageToken, RpcError> {
    bcs::from_bytes(page_token).map_err(|_| {
        RpcError::new(
            tonic::Code::InvalidArgument,
            "invalid page_token".to_string(),
        )
    })
}

fn encode_page_token(page_token: PageToken) -> Bytes {
    bcs::to_bytes(&page_token).unwrap().into()
}

#[tracing::instrument(skip(service))]
pub fn list_authenticated_events(
    service: &RpcService,
    request: ListAuthenticatedEventsRequest,
) -> Result<ListAuthenticatedEventsResponse, RpcError> {
    let stream_id = request.stream_id.ok_or_else(|| {
        RpcError::new(
            tonic::Code::InvalidArgument,
            "missing stream_id".to_string(),
        )
    })?;

    if stream_id.trim().is_empty() {
        return Err(RpcError::new(
            tonic::Code::InvalidArgument,
            "stream_id cannot be empty".to_string(),
        ));
    }

    let stream_addr = SuiAddress::from_str(&stream_id).map_err(|e| {
        RpcError::new(
            tonic::Code::InvalidArgument,
            format!("invalid stream_id: {e}"),
        )
    })?;

    let page_size = request
        .page_size
        .map(|s| s.clamp(1, MAX_PAGE_SIZE))
        .unwrap_or(DEFAULT_PAGE_SIZE);

    let page_token = request
        .page_token
        .as_ref()
        .map(|token| decode_page_token(token))
        .transpose()?;

    if let Some(token) = &page_token {
        if token.stream_id != stream_addr {
            return Err(RpcError::new(
                tonic::Code::InvalidArgument,
                "page_token stream_id mismatch".to_string(),
            ));
        }
    }

    let start = request.start_checkpoint.unwrap_or(0);

    let reader = service.reader.inner();
    let indexes = reader.indexes().ok_or_else(RpcError::not_found)?;

    let highest_indexed = indexes
        .get_highest_indexed_checkpoint_seq_number()
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        .unwrap_or(0);

    let lowest_available = reader
        .get_lowest_available_checkpoint_objects()
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

    if start < lowest_available {
        return Err(RpcError::new(
            tonic::Code::InvalidArgument,
            format!(
                "Requested start checkpoint {} has been pruned. Lowest available checkpoint is {}",
                start, lowest_available
            ),
        ));
    }

    if start > highest_indexed {
        let mut response = ListAuthenticatedEventsResponse::default();
        response.events = vec![];
        response.last_checkpoint = Some(highest_indexed);
        response.next_page_token = None;
        return Ok(response);
    }

    let start_cp = if let Some(token) = &page_token {
        token.last_checkpoint + 1
    } else {
        start
    };

    let end_cp = (start_cp + page_size as u64 - 1).min(highest_indexed);

    let iter = indexes
        .authenticated_event_iter(stream_addr, start_cp, end_cp)
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

    let mut events = Vec::new();
    let mut size_bytes = 0;
    let mut checkpoints_processed: u32 = 0;
    let mut last_checkpoint_seen: Option<u64> = None;
    let mut current_checkpoint_events = Vec::new();
    let mut current_checkpoint: Option<u64> = None;

    for event_result in iter {
        let (cp, transaction_idx, event_idx, ev) =
            event_result.map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

        if current_checkpoint.is_some() && current_checkpoint != Some(cp) {
            events.append(&mut current_checkpoint_events);
            checkpoints_processed += 1;

            if checkpoints_processed >= page_size || size_bytes >= MAX_PAGE_SIZE_BYTES {
                last_checkpoint_seen = current_checkpoint;
                break;
            }
        }

        current_checkpoint = Some(cp);
        let authenticated_event =
            to_authenticated_event(&stream_id, cp, transaction_idx, event_idx, &ev);
        size_bytes += authenticated_event.encoded_len();
        current_checkpoint_events.push(authenticated_event);
        last_checkpoint_seen = Some(cp);
    }

    if !current_checkpoint_events.is_empty()
        && checkpoints_processed < page_size
        && size_bytes < MAX_PAGE_SIZE_BYTES
    {
        events.extend(current_checkpoint_events);
        checkpoints_processed += 1;
        last_checkpoint_seen = current_checkpoint;
    }

    let has_next_page = checkpoints_processed == page_size && end_cp < highest_indexed;

    let next_page_token = if has_next_page {
        last_checkpoint_seen.map(|last_cp| {
            encode_page_token(PageToken {
                stream_id: stream_addr,
                start_checkpoint: start,
                last_checkpoint: last_cp,
            })
        })
    } else {
        None
    };

    let last_checkpoint_with_events = events.last().and_then(|e| e.checkpoint);
    let event_stream_head = last_checkpoint_with_events
        .and_then(|last_checkpoint| load_event_stream_head(reader, &stream_id, last_checkpoint));

    let mut response = ListAuthenticatedEventsResponse::default();
    response.events = events;
    response.proof = event_stream_head.map(|esh| {
        let mut proof = Proof::default();
        proof.event_stream_head = Some(esh);
        proof
    });
    response.last_checkpoint = Some(highest_indexed);
    response.next_page_token = next_page_token.map(|token| token.to_vec());
    Ok(response)
}
