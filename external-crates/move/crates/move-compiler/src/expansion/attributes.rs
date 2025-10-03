// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    expansion::{
        ast::{self as E, ModuleIdent},
        byte_string,
        translate::Context,
    },
    ice, ice_assert,
    parser::ast as P,
    shared::{
        Name,
        known_attributes::{
            self as A, AttributeKind, AttributeKind_, AttributePosition, KnownAttribute,
            ModeAttribute, TestingAttribute,
        },
        unique_map::UniqueMap,
        unique_set::UniqueSet,
    },
};

use move_core_types::vm_status::StatusCode;
use move_ir_types::location::*;

pub fn expand_attributes(
    context: &mut Context,
    attr_position: AttributePosition,
    attribute_vec: Vec<P::Attributes>,
) -> E::Attributes {
    let mut attributes: Vec<(AttributeKind, Spanned<KnownAttribute>)> = vec![];
    for sp!(_, attrs) in attribute_vec {
        let attrs = attrs
            .0
            .into_iter()
            .filter_map(|attr| attribute(context, attr_position, attr))
            .collect::<Vec<_>>();
        let attrs = attrs
            .into_iter()
            .filter(|attr| validate_position(context, &attr_position, attr))
            .map(|attr| {
                let attr_kind = sp(attr.loc, attr.value.attribute_kind());
                (attr_kind, attr)
            })
            .collect::<Vec<_>>();
        attributes.extend(attrs);
    }

    let attributes = collect_modes(context, &attr_position, attributes);

    let mut attr_map = UniqueMap::new();
    for (kind, attr) in attributes {
        if no_conflicts(context, &attr_position, &attr_map, &attr) {
            insert_attribute(context, &mut attr_map, kind, attr);
        }
    }
    attr_map
}

fn validate_position(
    context: &mut Context,
    posn: &AttributePosition,
    sp!(loc, attr): &Spanned<KnownAttribute>,
) -> bool {
    let expected_positions = attr.expected_positions();
    if !expected_positions.contains(posn) {
        let msg = format!(
            "Attribute '{}' is not expected with a {}",
            attr.name(),
            posn
        );
        let all_expected = expected_positions
            .iter()
            .map(|p| format!("{}", p))
            .collect::<Vec<_>>()
            .join(", ");
        let expected_msg = format!(
            "Expected to be used with one of the following: {}",
            all_expected
        );
        context.add_diag(diag!(
            Attributes::InvalidUsage,
            (*loc, msg),
            (*loc, expected_msg)
        ));
        false
    } else {
        true
    }
}

/// Checks there are no mode conflicts, including duplicates or testing definitions. Returns the
/// provided attributes with the modes combined, and reports diagnostics for duplicates.
fn collect_modes(
    context: &mut Context,
    posn: &AttributePosition,
    attributes: Vec<(AttributeKind, Spanned<KnownAttribute>)>,
) -> Vec<(AttributeKind, Spanned<KnownAttribute>)> {
    use AttributeKind_ as K;
    let (mode_attrs, mut attributes): (Vec<(AttributeKind, Spanned<KnownAttribute>)>, _) =
        attributes
            .into_iter()
            .partition(|(kind, _)| matches!(kind.value, K::Mode));
    let mut modes: UniqueSet<Name> = UniqueSet::new();
    let mut attr_loc = None;
    for (_, mode) in mode_attrs {
        let sp!(loc, KnownAttribute::Mode(mode_attr)) = mode else {
            unreachable!()
        };
        let ModeAttribute { modes: new_modes } = mode_attr;
        for mode in new_modes {
            if let Err((_, prev_loc)) = modes.add(mode) {
                let msg = format!("{posn} annotated with duplicate mode '{mode}'");
                let prev_msg = "Previously annotated here";
                let mut diag = diag!(
                    Attributes::ValueWarning,
                    (mode.loc, msg),
                    (prev_loc, prev_msg)
                );
                let has_test_attribute = attributes
                    .iter()
                    .any(|(kind, _)| matches!(kind.value, K::Test | K::RandTest));
                // Carve-out additional note for `test` and `random-test`
                if mode.value.as_str() == ModeAttribute::TEST && has_test_attribute {
                    let msg = format!(
                        "Attributes '#[{}]' and '#[{}]' implicitly specify '#[{}({})]'",
                        A::TestingAttribute::TEST,
                        A::TestingAttribute::RAND_TEST,
                        A::ModeAttribute::MODE,
                        A::ModeAttribute::TEST
                    );
                    diag.add_note(msg);
                }
                context.add_diag(diag);
            } else {
                attr_loc.get_or_insert(loc);
            }
        }
    }

    if !modes.is_empty() {
        let loc = attr_loc.expect("Bad logic in mode resolution");
        let attr_kind = sp(loc, AttributeKind_::Mode);
        let attr = KnownAttribute::Mode(ModeAttribute { modes });
        attributes.push((attr_kind, sp(loc, attr)));
    }
    attributes
}

