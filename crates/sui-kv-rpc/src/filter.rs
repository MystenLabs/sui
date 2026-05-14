// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_inverted_index::BitmapLiteral;
use sui_inverted_index::BitmapQuery;
use sui_inverted_index::BitmapTerm;
use sui_inverted_index::IndexDimension;
use sui_inverted_index::emit_module_value;
use sui_inverted_index::encode_dimension_key;
use sui_inverted_index::event_type_value;
use sui_inverted_index::move_call_value;
use sui_rpc::proto::sui::rpc::v2alpha::AffectedAddressFilter;
use sui_rpc::proto::sui::rpc::v2alpha::AffectedObjectFilter;
use sui_rpc::proto::sui::rpc::v2alpha::EmitModuleFilter;
use sui_rpc::proto::sui::rpc::v2alpha::EventFilter;
use sui_rpc::proto::sui::rpc::v2alpha::EventLiteral;
use sui_rpc::proto::sui::rpc::v2alpha::EventPredicate;
use sui_rpc::proto::sui::rpc::v2alpha::EventStreamHeadFilter;
use sui_rpc::proto::sui::rpc::v2alpha::EventTerm;
use sui_rpc::proto::sui::rpc::v2alpha::EventTypeFilter;
use sui_rpc::proto::sui::rpc::v2alpha::MoveCallFilter;
use sui_rpc::proto::sui::rpc::v2alpha::SenderFilter;
use sui_rpc::proto::sui::rpc::v2alpha::TransactionFilter;
use sui_rpc::proto::sui::rpc::v2alpha::TransactionLiteral;
use sui_rpc::proto::sui::rpc::v2alpha::TransactionPredicate;
use sui_rpc::proto::sui::rpc::v2alpha::TransactionTerm;
use sui_rpc::proto::sui::rpc::v2alpha::event_literal;
use sui_rpc::proto::sui::rpc::v2alpha::event_predicate;
use sui_rpc::proto::sui::rpc::v2alpha::transaction_literal;
use sui_rpc::proto::sui::rpc::v2alpha::transaction_predicate;
use sui_rpc_api::ErrorReason;
use sui_rpc_api::RpcError;
use sui_rpc_api::proto::google::rpc::bad_request::FieldViolation;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;

/// Convert a proto `TransactionFilter` DNF filter into a `BitmapQuery`.
///
/// `max_literals` caps the total number of literals across all terms. Each
/// literal can become one bitmap dimension stream, so this prevents one
/// filter from monopolizing bitmap scan fanout.
pub(crate) fn transaction_filter_to_query(
    filter: &TransactionFilter,
    max_literals: usize,
) -> Result<BitmapQuery, RpcError> {
    if filter.terms.is_empty() {
        return Err(FieldViolation::new("filter.terms")
            .with_description("at least one filter term is required")
            .with_reason(ErrorReason::FieldMissing)
            .into());
    }
    validate_literal_fanout(
        filter.terms.iter().map(|term| term.literals.len()),
        max_literals,
    )?;

    let terms = filter
        .terms
        .iter()
        .map(transaction_term_to_query)
        .collect::<Result<Vec<_>, _>>()?;
    BitmapQuery::new(terms).map_err(|e| {
        FieldViolation::new("filter")
            .with_description(e.to_string())
            .with_reason(ErrorReason::FieldInvalid)
            .into()
    })
}

/// Convert a proto `EventFilter` DNF filter into a `BitmapQuery`.
///
/// The resulting query is evaluated against the event-keyed bitmap index
/// (see `BitmapIndexSpec::event()`), so every leaf — including tx-level
/// predicates like `Sender` — resolves precisely in event-space: no
/// post-filter pass is required.
pub(crate) fn event_filter_to_query(
    filter: &EventFilter,
    max_literals: usize,
) -> Result<BitmapQuery, RpcError> {
    if filter.terms.is_empty() {
        return Err(FieldViolation::new("filter.terms")
            .with_description("at least one filter term is required")
            .with_reason(ErrorReason::FieldMissing)
            .into());
    }
    validate_literal_fanout(
        filter.terms.iter().map(|term| term.literals.len()),
        max_literals,
    )?;

    let terms = filter
        .terms
        .iter()
        .map(event_term_to_query)
        .collect::<Result<Vec<_>, _>>()?;
    BitmapQuery::new(terms).map_err(|e| {
        FieldViolation::new("filter")
            .with_description(e.to_string())
            .with_reason(ErrorReason::FieldInvalid)
            .into()
    })
}

