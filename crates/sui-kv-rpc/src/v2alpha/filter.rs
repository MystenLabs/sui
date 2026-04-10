// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::proto::sui::rpc::kv::v2alpha::AffectedObjectFilter;
use crate::proto::sui::rpc::kv::v2alpha::EmitModuleFilter;
use crate::proto::sui::rpc::kv::v2alpha::EventFilter;
use crate::proto::sui::rpc::kv::v2alpha::EventTypeFilter;
use crate::proto::sui::rpc::kv::v2alpha::MoveCallFilter;
use crate::proto::sui::rpc::kv::v2alpha::RecipientFilter;
use crate::proto::sui::rpc::kv::v2alpha::SenderFilter;
use crate::proto::sui::rpc::kv::v2alpha::TransactionFilter;
use crate::proto::sui::rpc::kv::v2alpha::event_filter;
use crate::proto::sui::rpc::kv::v2alpha::transaction_filter;
use sui_index_dimensions::IndexDimension;
use sui_index_dimensions::emit_module_value;
use sui_index_dimensions::encode_dimension_key;
use sui_index_dimensions::event_type_value;
use sui_index_dimensions::move_call_value;
use sui_kvstore::BitmapQuery;
use sui_rpc_api::ErrorReason;
use sui_rpc_api::RpcError;
use sui_rpc_api::proto::google::rpc::bad_request::FieldViolation;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;

