// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_inverted_index::BitmapLiteral;
use sui_inverted_index::BitmapQuery;
use sui_inverted_index::BitmapTerm;
use sui_inverted_index::EVENT_EXTANT_VALUE;
use sui_inverted_index::IndexDimension;
use sui_inverted_index::TX_UNIVERSE_VALUE;
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
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;

use crate::ErrorReason;
use crate::RpcError;
use crate::proto::google::rpc::bad_request::FieldViolation;

/// Convert a proto `TransactionFilter` DNF filter into a `BitmapQuery`.
///
/// `max_literals` caps the total number of literals across all terms. Each
/// literal can become one bitmap dimension stream, so this prevents one
/// filter from monopolizing bitmap scan fanout.
pub fn transaction_filter_to_query(
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
/// The resulting query is evaluated against the event-keyed bitmap index, so
/// every leaf, including tx-level predicates like `Sender`, resolves precisely
/// in event-space: no post-filter pass is required.
pub fn event_filter_to_query(
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
    let has_include = term.literals.iter().any(|literal| {
        matches!(
            literal.polarity,
            Some(transaction_literal::Polarity::Include(_))
        )
    });

    let mut literals = Vec::with_capacity(term.literals.len() + usize::from(!has_include));
    // Unanchored negation: a term with only excludes resolves as
    // `range \ union(excludes)`. The tx-seq namespace is dense, so the
    // universe needs no stored marker — backends synthesize full buckets for
    // the `TxUniverse` key at scan time. The synthetic include anchors the
    // term so the merge-join driver's floor advances through every bucket in
    // range. `expect` is sound for the same reason as the event-side synthesis.
    if !has_include {
        literals.push(
            BitmapLiteral::include(encode_dimension_key(
                IndexDimension::TxUniverse,
                TX_UNIVERSE_VALUE,
            ))
            .expect("TxUniverse key is statically valid"),
        );
    }
    for p in &term.literals {
        literals.push(convert_transaction_literal(p, "filter.terms.literals")?);
    }

    BitmapTerm::new(literals).map_err(|e| {
        FieldViolation::new("filter.terms")
            .with_description(e.to_string())
            .with_reason(ErrorReason::FieldInvalid)
            .into()
    })
}

fn event_term_to_query(term: &EventTerm) -> Result<BitmapTerm, RpcError> {
    let has_include = term
        .literals
        .iter()
        .any(|literal| matches!(literal.polarity, Some(event_literal::Polarity::Include(_))));

    let mut literals = Vec::with_capacity(term.literals.len() + usize::from(!has_include));
    // Unanchored negation: a term with only excludes resolves as
    // `EventExtant \ union(excludes)`. The synthetic include anchors the
    // term on the existence marker so the merge-join driver's floor advances
    // through every bucket that contains a real event. `expect` is sound: the
    // key bytes are a workspace-constant `[EventExtant.tag_byte(), 0x00]`, so
    // `BitmapKey::new`'s validation can only fail under a source change
    // that test suites would catch.
    if !has_include {
        literals.push(
            BitmapLiteral::include(encode_dimension_key(
                IndexDimension::EventExtant,
                EVENT_EXTANT_VALUE,
            ))
            .expect("EventExtant universe key is statically valid"),
        );
    }
    for p in &term.literals {
        literals.push(convert_event_literal(p, "filter.terms.literals")?);
    }

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
    Ok(encode_dimension_key(IndexDimension::Sender, &bytes))
}

fn convert_affected_address(f: &AffectedAddressFilter, field: &str) -> Result<Vec<u8>, RpcError> {
    let addr = f.address.as_deref().ok_or_else(|| {
        FieldViolation::new(field)
            .with_description("address is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    let bytes = parse_address(addr, field)?;
    Ok(encode_dimension_key(
        IndexDimension::AffectedAddress,
        &bytes,
    ))
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
    Ok(encode_dimension_key(
        IndexDimension::AffectedObject,
        object_id.as_ref(),
    ))
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
    let value = move_call_value(&pkg_bytes, parts.get(1).copied(), parts.get(2).copied());
    Ok(encode_dimension_key(IndexDimension::MoveCall, &value))
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
    Ok(encode_dimension_key(IndexDimension::EmitModule, &value))
}

fn convert_event_type(f: &EventTypeFilter, field: &str) -> Result<Vec<u8>, RpcError> {
    let s = f.r#type.as_deref().ok_or_else(|| {
        FieldViolation::new(field)
            .with_description("type is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
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
    Ok(encode_dimension_key(IndexDimension::EventType, &value))
}

fn convert_event_stream_head(f: &EventStreamHeadFilter, field: &str) -> Result<Vec<u8>, RpcError> {
    let stream_id = f.stream_id.as_deref().ok_or_else(|| {
        FieldViolation::new(field)
            .with_description("stream_id is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    let bytes = parse_address(stream_id, field)?;
    Ok(encode_dimension_key(
        IndexDimension::EventStreamHead,
        &bytes,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MAX_LITERALS: usize = 10;

    fn sender_literal(byte: u8) -> TransactionLiteral {
        tx_sender_literal(byte, false)
    }

    fn tx_sender_literal(byte: u8, exclude: bool) -> TransactionLiteral {
        let address = SuiAddress::from_bytes([byte; 32])
            .expect("valid address bytes")
            .to_string();
        let mut sender = SenderFilter::default();
        sender.address = Some(address);

        let mut predicate = TransactionPredicate::default();
        predicate.predicate = Some(transaction_predicate::Predicate::Sender(sender));

        let mut literal = TransactionLiteral::default();
        literal.polarity = Some(if exclude {
            transaction_literal::Polarity::Exclude(predicate)
        } else {
            transaction_literal::Polarity::Include(predicate)
        });
        literal
    }

    fn tx_term(literals: Vec<TransactionLiteral>) -> TransactionTerm {
        let mut term = TransactionTerm::default();
        term.literals = literals;
        term
    }

    fn tx_filter_with_terms(terms: Vec<TransactionTerm>) -> TransactionFilter {
        let mut filter = TransactionFilter::default();
        filter.terms = terms;
        filter
    }

    fn tx_universe_key() -> Vec<u8> {
        encode_dimension_key(IndexDimension::TxUniverse, TX_UNIVERSE_VALUE)
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

    fn ev_sender_literal(byte: u8, exclude: bool) -> EventLiteral {
        let address = SuiAddress::from_bytes([byte; 32])
            .expect("valid address bytes")
            .to_string();
        let mut sender = SenderFilter::default();
        sender.address = Some(address);

        let mut predicate = EventPredicate::default();
        predicate.predicate = Some(event_predicate::Predicate::Sender(sender));

        let mut literal = EventLiteral::default();
        literal.polarity = Some(if exclude {
            event_literal::Polarity::Exclude(predicate)
        } else {
            event_literal::Polarity::Include(predicate)
        });
        literal
    }

    fn ev_term(literals: Vec<EventLiteral>) -> EventTerm {
        let mut term = EventTerm::default();
        term.literals = literals;
        term
    }

    fn ev_filter_with_terms(terms: Vec<EventTerm>) -> EventFilter {
        let mut filter = EventFilter::default();
        filter.terms = terms;
        filter
    }

    fn universe_key() -> Vec<u8> {
        encode_dimension_key(IndexDimension::EventExtant, EVENT_EXTANT_VALUE)
    }

    #[test]
    fn event_filter_exclude_only_term_synthesizes_event_extant_include() {
        let filter = ev_filter_with_terms(vec![ev_term(vec![ev_sender_literal(1, true)])]);
        let query =
            event_filter_to_query(&filter, TEST_MAX_LITERALS).expect("unanchored term is valid");

        let term = &query.terms()[0];
        let literals = term.literals();
        assert_eq!(literals.len(), 2, "synthetic include + user exclude");
        assert!(
            matches!(literals[0], BitmapLiteral::Include(_)),
            "synthetic universe include is prepended"
        );
        assert_eq!(literals[0].key_bytes(), universe_key().as_slice());
        assert!(matches!(literals[1], BitmapLiteral::Exclude(_)));
    }

    #[test]
    fn event_filter_multiple_unanchored_terms_each_get_universe_include() {
        let filter = ev_filter_with_terms(vec![
            ev_term(vec![ev_sender_literal(1, true)]),
            ev_term(vec![ev_sender_literal(2, true)]),
        ]);
        let query =
            event_filter_to_query(&filter, TEST_MAX_LITERALS).expect("unanchored terms are valid");

        for term in query.terms() {
            assert_eq!(term.literals()[0].key_bytes(), universe_key().as_slice());
        }
    }

    #[test]
    fn event_filter_mixed_dnf_only_synthesizes_when_needed() {
        let filter = ev_filter_with_terms(vec![
            ev_term(vec![ev_sender_literal(1, false)]),
            ev_term(vec![ev_sender_literal(2, true)]),
        ]);
        let query = event_filter_to_query(&filter, TEST_MAX_LITERALS).expect("mixed DNF is valid");

        let terms = query.terms();
        assert_ne!(
            terms[0].literals()[0].key_bytes(),
            universe_key().as_slice(),
            "anchored term gets no synthetic include"
        );
        assert_eq!(
            terms[1].literals()[0].key_bytes(),
            universe_key().as_slice(),
            "unanchored term gets the synthetic include"
        );
    }

    #[test]
    fn event_filter_unanchored_term_does_not_count_against_fanout() {
        // Two unanchored terms with one exclude each = 2 user-provided literals;
        // synthetic includes are not counted, so we stay under the cap.
        let filter = ev_filter_with_terms(vec![
            ev_term(vec![ev_sender_literal(1, true)]),
            ev_term(vec![ev_sender_literal(2, true)]),
        ]);
        assert!(event_filter_to_query(&filter, 2).is_ok());
    }

    #[test]
    fn transaction_filter_exclude_only_term_synthesizes_tx_universe_include() {
        let filter = tx_filter_with_terms(vec![tx_term(vec![tx_sender_literal(1, true)])]);
        let query = transaction_filter_to_query(&filter, TEST_MAX_LITERALS)
            .expect("unanchored term is valid");

        let term = &query.terms()[0];
        let literals = term.literals();
        assert_eq!(literals.len(), 2, "synthetic include + user exclude");
        assert!(
            matches!(literals[0], BitmapLiteral::Include(_)),
            "synthetic universe include is prepended"
        );
        assert_eq!(literals[0].key_bytes(), tx_universe_key().as_slice());
        assert!(matches!(literals[1], BitmapLiteral::Exclude(_)));
    }

    #[test]
    fn transaction_filter_unanchored_term_does_not_count_against_fanout() {
        // Two unanchored terms with one exclude each = 2 user-provided literals;
        // synthetic includes are not counted, so we stay under the cap.
        let filter = tx_filter_with_terms(vec![
            tx_term(vec![tx_sender_literal(1, true)]),
            tx_term(vec![tx_sender_literal(2, true)]),
        ]);
        assert!(transaction_filter_to_query(&filter, 2).is_ok());
    }
}
