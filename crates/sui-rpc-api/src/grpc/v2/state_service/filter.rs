// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Converts the `filter` of `ListOwnedObjects` and `ListDynamicFields` requests
//! into the filters the index reader evaluates.

use std::str::FromStr;

use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2::DynamicFieldFilter;
use sui_rpc::proto::sui::rpc::v2::ErrorReason;
use sui_rpc::proto::sui::rpc::v2::ObjectFilter;
use sui_rpc::proto::sui::rpc::v2::dynamic_field_literal;
use sui_rpc::proto::sui::rpc::v2::object_literal;
use sui_types::storage;
use sui_types::storage::DynamicFieldPredicate;
use sui_types::storage::FilterLiteral;
use sui_types::storage::OwnedObjectFilter;

use crate::RpcError;

/// The most literals a filter may hold across all of its terms. Each literal
/// adds at most three key ranges to the scan (a dynamic field name type at
/// `0x2` splits around the `Wrapper` range), and the scan seeks at most once
/// per range, so this bounds the seeks a page can cost without matching
/// anything.
const MAX_FILTER_LITERALS: usize = 32;

const LITERALS_FIELD: &str = "filter.terms.literals";

pub(super) fn owned_object_filter(filter: &ObjectFilter) -> Result<OwnedObjectFilter, RpcError> {
    validate_shape(filter.terms.iter().map(|term| term.literals.len()))?;
    let terms = filter
        .terms
        .iter()
        .map(|term| {
            term.literals
                .iter()
                .map(|literal| {
                    let object_type =
                        match literal.predicate.as_ref().ok_or_else(missing_predicate)? {
                            object_literal::Predicate::ObjectType(object_type) => object_type,
                            _ => return Err(unsupported_predicate()),
                        };
                    let field = format!("{LITERALS_FIELD}.object_type.object_type");
                    Ok(FilterLiteral {
                        negated: literal.negated,
                        predicate: parse_required(object_type.object_type.as_deref(), &field)?,
                    })
                })
                .collect::<Result<Vec<_>, RpcError>>()
        })
        .collect::<Result<_, _>>()?;
    Ok(OwnedObjectFilter { terms })
}

pub(super) fn dynamic_field_filter(
    filter: &DynamicFieldFilter,
) -> Result<storage::DynamicFieldFilter, RpcError> {
    validate_shape(filter.terms.iter().map(|term| term.literals.len()))?;
    let terms = filter
        .terms
        .iter()
        .map(|term| {
            term.literals
                .iter()
                .map(|literal| {
                    let predicate =
                        match literal.predicate.as_ref().ok_or_else(missing_predicate)? {
                            dynamic_field_literal::Predicate::NameType(name_type) => {
                                let field = format!("{LITERALS_FIELD}.name_type.name_type");
                                DynamicFieldPredicate::NameType(parse_required(
                                    name_type.name_type.as_deref(),
                                    &field,
                                )?)
                            }
                            dynamic_field_literal::Predicate::DynamicObjectField(_) => {
                                DynamicFieldPredicate::DynamicObjectField
                            }
                            _ => return Err(unsupported_predicate()),
                        };
                    Ok(FilterLiteral {
                        negated: literal.negated,
                        predicate,
                    })
                })
                .collect::<Result<Vec<_>, RpcError>>()
        })
        .collect::<Result<_, _>>()?;
    Ok(storage::DynamicFieldFilter { terms })
}

/// A filter needs at least one term, every term at least one literal, and at
/// most [`MAX_FILTER_LITERALS`] literals in all.
fn validate_shape(literals_per_term: impl ExactSizeIterator<Item = usize>) -> Result<(), RpcError> {
    if literals_per_term.len() == 0 {
        return Err(FieldViolation::new("filter.terms")
            .with_description("at least one filter term is required")
            .with_reason(ErrorReason::FieldMissing)
            .into());
    }
    let mut total = 0;
    for count in literals_per_term {
        if count == 0 {
            return Err(FieldViolation::new(LITERALS_FIELD)
                .with_description("every filter term needs at least one literal")
                .with_reason(ErrorReason::FieldMissing)
                .into());
        }
        total += count;
    }
    if total > MAX_FILTER_LITERALS {
        return Err(FieldViolation::new(LITERALS_FIELD)
            .with_description(format!(
                "filter contains {total} literals; at most {MAX_FILTER_LITERALS} are allowed"
            ))
            .with_reason(ErrorReason::FieldInvalid)
            .into());
    }
    Ok(())
}

fn missing_predicate() -> RpcError {
    FieldViolation::new(LITERALS_FIELD)
        .with_description("predicate variant is required")
        .with_reason(ErrorReason::FieldMissing)
        .into()
}

fn unsupported_predicate() -> RpcError {
    FieldViolation::new(LITERALS_FIELD)
        .with_description("predicate variant is not supported")
        .with_reason(ErrorReason::FieldInvalid)
        .into()
}