fn validate_literal_fanout(
    counts: impl IntoIterator<Item = usize>,
    max: usize,
) -> Result<(), RpcError> {
    let mut total = 0usize;
    for count in counts {
        total += count;
        if total > max {
            return Err(FieldViolation::new("filter.terms.literals")
                .with_description(format!(
                    "filter contains {total} literals; at most {max} are allowed"
                ))
                .with_reason(ErrorReason::FieldInvalid)
                .into());
        }
    }
    Ok(())
}

fn transaction_term_to_query(term: &TransactionTerm) -> Result<BitmapTerm, RpcError> {
    if !term.literals.iter().any(|literal| {
        matches!(
            literal.polarity,
            Some(transaction_literal::Polarity::Include(_))
        )
    }) {
        return Err(FieldViolation::new("filter.terms.literals")
            .with_description("at least one included literal is required")
            .with_reason(ErrorReason::FieldMissing)
            .into());
    }

    let literals = term
        .literals
        .iter()
        .map(|p| convert_transaction_literal(p, "filter.terms.literals"))
        .collect::<Result<Vec<_>, _>>()?;

    BitmapTerm::new(literals).map_err(|e| {
        FieldViolation::new("filter.terms")
            .with_description(e.to_string())
            .with_reason(ErrorReason::FieldInvalid)
            .into()
    })
}

fn event_term_to_query(term: &EventTerm) -> Result<BitmapTerm, RpcError> {
    if !term
        .literals
        .iter()
        .any(|literal| matches!(literal.polarity, Some(event_literal::Polarity::Include(_))))
    {
        return Err(FieldViolation::new("filter.terms.literals")
            .with_description("at least one included literal is required")
            .with_reason(ErrorReason::FieldMissing)
            .into());
    }

    let literals = term
        .literals
        .iter()
        .map(|p| convert_event_literal(p, "filter.terms.literals"))
        .collect::<Result<Vec<_>, _>>()?;

    BitmapTerm::new(literals).map_err(|e| {
        FieldViolation::new("filter.terms")
            .with_description(e.to_string())
            .with_reason(ErrorReason::FieldInvalid)
            .into()
    })
}