/// Returns true if there are no conflicting definitions, or false if there are.
/// Checks there are no conflicting definitions (though does not check for duplicates), and reports
/// diagnostics for duplicates.
/// This also ensures that the modes are compatible for testing definitions.
fn no_conflicts(
    context: &mut Context,
    posn: &AttributePosition,
    attr_map: &E::Attributes,
    sp!(attr_loc, attr): &Spanned<KnownAttribute>,
) -> bool {
    use AttributeKind_ as K;
    use KnownAttribute as KA;

    fn matching_kinds<'a, I>(attr_map: &E::Attributes, kinds: I) -> Vec<(Loc, AttributeKind_)>
    where
        I: IntoIterator<Item = &'a AttributeKind_>,
    {
        kinds
            .into_iter()
            .map(|kind| sp(Loc::invalid(), *kind))
            .filter_map(|kind| attr_map.get_loc(&kind).map(|loc| (*loc, kind.value)))
            .collect()
    }

    let conflicts: Vec<(Loc, AttributeKind_)> = match attr {
        KA::BytecodeInstruction(..)
        | KA::DefinesPrimitive(..)
        | KA::Deprecation(..)
        | KA::Diagnostic(..)
        | KA::Error(..)
        | KA::External(..)
        | KA::Mode(..)
        | KA::Syntax(..) => vec![],
        KA::Testing(test_attr) => match test_attr {
            crate::shared::known_attributes::TestingAttribute::ExpectedFailure(..) => vec![],
            crate::shared::known_attributes::TestingAttribute::Test => {
                matching_kinds(attr_map, &[K::RandTest])
            }
            crate::shared::known_attributes::TestingAttribute::RandTest => {
                matching_kinds(attr_map, &[K::Test])
            }
        },
    };
    if !conflicts.is_empty() {
        for (conflict_loc, conflict_kind) in conflicts {
            let msg = format!(
                "{posn} annotated as both #[{}] and #[{conflict_kind}]. \
                You need to declare it as either one or the other",
                attr.name(),
            );
            let prev_msg = "Previously annotated here";
            context.add_diag(diag!(
                Attributes::InvalidUsage,
                (*attr_loc, msg),
                (conflict_loc, prev_msg)
            ));
        }
        false
    } else {
        true
    }
}

fn insert_attribute(
    context: &mut Context,
    attr_map: &mut E::Attributes,
    attr_kind: AttributeKind,
    sp!(attr_loc, attr): Spanned<KnownAttribute>,
) {
    if let Some(prev_loc) = attr_map.get_loc(&attr_kind) {
        let msg = format!("Duplicate attribute '{attr_kind}' attached to the same item");
        let prev_msg = "Attribute previously given here".to_string();
        let diag = diag!(
            Attributes::Duplicate,
            (attr_loc, msg),
            (*prev_loc, prev_msg)
        );
        context.add_diag(diag);
    } else {
        attr_map
            .add(attr_kind, sp(attr_loc, attr))
            .expect("ICE: failed insert");
    }
}