fn parse_required<T>(value: Option<&str>, field: &str) -> Result<T, RpcError>
where
    T: FromStr<Err = anyhow::Error>,
{
    let value = value.ok_or_else(|| {
        FieldViolation::new(field)
            .with_description("type is required")
            .with_reason(ErrorReason::FieldMissing)
    })?;
    value.parse().map_err(|e: anyhow::Error| {
        FieldViolation::new(field)
            .with_description(format!("invalid type `{value}`: {e}"))
            .with_reason(ErrorReason::FieldInvalid)
            .into()
    })
}

#[cfg(test)]
mod tests {
    use sui_rpc::proto::sui::rpc::v2::DynamicFieldLiteral;
    use sui_rpc::proto::sui::rpc::v2::DynamicFieldNameTypeFilter;
    use sui_rpc::proto::sui::rpc::v2::DynamicFieldTerm;
    use sui_rpc::proto::sui::rpc::v2::ObjectLiteral;
    use sui_rpc::proto::sui::rpc::v2::ObjectTerm;
    use sui_rpc::proto::sui::rpc::v2::ObjectTypeFilter;

    use super::*;

    fn object_literal(negated: bool, object_type: &str) -> ObjectLiteral {
        let mut filter = ObjectTypeFilter::default();
        filter.object_type = Some(object_type.to_owned());
        let mut literal = ObjectLiteral::default();
        literal.negated = negated;
        literal.predicate = Some(object_literal::Predicate::ObjectType(filter));
        literal
    }

    fn object_filter(terms: Vec<Vec<ObjectLiteral>>) -> ObjectFilter {
        let mut filter = ObjectFilter::default();
        filter.terms = terms
            .into_iter()
            .map(|literals| {
                let mut term = ObjectTerm::default();
                term.literals = literals;
                term
            })
            .collect();
        filter
    }

    fn reason(error: RpcError) -> String {
        format!("{:?}", tonic::Status::from(error))
    }

    #[test]
    fn converts_negated_object_types() {
        let filter = owned_object_filter(&object_filter(vec![vec![
            object_literal(true, "0x2::coin::Coin"),
            object_literal(true, "0xd22b::suins_registration"),
        ]]))
        .unwrap();
        assert_eq!(filter.literal_count(), 2);
        assert!(filter.terms[0].iter().all(|literal| literal.negated));
        assert!(!filter.matches_object_type(
            &sui_types::parse_sui_struct_tag("0x2::coin::Coin<0x2::sui::SUI>").unwrap()
        ));
    }

    #[test]
    fn rejects_malformed_filters() {
        assert!(
            reason(owned_object_filter(&object_filter(vec![])).unwrap_err())
                .contains("at least one filter term")
        );
        assert!(
            reason(owned_object_filter(&object_filter(vec![vec![]])).unwrap_err())
                .contains("at least one literal")
        );
        let too_many = vec![vec![object_literal(false, "0x2"); MAX_FILTER_LITERALS + 1]];
        assert!(
            reason(owned_object_filter(&object_filter(too_many)).unwrap_err())
                .contains("at most 32")
        );
        assert!(
            reason(
                owned_object_filter(&object_filter(vec![vec![object_literal(
                    false,
                    "0x2::a::b::c"
                )]]))
                .unwrap_err()
            )
            .contains("invalid type")
        );

        let mut term = DynamicFieldTerm::default();
        term.literals = vec![DynamicFieldLiteral::default()];
        let mut filter = DynamicFieldFilter::default();
        filter.terms = vec![term];
        assert!(reason(dynamic_field_filter(&filter).unwrap_err()).contains("predicate variant"));
    }

    #[test]
    fn converts_dynamic_field_predicates() {
        let mut name_type = DynamicFieldNameTypeFilter::default();
        name_type.name_type = Some("0x2::kiosk::Item".to_owned());
        let mut name = DynamicFieldLiteral::default();
        name.predicate = Some(dynamic_field_literal::Predicate::NameType(name_type));
        let mut object = DynamicFieldLiteral::default();
        object.negated = true;
        object.predicate = Some(dynamic_field_literal::Predicate::DynamicObjectField(
            Default::default(),
        ));
        let mut term = DynamicFieldTerm::default();
        term.literals = vec![name, object];
        let mut filter = DynamicFieldFilter::default();
        filter.terms = vec![term];

        let filter = dynamic_field_filter(&filter).unwrap();
        assert_eq!(
            filter.terms,
            vec![vec![
                FilterLiteral {
                    negated: false,
                    predicate: DynamicFieldPredicate::NameType("0x2::kiosk::Item".parse().unwrap()),
                },
                FilterLiteral {
                    negated: true,
                    predicate: DynamicFieldPredicate::DynamicObjectField,
                },
            ]]
        );
    }
}
