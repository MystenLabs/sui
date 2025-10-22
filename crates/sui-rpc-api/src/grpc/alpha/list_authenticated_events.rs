// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::field_reassign_with_default)]

use crate::RpcError;
use crate::RpcService;
use crate::grpc::alpha::event_service_proto::{
    AuthenticatedEvent, ListAuthenticatedEventsRequest, ListAuthenticatedEventsResponse,
};
use bytes::Bytes;
use prost::Message;
use std::str::FromStr;
use sui_rpc::proto::sui::rpc::v2::{Bcs, Event};
use sui_types::base_types::SuiAddress;

const MAX_PAGE_SIZE: u32 = 1000;
const DEFAULT_PAGE_SIZE: u32 = 1000;
const MAX_PAGE_SIZE_BYTES: usize = 512 * 1024; // 512KiB

#[derive(serde::Serialize, serde::Deserialize)]
struct PageToken {
    stream_id: SuiAddress,
    start_checkpoint: u64,
    last_event_checkpoint: u64,
    last_event_transaction_idx: u32,
    last_event_index: u32,
}

fn to_grpc_event(ev: &sui_types::event::Event) -> Event {
    let mut bcs = Bcs::default();
    bcs.value = Some(ev.contents.clone().into());

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
    authenticated_event.event_idx = Some(idx);
    authenticated_event.event = Some(to_grpc_event(ev));
    authenticated_event.stream_id = Some(stream_id.to_string());
    authenticated_event
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
    if !service.config.authenticated_events_indexing() {
        return Err(RpcError::new(
            tonic::Code::Unimplemented,
            "Authenticated events indexing is disabled".to_string(),
        ));
    }
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

    if let Some(token) = &page_token
        && token.stream_id != stream_addr
    {
        return Err(RpcError::new(
            tonic::Code::InvalidArgument,
            "page_token stream_id mismatch".to_string(),
        ));
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
        response.highest_indexed_checkpoint = Some(highest_indexed);
        response.next_page_token = None;
        return Ok(response);
    }

    let start_transaction_idx = page_token.as_ref().map(|t| t.last_event_transaction_idx);
    let start_event_idx = page_token.as_ref().map(|t| t.last_event_index);

    let iter = indexes
        .authenticated_event_iter(
            stream_addr,
            start,
            start_transaction_idx,
            start_event_idx,
            highest_indexed,
            page_size,
        )
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

    let mut events = Vec::new();
    let mut size_bytes = 0;
    let mut events_processed: u32 = 0;
    let mut last_event_info: Option<(u64, u32, u32)> = None;

    for event_result in iter {
        let (cp, transaction_idx, event_idx, ev) =
            event_result.map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

        // Check if we've reached our size limit
        if size_bytes >= MAX_PAGE_SIZE_BYTES {
            break;
        }

        let authenticated_event =
            to_authenticated_event(&stream_id, cp, transaction_idx, event_idx, &ev);
        size_bytes += authenticated_event.encoded_len();
        events.push(authenticated_event);
        last_event_info = Some((cp, transaction_idx, event_idx));
        events_processed += 1;
    }

    let next_page_token = if events_processed == page_size {
        last_event_info.map(|(last_cp, last_tx_idx, last_ev_idx)| {
            encode_page_token(PageToken {
                stream_id: stream_addr,
                start_checkpoint: start,
                last_event_checkpoint: last_cp,
                last_event_transaction_idx: last_tx_idx,
                last_event_index: last_ev_idx,
            })
        })
    } else {
        None
    };

    let mut response = ListAuthenticatedEventsResponse::default();
    response.events = events;
    response.highest_indexed_checkpoint = Some(highest_indexed);
    response.next_page_token = next_page_token.map(|token| token.to_vec());
    Ok(response)
}