fn attribute(
    context: &mut Context,
    _attr_position: AttributePosition,
    sp!(loc, attribute): P::Attribute,
) -> Option<Spanned<KnownAttribute>> {
    use KnownAttribute as KA;
    use P::Attribute_ as PA;
    let attr_ = match attribute {
        PA::BytecodeInstruction => KA::BytecodeInstruction(A::BytecodeInstructionAttribute),
        PA::DefinesPrimitive(name) => KA::DefinesPrimitive(A::DefinesPrimitiveAttribute { name }),
        PA::Deprecation { note } => {
            let note = note.and_then(|symbol| match byte_string::decode(loc, symbol.as_ref()) {
                Ok(v) => Some(v),
                Err(e) => {
                    for diag in e.into_iter() {
                        context.add_diag(diag.into_diagnostic());
                    }
                    None
                }
            });
            KA::Deprecation(A::DeprecationAttribute { note })
        }
        PA::Error { code } => {
            let code = context
                .value_opt(code)
                .and_then(|sp!(loc, code)| match code {
                    E::Value_::InferredNum(value) => {
                        let new_err_code = u8::try_from(value).ok();
                        if new_err_code.is_none() {
                            context.add_diag(diag!(
                                Attributes::InvalidValue,
                                (loc, "Error code must be a u8")
                            ));
                        }
                        new_err_code
                    }
                    E::Value_::U8(value) => Some(value),
                    _ => {
                        context.add_diag(diag!(
                            Attributes::InvalidValue,
                            (loc, "Error code must be a u8")
                        ));
                        None
                    }
                });
            KA::Error(A::ErrorAttribute { code })
        }
        PA::External { attrs } => {
            let sp!(_, attrs) = attrs;
            let attrs = attrs
                .into_iter()
                .filter_map(|attr| ext_attribue(context, attr))
                .collect();
            let attrs = unique_ext_attributes(context, attrs);
            KA::External(A::ExternalAttribute { attrs })
        }
        PA::Mode { modes } => KA::Mode(A::ModeAttribute { modes }),
        PA::Syntax { kind } => KA::Syntax(A::SyntaxAttribute { kind }),
        // -- allow --
        PA::Allow { allow_set } => KA::Diagnostic(A::DiagnosticAttribute::Allow { allow_set }),
        PA::LintAllow { allow_set } => {
            KA::Diagnostic(A::DiagnosticAttribute::LintAllow { allow_set })
        }
        // -- testing --
        PA::Test => KA::Testing(A::TestingAttribute::Test),
        PA::ExpectedFailure {
            failure_kind,
            minor_status,
            location,
        } => {
            let failure =
                expected_failure_attribute(context, &loc, failure_kind, minor_status, location)?;
            KA::Testing(TestingAttribute::ExpectedFailure(Box::new(failure)))
        }
        PA::RandomTest => KA::Testing(A::TestingAttribute::RandTest),
    };
    Some(sp(loc, attr_))
}

fn unique_ext_attributes(
    context: &mut Context,
    attrs: Vec<A::ExternalAttributeEntry>,
) -> A::ExternalAttributeEntries {
    let mut attr_map = UniqueMap::new();
    for attr in attrs {
        let loc = attr.loc;
        let sp!(nloc, name_) = attr.value.name();
        if let Err((_, old_loc)) = attr_map.add(sp(nloc, name_), attr) {
            let msg = format!("Duplicate attribute '{name_}' attached to the same item");
            context.add_diag(diag!(
                Declarations::DuplicateItem,
                (loc, msg),
                (old_loc, "Attribute previously given here"),
            ));
        }
    }
    attr_map
}

fn ext_attribue(
    context: &mut Context,
    sp!(loc, attribute_): P::ParsedAttribute,
) -> Option<A::ExternalAttributeEntry> {
    use A::ExternalAttributeEntry_ as EA;
    use P::ParsedAttribute_ as PA;

    Some(sp(
        loc,
        match attribute_ {
            PA::Name(n) => EA::Name(n),
            PA::Assigned(n, v) => EA::Assigned(n, Box::new(context.external_attribute_value(*v)?)),
            PA::Parameterized(n, sp!(_, pattrs_)) => {
                let attrs = pattrs_
                    .into_iter()
                    .map(|a| ext_attribue(context, a))
                    .collect::<Option<Vec<_>>>()?;
                EA::Parameterized(n, unique_ext_attributes(context, attrs))
            }
        },
    ))
}

fn expected_failure_attribute(
    context: &mut Context,
    attr_loc: &Loc,
    failure_kind: Box<P::ExpectedFailureKind>,
    minor_status: Option<P::AttributeValue>,
    location: Option<P::NameAccessChain>,
) -> Option<A::ExpectedFailure> {
    let sp!(failure_loc, failure_kind) = *failure_kind;
    match failure_kind {
        P::ExpectedFailureKind_::Empty => Some(A::ExpectedFailure::Expected),
        P::ExpectedFailureKind_::Name(name) => expected_failure_named(
            context,
            attr_loc,
            &failure_loc,
            name,
            minor_status,
            location,
        ),
        P::ExpectedFailureKind_::MajorStatus(value) => expected_failure_major_status(
            context,
            attr_loc,
            &failure_loc,
            value,
            minor_status,
            location,
        ),
        P::ExpectedFailureKind_::AbortCode(value) => expected_failure_abort_code(
            context,
            attr_loc,
            &failure_loc,
            value,
            minor_status,
            location,
        ),
    }
}

