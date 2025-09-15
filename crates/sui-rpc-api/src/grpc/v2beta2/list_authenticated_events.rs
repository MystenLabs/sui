// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::grpc::v2beta2::event_service_proto::{
    AuthenticatedEvent, Bcs, Event, EventStreamHead, ListAuthenticatedEventsRequest,
    ListAuthenticatedEventsResponse, Proof,
};
use crate::RpcError;
use crate::RpcService;
use move_core_types::language_storage::{StructTag, TypeTag};
use std::str::FromStr;
use std::sync::Arc;
use sui_types::accumulator_root as ar;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::MoveTypeTagTraitGeneric;

fn to_grpc_event(ev: &sui_types::event::Event) -> Event {
    let bcs = Bcs {
        value: Some(ev.contents.clone()),
    };
    Event {
        package_id: Some(ev.package_id.to_canonical_string(true)),
        module: Some(ev.transaction_module.to_string()),
        sender: Some(ev.sender.to_string()),
        event_type: Some(ev.type_.to_canonical_string(true)),
        contents: Some(bcs),
        json: None,
    }
}

fn to_authenticated_event(
    stream_id: &str,
    cp: u64,
    transaction_idx: u32,
    idx: u32,
    ev: &sui_types::event::Event,
) -> AuthenticatedEvent {
    AuthenticatedEvent {
        checkpoint: Some(cp),
        transaction_idx: Some(transaction_idx),
        event_index: Some(idx),
        event: Some(to_grpc_event(ev)),
        stream_id: Some(stream_id.to_string()),
    }
}

pub(crate) fn load_event_stream_head(
    reader: &Arc<dyn sui_types::storage::RpcStateReader>,
    stream_id: &str,
    at_checkpoint: u64,
) -> Option<EventStreamHead> {
    #[derive(serde::Deserialize)]
    struct MoveEventStreamHead {
        mmr: Vec<Vec<u8>>,
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
    for exec in contents.iter() {
        let tx = exec.transaction;
        if let Some(effects) = reader.get_transaction_effects(&tx) {
            for (obj_id, ver, _dig) in effects.written() {
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

    Some(EventStreamHead {
        mmr: field.value.mmr,
        checkpoint_seq: Some(field.value.checkpoint_seq),
        num_events: Some(field.value.num_events),
    })
}

#[tracing::instrument(skip(_service))]
pub fn list_authenticated_events(
    _service: &RpcService,
    request: ListAuthenticatedEventsRequest,
) -> Result<ListAuthenticatedEventsResponse, RpcError> {
    let stream_id = request.stream_id.ok_or_else(|| {
        RpcError::new(
            tonic::Code::InvalidArgument,
            "missing stream_id".to_string(),
        )
    })?;

    let start = request.start_checkpoint.unwrap_or(0);
    if let Some(lim) = request.limit {
        if lim > 1000 {
            return Err(RpcError::new(
                tonic::Code::InvalidArgument,
                "limit must be <= 1000".to_string(),
            ));
        }
    }
    let limit = request.limit.unwrap_or(1000);
    let end = start.saturating_add(limit.saturating_sub(1));

    let reader = _service.reader.inner();
    let indexes = reader.indexes().ok_or_else(RpcError::not_found)?;

    let stream_addr = sui_types::base_types::SuiAddress::from_str(&stream_id).map_err(|e| {
        RpcError::new(
            tonic::Code::InvalidArgument,
            format!("invalid stream_id: {e}"),
        )
    })?;

    let highest_indexed = reader
        .indexes()
        .and_then(|idx| {
            idx.get_highest_indexed_checkpoint_seq_number()
                .ok()
                .flatten()
        })
        .unwrap();
    let capped_end = end.min(highest_indexed);
    let iter = indexes
        .authenticated_event_iter(stream_addr, start, capped_end)
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
    let events: Vec<AuthenticatedEvent> = iter
        .map(|res| {
            res.map(|(cp, transaction_idx, idx, ev)| {
                to_authenticated_event(&stream_id, cp, transaction_idx, idx, &ev)
            })
        })
        .collect::<Result<_, _>>()
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
    let last_checkpoint_with_events = events.last().and_then(|e| e.checkpoint);

    let event_stream_head = last_checkpoint_with_events
        .and_then(|last_checkpoint| load_event_stream_head(reader, &stream_id, last_checkpoint));
    Ok(ListAuthenticatedEventsResponse {
        events,
        proof: event_stream_head.map(|esh| Proof {
            event_stream_head: Some(esh),
        }),
        last_checkpoint: Some(capped_end),
    })
}
