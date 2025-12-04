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
    next_checkpoint: u64,
    next_accumulator_version: u64,
    next_transaction_idx: u32,
    next_event_idx: u32,
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
    accumulator_version: u64,
    transaction_idx: u32,
    idx: u32,
    ev: &sui_types::event::Event,
) -> AuthenticatedEvent {
    let mut authenticated_event = AuthenticatedEvent::default();
    authenticated_event.checkpoint = Some(cp);
    authenticated_event.accumulator_version = Some(accumulator_version);
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

    let (actual_start, start_accumulator_version, start_transaction_idx, start_event_idx) =
        if let Some(token) = &page_token {
            (
                token.next_checkpoint,
                Some(token.next_accumulator_version),
                Some(token.next_transaction_idx),
                Some(token.next_event_idx),
            )
        } else {
            (start, None, None, None)
        };

    let iter = indexes
        .authenticated_event_iter(
            stream_addr,
            actual_start,
            start_accumulator_version,
            start_transaction_idx,
            start_event_idx,
            highest_indexed,
            page_size + 1,
        )
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

    let mut events = Vec::new();
    let mut size_bytes = 0;
    let mut next_page_token = None;

    for (i, event_result) in iter.enumerate() {
        let (cp, accumulator_version, transaction_idx, event_idx, ev) =
            event_result.map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

        if i >= page_size as usize {
            next_page_token = Some(encode_page_token(PageToken {
                stream_id: stream_addr,
                next_checkpoint: cp,
                next_accumulator_version: accumulator_version,
                next_transaction_idx: transaction_idx,
                next_event_idx: event_idx,
            }));
            break;
        }

        let authenticated_event = to_authenticated_event(
            &stream_id,
            cp,
            accumulator_version,
            transaction_idx,
            event_idx,
            &ev,
        );
        let event_size = authenticated_event.encoded_len();

        if i > 0 && size_bytes + event_size > MAX_PAGE_SIZE_BYTES {
            next_page_token = Some(encode_page_token(PageToken {
                stream_id: stream_addr,
                next_checkpoint: cp,
                next_accumulator_version: accumulator_version,
                next_transaction_idx: transaction_idx,
                next_event_idx: event_idx,
            }));
            break;
        }

        size_bytes += event_size;
        events.push(authenticated_event);
    }

    let mut response = ListAuthenticatedEventsResponse::default();
    response.events = events;
    response.highest_indexed_checkpoint = Some(highest_indexed);
    response.next_page_token = next_page_token.map(|t| t.into());
    Ok(response)
}