fn convert_transaction_literal(
    literal: &TransactionLiteral,
    field_prefix: &str,
) -> Result<BitmapLiteral, RpcError> {
    let polarity = literal.polarity.as_ref().ok_or_else(|| {
        FieldViolation::new(field_prefix)
            .with_description("literal polarity is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    match polarity {
        transaction_literal::Polarity::Include(predicate) => {
            let key = convert_transaction_predicate(predicate, &format!("{field_prefix}.include"))?;
            BitmapLiteral::include(key).map_err(|e| {
                FieldViolation::new(field_prefix)
                    .with_description(e.to_string())
                    .with_reason(ErrorReason::FieldInvalid)
                    .into()
            })
        }
        transaction_literal::Polarity::Exclude(predicate) => {
            let key = convert_transaction_predicate(predicate, &format!("{field_prefix}.exclude"))?;
            BitmapLiteral::exclude(key).map_err(|e| {
                FieldViolation::new(field_prefix)
                    .with_description(e.to_string())
                    .with_reason(ErrorReason::FieldInvalid)
                    .into()
            })
        }
        _ => Err(FieldViolation::new(field_prefix)
            .with_description("unknown literal polarity")
            .with_reason(ErrorReason::FieldInvalid)
            .into()),
    }
}

fn convert_event_literal(
    literal: &EventLiteral,
    field_prefix: &str,
) -> Result<BitmapLiteral, RpcError> {
    let polarity = literal.polarity.as_ref().ok_or_else(|| {
        FieldViolation::new(field_prefix)
            .with_description("literal polarity is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    match polarity {
        event_literal::Polarity::Include(predicate) => {
            let key = convert_event_predicate(predicate, &format!("{field_prefix}.include"))?;
            BitmapLiteral::include(key).map_err(|e| {
                FieldViolation::new(field_prefix)
                    .with_description(e.to_string())
                    .with_reason(ErrorReason::FieldInvalid)
                    .into()
            })
        }
        event_literal::Polarity::Exclude(predicate) => {
            let key = convert_event_predicate(predicate, &format!("{field_prefix}.exclude"))?;
            BitmapLiteral::exclude(key).map_err(|e| {
                FieldViolation::new(field_prefix)
                    .with_description(e.to_string())
                    .with_reason(ErrorReason::FieldInvalid)
                    .into()
            })
        }
        _ => Err(FieldViolation::new(field_prefix)
            .with_description("unknown literal polarity")
            .with_reason(ErrorReason::FieldInvalid)
            .into()),
    }
}

fn convert_transaction_predicate(
    predicate: &TransactionPredicate,
    field_prefix: &str,
) -> Result<Vec<u8>, RpcError> {
    let predicate = predicate.predicate.as_ref().ok_or_else(|| {
        FieldViolation::new(field_prefix)
            .with_description("predicate variant is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    match predicate {
        transaction_predicate::Predicate::Sender(f) => {
            convert_sender(f, &format!("{field_prefix}.sender.address"))
        }
        transaction_predicate::Predicate::AffectedAddress(f) => {
            convert_affected_address(f, &format!("{field_prefix}.affected_address.address"))
        }
        transaction_predicate::Predicate::AffectedObject(f) => {
            convert_affected_object(f, &format!("{field_prefix}.affected_object.object_id"))
        }
        transaction_predicate::Predicate::MoveCall(f) => {
            convert_move_call(f, &format!("{field_prefix}.move_call.function"))
        }
        transaction_predicate::Predicate::EmitModule(f) => {
            convert_emit_module(f, &format!("{field_prefix}.emit_module.module"))
        }
        transaction_predicate::Predicate::EventType(f) => {
            convert_event_type(f, &format!("{field_prefix}.event_type.type"))
        }
        transaction_predicate::Predicate::EventStreamHead(f) => {
            convert_event_stream_head(f, &format!("{field_prefix}.event_stream_head.stream_id"))
        }
        _ => Err(FieldViolation::new(field_prefix)
            .with_description("unknown predicate variant")
            .with_reason(ErrorReason::FieldInvalid)
            .into()),
    }
}

fn convert_event_predicate(
    predicate: &EventPredicate,
    field_prefix: &str,
) -> Result<Vec<u8>, RpcError> {
    let predicate = predicate.predicate.as_ref().ok_or_else(|| {
        FieldViolation::new(field_prefix)
            .with_description("predicate variant is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    match predicate {
        event_predicate::Predicate::Sender(f) => {
            convert_sender(f, &format!("{field_prefix}.sender.address"))
        }
        event_predicate::Predicate::EmitModule(f) => {
            convert_emit_module(f, &format!("{field_prefix}.emit_module.module"))
        }
        event_predicate::Predicate::EventType(f) => {
            convert_event_type(f, &format!("{field_prefix}.event_type.type"))
        }
        event_predicate::Predicate::EventStreamHead(f) => {
            convert_event_stream_head(f, &format!("{field_prefix}.event_stream_head.stream_id"))
        }
        _ => Err(FieldViolation::new(field_prefix)
            .with_description("unknown predicate variant")
            .with_reason(ErrorReason::FieldInvalid)
            .into()),
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

fn convert_sender(f: &SenderFilter, field: &str) -> Result<Vec<u8>, RpcError> {
    let addr = f.address.as_deref().ok_or_else(|| {
        FieldViolation::new(field)
            .with_description("address is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    let bytes = parse_address(addr, field)?;
    let key = encode_dimension_key(IndexDimension::Sender, &bytes);
    Ok(key)
}

fn convert_affected_address(f: &AffectedAddressFilter, field: &str) -> Result<Vec<u8>, RpcError> {
    let addr = f.address.as_deref().ok_or_else(|| {
        FieldViolation::new(field)
            .with_description("address is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    let bytes = parse_address(addr, field)?;
    let key = encode_dimension_key(IndexDimension::AffectedAddress, &bytes);
    Ok(key)
}

fn convert_affected_object(f: &AffectedObjectFilter, field: &str) -> Result<Vec<u8>, RpcError> {
    let id = f.object_id.as_deref().ok_or_else(|| {
        FieldViolation::new(field)
            .with_description("object_id is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    let object_id = id.parse::<ObjectID>().map_err(|e| {
        FieldViolation::new(field)
            .with_description(format!("invalid object_id: {e}"))
            .with_reason(ErrorReason::FieldInvalid)
    })?;
    let key = encode_dimension_key(IndexDimension::AffectedObject, object_id.as_ref());
    Ok(key)
}

fn convert_move_call(f: &MoveCallFilter, field: &str) -> Result<Vec<u8>, RpcError> {
    let s = f.function.as_deref().ok_or_else(|| {
        FieldViolation::new(field)
            .with_description("function is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    let parts: Vec<&str> = s.split("::").collect();
    if parts.is_empty() || parts.len() > 3 {
        return Err(FieldViolation::new(field)
            .with_description("expected `package[::module[::function]]`")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }
    let pkg_bytes = parse_address(parts[0], field)?;
    let module = parts.get(1).copied();
    let function = parts.get(2).copied();
    let value = move_call_value(&pkg_bytes, module, function);
    let key = encode_dimension_key(IndexDimension::MoveCall, &value);
    Ok(key)
}

fn convert_emit_module(f: &EmitModuleFilter, field: &str) -> Result<Vec<u8>, RpcError> {
    let s = f.module.as_deref().ok_or_else(|| {
        FieldViolation::new(field)
            .with_description("module is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    let parts: Vec<&str> = s.split("::").collect();
    if parts.is_empty() || parts.len() > 2 {
        return Err(FieldViolation::new(field)
            .with_description("expected `package[::module]`")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }
    let pkg_bytes = parse_address(parts[0], field)?;
    let value = emit_module_value(&pkg_bytes, parts.get(1).copied());
    let key = encode_dimension_key(IndexDimension::EmitModule, &value);
    Ok(key)
}

fn convert_event_type(f: &EventTypeFilter, field: &str) -> Result<Vec<u8>, RpcError> {
    let s = f.r#type.as_deref().ok_or_else(|| {
        FieldViolation::new(field)
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
        return Err(FieldViolation::new(field)
            .with_description("expected `address[::module[::Name[<type_params>]]]`")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }
    let addr_bytes = parse_address(parts[0], field)?;
    let module = parts.get(1).copied();
    let name = parts.get(2).copied();
    if generics.is_some() && name.is_none() {
        return Err(FieldViolation::new(field)
            .with_description("generic instantiation requires a type name")
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }
    let instantiation_bcs = if generics.is_some() {
        // Parse the full type tag to extract type parameters.
        let tag = sui_types::parse_sui_type_tag(s).map_err(|e| {
            FieldViolation::new(field)
                .with_description(format!("invalid type: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        let sui_types::TypeTag::Struct(st) = tag else {
            return Err(FieldViolation::new(field)
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
    Ok(key)
}

fn convert_event_stream_head(f: &EventStreamHeadFilter, field: &str) -> Result<Vec<u8>, RpcError> {
    let stream_id = f.stream_id.as_deref().ok_or_else(|| {
        FieldViolation::new(field)
            .with_description("stream_id is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    let bytes = parse_address(stream_id, field)?;
    let key = encode_dimension_key(IndexDimension::EventStreamHead, &bytes);
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MAX_LITERALS: usize = 10;

    fn sender_literal(byte: u8) -> TransactionLiteral {
        let address = SuiAddress::from_bytes([byte; 32])
            .expect("valid address bytes")
            .to_string();
        let mut sender = SenderFilter::default();
        sender.address = Some(address);

        let mut predicate = TransactionPredicate::default();
        predicate.predicate = Some(transaction_predicate::Predicate::Sender(sender));

        let mut literal = TransactionLiteral::default();
        literal.polarity = Some(transaction_literal::Polarity::Include(predicate));
        literal
    }

    #[test]
    fn transaction_filter_accepts_max_literal_fanout() {
        let mut term = TransactionTerm::default();
        term.literals = (0..TEST_MAX_LITERALS)
            .map(|i| sender_literal(i as u8))
            .collect();

        let mut filter = TransactionFilter::default();
        filter.terms = vec![term];

        assert!(transaction_filter_to_query(&filter, TEST_MAX_LITERALS).is_ok());
    }

    #[test]
    fn transaction_filter_rejects_excess_literal_fanout() {
        let mut term = TransactionTerm::default();
        term.literals = (0..=TEST_MAX_LITERALS)
            .map(|i| sender_literal(i as u8))
            .collect();

        let mut filter = TransactionFilter::default();
        filter.terms = vec![term];

        let error = transaction_filter_to_query(&filter, TEST_MAX_LITERALS)
            .expect_err("fanout should be rejected")
            .into_status_proto();
        assert!(error.message.contains("at most 10 are allowed"));
    }

    #[test]
    fn event_stream_head_filter_encodes_dimension_key() {
        let bytes = [7; 32];
        let stream_id = SuiAddress::from_bytes(bytes)
            .expect("valid address bytes")
            .to_string();
        let mut filter = EventStreamHeadFilter::default();
        filter.stream_id = Some(stream_id);

        let key = convert_event_stream_head(
            &filter,
            "filter.terms.literals.include.event_stream_head.stream_id",
        )
        .expect("valid stream id");

        assert_eq!(
            key,
            encode_dimension_key(IndexDimension::EventStreamHead, &bytes)
        );
    }
}