/// Convert a proto `TransactionFilter` expression tree into a `BitmapQuery`.
pub(crate) fn transaction_filter_to_query(
    filter: &TransactionFilter,
) -> Result<BitmapQuery, RpcError> {
    let f = filter.filter.as_ref().ok_or_else(|| {
        FieldViolation::new("filter")
            .with_description("filter variant is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    match f {
        transaction_filter::Filter::And(and) => {
            let children = and
                .filters
                .iter()
                .map(transaction_filter_to_query)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(BitmapQuery::and(children))
        }
        transaction_filter::Filter::Or(or) => {
            let children = or
                .filters
                .iter()
                .map(transaction_filter_to_query)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(BitmapQuery::or(children))
        }
        transaction_filter::Filter::Not(not) => {
            let inner = not.filter.as_ref().ok_or_else(|| {
                FieldViolation::new("filter.not.filter")
                    .with_description("inner filter is required")
                    .with_reason(ErrorReason::FieldMissing)
            })?;
            Ok(BitmapQuery::complement(transaction_filter_to_query(inner)?))
        }
        transaction_filter::Filter::Xor(xor) => convert_xor(
            &xor.filters,
            transaction_filter_to_query,
            "filter.xor.filters",
        ),
        transaction_filter::Filter::Sender(f) => convert_sender(f),
        transaction_filter::Filter::Recipient(f) => convert_recipient(f),
        transaction_filter::Filter::AffectedObject(f) => convert_affected_object(f),
        transaction_filter::Filter::MoveCall(f) => convert_move_call(f),
        transaction_filter::Filter::EmitModule(f) => convert_emit_module(f),
        transaction_filter::Filter::EventType(f) => convert_event_type(f),
    }
}

/// Convert a proto `EventFilter` expression tree into a `BitmapQuery`.
///
/// The resulting query is evaluated against the event-keyed bitmap index
/// (see `BitmapIndexSpec::event()`), so every leaf — including tx-level
/// predicates like `Sender` — resolves precisely in event-space: no
/// post-filter pass is required.
pub(crate) fn event_filter_to_query(filter: &EventFilter) -> Result<BitmapQuery, RpcError> {
    let f = filter.filter.as_ref().ok_or_else(|| {
        FieldViolation::new("filter")
            .with_description("filter variant is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    match f {
        event_filter::Filter::And(and) => {
            let children = and
                .filters
                .iter()
                .map(event_filter_to_query)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(BitmapQuery::and(children))
        }
        event_filter::Filter::Or(or) => {
            let children = or
                .filters
                .iter()
                .map(event_filter_to_query)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(BitmapQuery::or(children))
        }
        event_filter::Filter::Not(not) => {
            let inner = not.filter.as_ref().ok_or_else(|| {
                FieldViolation::new("filter.not.filter")
                    .with_description("inner filter is required")
                    .with_reason(ErrorReason::FieldMissing)
            })?;
            Ok(BitmapQuery::complement(event_filter_to_query(inner)?))
        }
        event_filter::Filter::Xor(xor) => {
            convert_xor(&xor.filters, event_filter_to_query, "filter.xor.filters")
        }
        event_filter::Filter::Sender(f) => convert_sender(f),
        event_filter::Filter::AffectedObject(f) => convert_affected_object(f),
        event_filter::Filter::MoveCall(f) => convert_move_call(f),
        event_filter::Filter::EmitModule(f) => convert_emit_module(f),
        event_filter::Filter::EventType(f) => convert_event_type(f),
    }
}

// --- Leaf predicate helpers ---

fn parse_address(hex: &str, field: &str) -> Result<[u8; 32], RpcError> {
    hex.parse::<SuiAddress>()
        .map(|a| a.to_inner())
        .map_err(|e| {
            FieldViolation::new(field)
                .with_description(format!("invalid address: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
                .into()
        })
}

fn convert_sender(f: &SenderFilter) -> Result<BitmapQuery, RpcError> {
    let addr = f.address.as_deref().ok_or_else(|| {
        FieldViolation::new("filter.sender.address")
            .with_description("address is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    let bytes = parse_address(addr, "filter.sender.address")?;
    let key = encode_dimension_key(IndexDimension::Sender, &bytes);
    Ok(BitmapQuery::scan(key))
}

fn convert_recipient(f: &RecipientFilter) -> Result<BitmapQuery, RpcError> {
    let addr = f.address.as_deref().ok_or_else(|| {
        FieldViolation::new("filter.recipient.address")
            .with_description("address is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    let bytes = parse_address(addr, "filter.recipient.address")?;
    let key = encode_dimension_key(IndexDimension::Recipient, &bytes);
    Ok(BitmapQuery::scan(key))
}

fn convert_affected_object(f: &AffectedObjectFilter) -> Result<BitmapQuery, RpcError> {
    let id = f.object_id.as_deref().ok_or_else(|| {
        FieldViolation::new("filter.affected_object.object_id")
            .with_description("object_id is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    let object_id = id.parse::<ObjectID>().map_err(|e| {
        FieldViolation::new("filter.affected_object.object_id")
            .with_description(format!("invalid object_id: {e}"))
            .with_reason(ErrorReason::FieldInvalid)
    })?;
    let key = encode_dimension_key(IndexDimension::AffectedObject, object_id.as_ref());
    Ok(BitmapQuery::scan(key))
}

fn convert_move_call(f: &MoveCallFilter) -> Result<BitmapQuery, RpcError> {
    let s = f.function.as_deref().ok_or_else(|| {
        FieldViolation::new("filter.move_call.function")
            .with_description("function is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    let parts: Vec<&str> = s.split("::").collect();
    if parts.is_empty() || parts.len() > 3 {
        return Err(FieldViolation::new("filter.move_call.function")
            .with_description("expected `package[::module[::function]]`")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }
    let pkg_bytes = parse_address(parts[0], "filter.move_call.function")?;
    let module = parts.get(1).copied();
    let function = parts.get(2).copied();
    let value = move_call_value(&pkg_bytes, module, function);
    let key = encode_dimension_key(IndexDimension::MoveCall, &value);
    Ok(BitmapQuery::scan(key))
}

fn convert_emit_module(f: &EmitModuleFilter) -> Result<BitmapQuery, RpcError> {
    let s = f.module.as_deref().ok_or_else(|| {
        FieldViolation::new("filter.emit_module.module")
            .with_description("module is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    let parts: Vec<&str> = s.split("::").collect();
    if parts.is_empty() || parts.len() > 2 {
        return Err(FieldViolation::new("filter.emit_module.module")
            .with_description("expected `package[::module]`")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }
    let pkg_bytes = parse_address(parts[0], "filter.emit_module.module")?;
    let value = emit_module_value(&pkg_bytes, parts.get(1).copied());
    let key = encode_dimension_key(IndexDimension::EmitModule, &value);
    Ok(BitmapQuery::scan(key))
}

fn convert_event_type(f: &EventTypeFilter) -> Result<BitmapQuery, RpcError> {
    let s = f.r#type.as_deref().ok_or_else(|| {
        FieldViolation::new("filter.event_type.type")
            .with_description("type is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    // Split head (before any `<`) for prefix levels; keep the generic
    // instantiation as a single trailing component so nested `::` inside
    // type parameters don't confuse prefix splitting.
    let (head, generics) = match s.find('<') {
        Some(i) => (&s[..i], Some(&s[i..])),
        None => (s, None),
    };
    let parts: Vec<&str> = head.split("::").collect();
    if parts.is_empty() || parts.len() > 3 {
        return Err(FieldViolation::new("filter.event_type.type")
            .with_description("expected `address[::module[::Name[<type_params>]]]`")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }
    let addr_bytes = parse_address(parts[0], "filter.event_type.type")?;
    let module = parts.get(1).copied();
    let name = parts.get(2).copied();
    if generics.is_some() && name.is_none() {
        return Err(FieldViolation::new("filter.event_type.type")
            .with_description("generic instantiation requires a type name")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }
    let instantiation_bcs = if generics.is_some() {
        // Parse the full type tag to extract type parameters.
        let tag = sui_types::parse_sui_type_tag(s).map_err(|e| {
            FieldViolation::new("filter.event_type.type")
                .with_description(format!("invalid type: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        let sui_types::TypeTag::Struct(st) = tag else {
            return Err(FieldViolation::new("filter.event_type.type")
                .with_description("expected a struct type")
                .with_reason(ErrorReason::FieldInvalid)
                .into());
        };
        Some(bcs::to_bytes(&st.type_params).map_err(|e| {
            RpcError::new(
                tonic::Code::Internal,
                format!("failed to BCS-encode type_params: {e}"),
            )
        })?)
    } else {
        None
    };
    let value = event_type_value(&addr_bytes, module, name, instantiation_bcs.as_deref());
    let key = encode_dimension_key(IndexDimension::EventType, &value);
    Ok(BitmapQuery::scan(key))
}

fn convert_xor<F, T>(filters: &[T], convert: F, field: &str) -> Result<BitmapQuery, RpcError>
where
    F: Fn(&T) -> Result<BitmapQuery, RpcError>,
{
    if filters.len() < 2 {
        return Err(FieldViolation::new(field)
            .with_description("xor requires at least 2 operands")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }
    let mut iter = filters.iter();
    let mut result = convert(iter.next().unwrap())?;
    for f in iter {
        result = BitmapQuery::xor(result, convert(f)?);
    }
    Ok(result)
}