fn expected_failure_named(
    context: &mut Context,
    _attr_loc: &Loc,
    failure_loc: &Loc,
    sp!(name_loc, name): Name,
    minor_status: Option<P::AttributeValue>,
    location: Option<P::NameAccessChain>,
) -> Option<A::ExpectedFailure> {
    let (status_code, minor_code) = match name.as_ref() {
        TestingAttribute::ARITHMETIC_ERROR_NAME => {
            if let Some(sp!(loc, _)) = minor_status {
                context.add_diag(diag!(
                    Attributes::ValueWarning,
                    (loc, "Arithmetic errors do not support minor statuses")
                ));
            };
            (StatusCode::ARITHMETIC_ERROR, None)
        }
        TestingAttribute::OUT_OF_GAS_NAME => {
            if let Some(sp!(loc, _)) = minor_status {
                context.add_diag(diag!(
                    Attributes::ValueWarning,
                    (loc, "Out-of-gas errors do not support minor statuses")
                ));
            };
            (StatusCode::OUT_OF_GAS, None)
        }
        TestingAttribute::VECTOR_ERROR_NAME => {
            let minor_code = attribute_value_to_minor_code(context, minor_status);
            (StatusCode::VECTOR_OPERATION_ERROR, minor_code)
        }
        _ => {
            context.add_diag(ice!((
                name_loc,
                format!("'expected_failure' attribute with invalid name {name}")
            )));
            return None;
        }
    };
    let Some(location) = location else {
        let msg = format!(
            "Expected '{}' following '{name}'",
            TestingAttribute::ERROR_LOCATION
        );
        context.add_diag(diag!(Attributes::InvalidUsage, (*failure_loc, msg)));
        return None;
    };
    let location = context.name_access_chain_to_module_ident(location)?;
    Some(A::ExpectedFailure::ExpectedWithError {
        status_code,
        minor_code,
        location,
    })
}

fn expected_failure_major_status(
    context: &mut Context,
    _attr_loc: &Loc,
    failure_loc: &Loc,
    value: P::Value,
    minor_status: Option<P::AttributeValue>,
    location: Option<P::NameAccessChain>,
) -> Option<A::ExpectedFailure> {
    let value_loc = value.loc;
    let status_code = context.value(value)?;
    let status_code = value_into_u64(context, status_code)?;
    let Some(status_code) = StatusCode::try_from(status_code).ok() else {
        let bad_value = format!(
            "Invalid value for '{}'",
            TestingAttribute::MAJOR_STATUS_NAME,
        );
        let no_code = format!("No status code associated with value '{}'", status_code);
        context.add_diag(diag!(
            Attributes::InvalidValue,
            (value_loc, bad_value),
            (*failure_loc, no_code)
        ));
        return None;
    };
    let minor_code = attribute_value_to_minor_code(context, minor_status);
    let Some(location) = location else {
        let msg = format!(
            "Expected '{}' following '{}'",
            TestingAttribute::ERROR_LOCATION,
            TestingAttribute::MAJOR_STATUS_NAME
        );
        context.add_diag(diag!(Attributes::InvalidUsage, (*failure_loc, msg)));
        return None;
    };
    let location = context.name_access_chain_to_module_ident(location)?;
    Some(A::ExpectedFailure::ExpectedWithError {
        status_code,
        minor_code,
        location,
    })
}

fn expected_failure_abort_code(
    context: &mut Context,
    _attr_loc: &Loc,
    failure_loc: &Loc,
    value: P::AttributeValue,
    minor_status: Option<P::AttributeValue>,
    location: Option<P::NameAccessChain>,
) -> Option<A::ExpectedFailure> {
    const BAD_ABORT_VALUE_WARNING: &str = "WARNING: passes for an abort from any module";

    if let Some(sp!(loc, _)) = minor_status {
        context.add_diag(diag!(
            Attributes::InvalidValue,
            (loc, "Abort code does not support minor statuses")
        ));
    }

    let minor_code = attribute_value_to_minor_code(context, Some(value))?;
    let minor_code_loc = minor_code.loc;

    match minor_code.value {
        A::MinorCode_::Value(ref code) => {
            let code = *code;
            let location = location.and_then(|loc| context.name_access_chain_to_module_ident(loc));
            if let Some(location) = location {
                let attr = A::ExpectedFailure::ExpectedWithError {
                    status_code: StatusCode::ABORTED,
                    minor_code: Some(minor_code),
                    location,
                };
                Some(attr)
            } else {
                context.add_diag(diag!(
                    Attributes::ValueWarning,
                    (*failure_loc, BAD_ABORT_VALUE_WARNING),
                    (
                        minor_code_loc,
                        format!(
                            "Replace value with a constant from expected module or add '{}=...'",
                            TestingAttribute::ERROR_LOCATION
                        )
                    )
                ));
                Some(A::ExpectedFailure::ExpectedWithCodeDEPRECATED(code))
            }
        }
        A::MinorCode_::Constant(_, _) => {
            let location = location.and_then(|loc| context.name_access_chain_to_module_ident(loc));
            let location = if let Some(location) = location {
                location
            } else {
                minor_code_location(&minor_code).unwrap()
            };
            Some(A::ExpectedFailure::ExpectedWithError {
                status_code: StatusCode::ABORTED,
                minor_code: Some(minor_code),
                location,
            })
        }
    }
}

fn value_into_u64(context: &mut Context, value: E::Value) -> Option<u64> {
    match value.value {
        E::Value_::U64(n) => Some(n),
        E::Value_::InferredNum(ref n) => match u64::try_from(*n) {
            Ok(num) => Some(num),
            Err(_) => {
                context.add_diag(diag!(
                    Attributes::InvalidValue,
                    (value.loc, "Expected abort code must be a u64")
                ));
                None
            }
        },
        _ => {
            context.add_diag(diag!(
                Attributes::InvalidValue,
                (value.loc, "Expected abort code must be a u64")
            ));
            None
        }
    }
}

fn attribute_value_to_minor_code(
    context: &mut Context,
    value: Option<P::AttributeValue>,
) -> Option<A::MinorCode> {
    const ERR_MSG: &str = "Invalid value in attribute assignment";
    const EXPECTED_MSG: &str = "Expected a u64 literal or named constant";
    let Some(sp!(value_loc, value)) = value else {
        return None;
    };
    match value {
        P::AttributeValue_::Value(value) => {
            let loc = value.loc;
            let value = context.value(value)?;
            match value.value {
                E::Value_::U64(n) => Some(sp(loc, A::MinorCode_::Value(n))),
                E::Value_::InferredNum(ref n) => match u64::try_from(*n) {
                    Ok(num) => Some(sp(loc, A::MinorCode_::Value(num))),
                    Err(_) => {
                        let mut diag = diag!(Attributes::InvalidValue, (value.loc, ERR_MSG));
                        diag.add_note(EXPECTED_MSG);
                        context.add_diag(diag);
                        None
                    }
                },
                _ => {
                    let mut diag = diag!(Attributes::InvalidValue, (value.loc, ERR_MSG));
                    diag.add_note(EXPECTED_MSG);
                    context.add_diag(diag);
                    None
                }
            }
        }
        P::AttributeValue_::ModuleAccess(chain) => {
            let chain_loc = chain.loc;
            let crate::expansion::path_expander::AccessPath {
                access,
                ptys_opt,
                is_macro,
            } = context.name_access_chain_to_module_access(
                crate::expansion::path_expander::Access::Term,
                chain,
            )?;
            ice_assert!(
                context.reporter(),
                ptys_opt.is_none(),
                value_loc,
                "'attribute' with tyargs"
            );
            ice_assert!(
                context.reporter(),
                is_macro.is_none(),
                value_loc,
                "Found a 'attribute as a macro"
            );
            match access.value {
                E::ModuleAccess_::Name(_) | E::ModuleAccess_::Variant(_, _) => {
                    let mut diag = diag!(Attributes::InvalidValue, (chain_loc, ERR_MSG));
                    diag.add_note(EXPECTED_MSG);
                    context.add_diag(diag);
                    None
                }
                E::ModuleAccess_::ModuleAccess(mident, name) => {
                    Some(sp(chain_loc, A::MinorCode_::Constant(mident, name)))
                }
            }
        }
    }
}

fn minor_code_location(minor_code: &A::MinorCode) -> Option<ModuleIdent> {
    match &minor_code.value {
        A::MinorCode_::Value(_) => None,
        A::MinorCode_::Constant(mident, _) => Some(*mident),
    }
}
