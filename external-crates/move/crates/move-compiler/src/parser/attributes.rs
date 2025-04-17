// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    diag,
    parser::{
        ast::{
            self as P, Attribute, AttributeValue, AttributeValue_, Attribute_, ExpectedFailureKind,
            ExpectedFailureKind_, NameAccessChain, ParsedAttribute, ParsedAttribute_,
        },
        format_one_of,
        syntax::Context,
    },
    shared::{
        known_attributes::{
            self as KA, DEPRECATED_EXPECTED_KEYS, EXPECTED_FAILURE_EXPECTED_KEYS,
            EXPECTED_FAILURE_EXPECTED_NAMES,
        },
        Name,
    },
};

use move_ir_types::location::*;

/// Converts a parsed attribute to a known Attribute, or leaves it as an Unknown attribute.
/// Some attributes may induce a number of internal attributes for easier handling later.
pub(crate) fn to_known_attributes(
    context: &mut Context,
    attribute: ParsedAttribute,
) -> Vec<Attribute> {
    let sp!(name_loc, name) = attribute.value.name();
    match name {
        // -- bytecode instruction attr --
        KA::BytecodeInstructionAttribute::BYTECODE_INSTRUCTION => {
            parse_bytecode_instruction(context, attribute)
        }
        // -- prim definition attribute --
        KA::DefinesPrimitiveAttribute::DEFINES_PRIM => parse_defines_prim(context, attribute),
        // -- deprecation attribute ------
        KA::DeprecationAttribute::DEPRECATED => parse_deprecated(context, attribute),
        // -- diagnostic attributes ------
        KA::DiagnosticAttribute::ALLOW => parse_allow(context, attribute),
        KA::DiagnosticAttribute::LINT_ALLOW => parse_lint_allow(context, attribute),
        // -- error attribtue ------------
        KA::ErrorAttribute::ERROR => {
            let _ = context.check_feature(
                crate::editions::FeatureGate::CleverAssertions,
                attribute.loc,
            );
            parse_error(context, attribute)
        }
        // -- external attributes --------
        KA::ExternalAttribute::EXTERNAL => parse_external(context, attribute),
        // -- mode attributes ------------
        KA::TestingAttribute::TEST_ONLY => parse_test_only(context, attribute),
        KA::VerificationAttribute::VERIFY_ONLY => parse_verify_only(context, attribute),
        // -- syntax attribute -----------
        KA::SyntaxAttribute::SYNTAX => {
            let _ =
                context.check_feature(crate::editions::FeatureGate::SyntaxMethods, attribute.loc);
            parse_syntax(context, attribute)
        }
        // -- testing attributes -=-------
        KA::TestingAttribute::TEST => parse_test(context, attribute),
        KA::TestingAttribute::RAND_TEST => parse_random_test(context, attribute),
        KA::TestingAttribute::EXPECTED_FAILURE => {
            // NB: This is intended to preserve previous behavior where errors would not be
            // reported in non-test mode.
            if context.env().flags().is_testing() {
                parse_expected_failure(context, attribute)
            } else {
                vec![]
            }
        }
        ref name => {
            let msg = format!(
                "Unknown attribute '{name}'. Custom attributes must be wrapped in '{ext}', \
                e.g. '#[{ext}({name})]'",
                ext = KA::ExternalAttribute::EXTERNAL
            );
            context.add_diag(diag!(Declarations::UnknownAttribute, (name_loc, msg)));
            report_duplicate_fields(context, &attribute);
            vec![]
        }
    }
}

fn parse_bytecode_instruction(context: &mut Context, attribute: ParsedAttribute) -> Vec<Attribute> {
    use ParsedAttribute_ as PA;
    let sp!(loc, attr) = attribute;
    match attr {
        // Valid: a bare identifier is required.
        PA::Name(_) => {
            let bytecode_attr = sp(loc, Attribute_::BytecodeInstruction);
            vec![bytecode_attr]
        }
        // Invalid: assignment is not allowed here. The error message indicates that a bare attribute is expected.
        PA::Assigned(_, _) | PA::Parameterized(_, _) => {
            let msg = make_attribute_format_error(&attr, "'#[bytecode_instruction]'");
            context.add_diag(diag!(Declarations::InvalidAttribute, (loc, msg)));
            vec![]
        }
    }
}

fn parse_defines_prim(context: &mut Context, attribute: ParsedAttribute) -> Vec<Attribute> {
    use ParsedAttribute_ as PA;
    let sp!(loc, attr) = attribute;
    match attr {
        PA::Name(_) | PA::Assigned(_, _) => {
            let msg = make_attribute_format_error(
                &attr,
                &format!(
                    "parameterized attribute '#[{}(<primitive_type_name>)'",
                    KA::DefinesPrimitiveAttribute::DEFINES_PRIM
                ),
            );
            let mut diag = diag!(Declarations::InvalidAttribute, (loc, msg));
            diag.add_note(
                format!(
                    "Attribute {prim} requires the name of the primitive being defined, e.g., '#[{prim}(vector)]'",
                    prim=KA::DefinesPrimitiveAttribute::DEFINES_PRIM
                    )
            );
            context.add_diag(diag);
            vec![]
        }
        PA::Parameterized(_name, attrs) => {
            let sp!(attrs_loc, attrs) = attrs;
            if attrs.len() != 1 {
                let msg = format!(
                    "Attribute {} requires a single argument representing the primitive type name, but {} were provided.",
                    KA::DefinesPrimitiveAttribute::DEFINES_PRIM,
                    attrs.len()
                );
                context.add_diag(diag!(Declarations::InvalidAttribute, (attrs_loc, msg)));
                return vec![];
            };
            let inner_attr = attrs.into_iter().next().unwrap();
            let Some(name) = expect_name_attr(context, inner_attr) else {
                return vec![];
            };
            let prim_attr = sp(loc, Attribute_::DefinesPrimitive(name));
            vec![prim_attr]
        }
    }
}

fn parse_deprecated(context: &mut Context, attribute: ParsedAttribute) -> Vec<Attribute> {
    use ParsedAttribute_ as PA;
    const DEPRECATED_NOTE: &str = "Deprecation attributes must be written as '#[deprecated]' or '#[deprecated(note = b\"message\")]'";
    let sp!(_, attr) = attribute;
    match attr {
        PA::Name(sp!(loc, _)) => {
            let deprecated = sp(loc, Attribute_::Deprecation { note: None });
            vec![deprecated]
        }
        PA::Assigned(sp!(loc, _), _) => {
            let msg = make_attribute_format_error(
                &attr,
                &format!(
                    "'#[{dep}]' or '#[{dep}(note = b\"message\")]'",
                    dep = KA::DeprecationAttribute::DEPRECATED
                ),
            );
            let mut diag = diag!(Attributes::InvalidUsage, (loc, msg));
            diag.add_note(DEPRECATED_NOTE);
            context.add_diag(diag);
            vec![]
        }
        PA::Parameterized(sp!(name_loc, _), inner_attrs) => {
            let sp!(inner_loc, attrs) = inner_attrs;
            if attrs.len() != 1 {
                let msg = format!(
                    "Attribute {} expects exactly one argument, found {}.",
                    KA::DeprecationAttribute::DEPRECATED,
                    attrs.len()
                );
                let mut diag = diag!(Attributes::InvalidUsage, (inner_loc, msg));
                diag.add_note(DEPRECATED_NOTE);
                context.add_diag(diag);
                return vec![];
            }
            let attr = attrs.into_iter().next().unwrap();
            if let Some((_key, sp!(loc, val_))) =
                expect_assigned_attr_value(context, attr, &DEPRECATED_EXPECTED_KEYS)
            {
                debug_assert!(_key.value.as_ref() == "note");
                match val_ {
                    AttributeValue_::Value(sp!(_, P::Value_::ByteString(bs))) => {
                        let deprecated = sp(name_loc, Attribute_::Deprecation { note: Some(bs) });
                        vec![deprecated]
                    }
                    _ => {
                        let msg = "Expected bytestring".to_string();
                        let mut diag = diag!(Attributes::InvalidUsage, (loc, msg));
                        diag.add_note(DEPRECATED_NOTE);
                        context.add_diag(diag);
                        vec![]
                    }
                }
            } else {
                vec![]
            }
        }
    }
}

// TODO: VALIDATE THIS
fn parse_allow(context: &mut Context, attribute: ParsedAttribute) -> Vec<Attribute> {
    use ParsedAttribute_ as PA;

    fn parse_allow_inner(
        context: &mut Context,
        attribute: ParsedAttribute,
    ) -> Vec<(Option<Name>, Name)> {
        let sp!(loc, attr) = attribute;
        match attr {
            PA::Name(name) => vec![(None, name)],
            PA::Parameterized(prefix, sub_attrs) => {
                let sp!(_, sub_attrs) = sub_attrs;
                let mut allow_set = BTreeSet::new();
                for attr in sub_attrs {
                    let Some(name) = expect_name_attr(context, attr) else {
                        continue;
                    };
                    let pair = (Some(prefix), name);
                    if let Some((_, prev)) = allow_set.get(&pair) {
                        let msg = format!("Duplicate lint '{}'", name);
                        context.add_diag(diag!(
                            Declarations::InvalidAttribute,
                            (name.loc, msg),
                            (prev.loc, "Lint first appears here"),
                        ));
                    } else {
                        let _ = allow_set.insert(pair);
                    }
                }
                allow_set.into_iter().collect()
            }
            attr @ PA::Assigned(_, _) => {
                let msg = make_attribute_format_error(
                    &attr,
                    "a stand alone warning filter identifier, e.g. '#[allow(unused)]'",
                );
                context.add_diag(diag!(Declarations::InvalidAttribute, (loc, msg)));
                vec![]
            }
        }
    }

    let sp!(loc, attr) = attribute;
    match attr {
        PA::Parameterized(_, inner_attrs) => {
            let sp!(_, inner_attrs) = inner_attrs;
            let mut allow_set = BTreeSet::new();
            for inner_attr in inner_attrs.into_iter() {
                let new_attrs = parse_allow_inner(context, inner_attr);
                for pair @ (_prefix, name) in new_attrs {
                    if let Some((_, prev)) = allow_set.get(&pair) {
                        let msg = format!("Duplicate lint '{}'", name);
                        context.add_diag(diag!(
                            Declarations::InvalidAttribute,
                            (name.loc, msg),
                            (prev.loc, "Lint first appears here"),
                        ));
                    } else {
                        let _ = allow_set.insert(pair);
                    }
                }
            }
            let diagnostic = sp(loc, Attribute_::Allow { allow_set });
            vec![diagnostic]
        }
        PA::Name(_) | PA::Assigned(_, _) => {
            let msg = make_attribute_format_error(
                &attr,
                &format!(
                    "parameterized attribute as '#[{}(<warning_name_1>, <warning_name_2>, ...)]'",
                    KA::DiagnosticAttribute::ALLOW
                ),
            );
            context.add_diag(diag!(Attributes::ValueWarning, (loc, msg)));
            vec![]
        }
    }
}

fn parse_lint_allow(context: &mut Context, attribute: ParsedAttribute) -> Vec<Attribute> {
    use ParsedAttribute_ as PA;
    let sp!(loc, attr) = attribute;
    match attr {
        PA::Parameterized(name, inner_attrs) => {
            let _prefix_loc = name.loc;
            let sp!(_, lint_attrs) = inner_attrs;
            let mut allow_set = BTreeSet::new();
            for lint_attr in lint_attrs.into_iter() {
                let attr_loc = lint_attr.loc;
                if let Some(lint_name) = expect_name_attr(context, lint_attr) {
                    if !allow_set.insert(lint_name) {
                        let msg = format!("Duplicate lint '{}'", lint_name);
                        context.add_diag(diag!(Declarations::InvalidAttribute, (attr_loc, msg)));
                    }
                }
            }
            let diagnostic = sp(loc, Attribute_::LintAllow { allow_set });
            vec![diagnostic]
        }
        PA::Name(_) | PA::Assigned(_, _) => {
            let msg = make_attribute_format_error(
                &attr,
                "parameterized attribute as '#[lint_allow(<lint_1>, <lint_2>, ...)]'",
            );
            context.add_diag(diag!(Attributes::ValueWarning, (loc, msg)));
            vec![]
        }
    }
}

fn parse_error(context: &mut Context, attribute: ParsedAttribute) -> Vec<Attribute> {
    use ParsedAttribute_ as PA;
    let sp!(loc, attr) = attribute;
    match attr {
        // Bare form: #[error]
        PA::Name(_) => {
            let error_attr = sp(loc, Attribute_::Error { code: None });
            vec![error_attr]
        }
        // Parameterized form: #[error(code = <value>)]
        PA::Parameterized(_name, inner_attrs) => {
            let sp!(inner_loc, inner_list) = inner_attrs;
            if inner_list.len() != 1 {
                let msg = format!(
                    "Attribute {} requires exactly one argument representing the error code, but {} were provided.",
                    KA::ErrorAttribute::ERROR,
                    inner_list.len()
                );
                context.add_diag(diag!(Declarations::InvalidAttribute, (inner_loc, msg)));
                return vec![];
            }
            let inner_attr = inner_list.into_iter().next().unwrap();
            if let Some((key, code_attr)) =
                expect_assigned_attr_value(context, inner_attr, &KA::ERROR_EXPECTED_KEYS)
            {
                debug_assert!(key.value.as_ref() == "code");
                match code_attr.value {
                    AttributeValue_::Value(val) => {
                        let error_attr = sp(loc, Attribute_::Error { code: Some(val) });
                        vec![error_attr]
                    }
                    AttributeValue_::ModuleAccess(_) => {
                        let msg = "Error attribute field 'code' must be a u8, not a module access."
                            .to_string();
                        context
                            .add_diag(diag!(Declarations::InvalidAttribute, (code_attr.loc, msg)));
                        vec![]
                    }
                }
            } else {
                vec![]
            }
        }
        // Assignment at the top level is not supported.
        PA::Assigned(_, _) => {
            let msg =
                make_attribute_format_error(&attr, "either '#[error]' or '#[error(code = <u8>)]'");
            context.add_diag(diag!(Declarations::InvalidAttribute, (loc, msg)));
            vec![]
        }
    }
}

fn parse_external(context: &mut Context, attribute: ParsedAttribute) -> Vec<Attribute> {
    use ParsedAttribute_ as PA;
    let sp!(loc, attr) = attribute;
    match attr {
        PA::Parameterized(_, attrs) => {
            vec![sp(loc, Attribute_::External { attrs })]
        }
        PA::Name(_) | PA::Assigned(_, _) => {
            let msg = make_attribute_format_error(
                &attr,
                "parameterized attribute as '#[ext(<external_attribute>)]'",
            );
            context.add_diag(diag!(Declarations::InvalidAttribute, (loc, msg)));
            vec![]
        }
    }
}

fn parse_test_only(context: &mut Context, attribute: ParsedAttribute) -> Vec<Attribute> {
    use ParsedAttribute_ as PA;
    let sp!(loc, attr) = attribute;
    match attr {
        // Valid: a bare identifier is required.
        PA::Name(_) => {
            let test_only_attr = sp(loc, Attribute_::TestOnly);
            vec![test_only_attr]
        }
        // Invalid: any assignment or parameterized use is not allowed.
        PA::Assigned(_, _) | PA::Parameterized(_, _) => {
            let msg = make_attribute_format_error(
                &attr,
                &format!("'#[{}]' with no arguments", KA::TestingAttribute::TEST_ONLY),
            );
            context.add_diag(diag!(Declarations::InvalidAttribute, (loc, msg)));
            vec![]
        }
    }
}

fn parse_verify_only(context: &mut Context, attribute: ParsedAttribute) -> Vec<Attribute> {
    use ParsedAttribute_ as PA;
    let sp!(loc, attr) = attribute;
    match attr {
        // Valid: a bare identifier is required.
        PA::Name(_) => {
            let verify_only_attr = sp(loc, Attribute_::VerifyOnly);
            vec![verify_only_attr]
        }
        // Invalid: any assignment or parameterized use is not allowed.
        PA::Assigned(_, _) | PA::Parameterized(_, _) => {
            let msg = make_attribute_format_error(
                &attr,
                &format!(
                    "'#[{}]' with no arguments",
                    KA::VerificationAttribute::VERIFY_ONLY
                ),
            );
            context.add_diag(diag!(Declarations::InvalidAttribute, (loc, msg)));
            vec![]
        }
    }
}

fn parse_test(context: &mut Context, attribute: ParsedAttribute) -> Vec<Attribute> {
    use ParsedAttribute_ as PA;
    let sp!(loc, attr) = attribute;
    match attr {
        // Valid: a bare identifier is required.
        PA::Name(_) => {
            let test_attr = sp(loc, Attribute_::Test);
            vec![test_attr]
        }
        PA::Parameterized(_, sp!(attr_loc, _)) => {
            let msg = format!(
                "Arguments are no longer supported in `#[{}]` attributes",
                KA::TestingAttribute::TEST
            );
            context.add_diag(diag!(
                Attributes::ValueWarning,
                (loc, msg),
                (attr_loc, "Ignoring these arguments")
            ));
            let test_attr = sp(loc, Attribute_::Test);
            vec![test_attr]
        }
        // Invalid: any assignment or parameterized use is not allowed.
        PA::Assigned(_, _) => {
            let msg = make_attribute_format_error(
                &attr,
                &format!(
                    "'#[{test}]' with no arguments",
                    test = KA::TestingAttribute::TEST
                ),
            );
            context.add_diag(diag!(Declarations::InvalidAttribute, (loc, msg)));
            vec![]
        }
    }
}

fn parse_random_test(context: &mut Context, attribute: ParsedAttribute) -> Vec<Attribute> {
    use ParsedAttribute_ as PA;
    let sp!(loc, attr) = attribute;
    match attr {
        // Valid: a bare identifier is required.
        PA::Name(_) => {
            let test_attr = sp(loc, Attribute_::RandomTest);
            vec![test_attr]
        }
        // Invalid: any assignment or parameterized use is not allowed.
        PA::Assigned(_, _) | PA::Parameterized(_, _) => {
            let msg = make_attribute_format_error(
                &attr,
                &format!("'#[{}]' with no arguments", KA::TestingAttribute::RAND_TEST),
            );
            context.add_diag(diag!(Declarations::InvalidAttribute, (loc, msg)));
            vec![]
        }
    }
}

fn parse_expected_failure(context: &mut Context, attribute: ParsedAttribute) -> Vec<Attribute> {
    use ParsedAttribute_ as PA;

    let sp!(outer_loc, attr) = attribute;
    // expected_failure must be parameterized
    let inner_args = match attr {
        PA::Name(sp!(name_loc, _)) => {
            return vec![sp(
                outer_loc,
                Attribute_::ExpectedFailure {
                    failure_kind: Box::new(sp(name_loc, ExpectedFailureKind_::Empty)),
                    minor_status: None,
                    location: None,
                },
            )]
        }
        PA::Parameterized(_, inner) => inner,
        PA::Assigned(_, _) => {
            let msg = make_attribute_format_error(
                &attr,
                &format!(
                    "either '#[{fail}]' or #[{fail}(<arg>, ...)]'",
                    fail = KA::TestingAttribute::EXPECTED_FAILURE,
                ),
            );
            context.add_diag(diag!(Declarations::InvalidAttribute, (outer_loc, msg)));
            return vec![];
        }
    };

    let sp!(_inner_loc, args) = inner_args;

    // Initialize fields of ExpectedFailure with default values.
    let mut failure_kind: Option<ExpectedFailureKind> = None;
    let mut minor_status: Option<AttributeValue> = None;
    let mut location_field: Option<NameAccessChain> = None;

    macro_rules! check_failure_kind_unset {
        ($arg_loc:expr) => {
            if let Some(kind) = &failure_kind {
                let msg = format!("Second failure kind given for expected failure");
                let prev_msg = format!("Previously defiend here");
                context.add_diag(diag!(
                    Declarations::InvalidAttribute,
                    ($arg_loc.clone(), msg),
                    (kind.loc.clone(), prev_msg)
                ));
                continue;
            }
        };
    }

    // Record if there is _any_ failure kind, even mis-formatted, to avoid reporting an unnecessary
    // error later;
    let field_names = args
        .iter()
        .map(|sp!(_, attr)| attr.name().value)
        .collect::<BTreeSet<_>>();
    let any_failure_kind = KA::TestingAttribute::expected_failure_cases()
        .iter()
        .any(|name| field_names.contains(name));

    let mut assigned_fields: BTreeSet<Name> = BTreeSet::new();
    for sp!(arg_loc, arg_value) in args {
        match arg_value {
            // Bare name: expected to be an error kind.
            PA::Name(name) => {
                if !EXPECTED_FAILURE_EXPECTED_NAMES.contains(name.value.as_str()) {
                    let msg = format!(
                        "Unused attribute for '{}'",
                        KA::TestingAttribute::EXPECTED_FAILURE
                    );
                    context.add_diag(diag!(Attributes::ValueWarning, (name.loc, msg)));
                    continue;
                };
                check_failure_kind_unset!(arg_loc);
                failure_kind = Some(sp(arg_loc, ExpectedFailureKind_::Name(name)));
            }
            // Assignment form: expected to be one of the allowed keys.
            PA::Assigned(_, _) => {
                if let Some((key, value)) = warn_expect_assigned_attr_value(
                    context,
                    KA::TestingAttribute::EXPECTED_FAILURE,
                    sp(arg_loc, arg_value),
                    &EXPECTED_FAILURE_EXPECTED_KEYS,
                ) {
                    // Check for duplicates in assigned fields.
                    if let Some(prev) = assigned_fields.get(&key) {
                        let msg = format!("Duplicate assignment for field '{}'.", key);
                        context.add_diag(diag!(
                            Declarations::InvalidAttribute,
                            (arg_loc, msg),
                            (prev.loc, "Previously defined here"),
                        ));
                        continue;
                    } else {
                        let _ = assigned_fields.insert(key);
                    }
                    let err_msg =
                        |expected| format!("Field '{}' must be a {}", key.value, expected);
                    match key.value.as_str() {
                        "abort_code" => {
                            check_failure_kind_unset!(arg_loc);
                            failure_kind =
                                Some(sp(arg_loc, ExpectedFailureKind_::AbortCode(value)));
                        }
                        "major_status" => match value.value {
                            AttributeValue_::Value(v) => {
                                check_failure_kind_unset!(arg_loc);
                                failure_kind =
                                    Some(sp(arg_loc, ExpectedFailureKind_::MajorStatus(v)));
                            }
                            AttributeValue_::ModuleAccess(_) => {
                                context.add_diag(diag!(
                                    Declarations::InvalidAttribute,
                                    (arg_loc, err_msg("literal value"))
                                ));
                            }
                        },
                        "minor_status" => minor_status = Some(value),
                        "location" => match value.value {
                            AttributeValue_::ModuleAccess(nac) => location_field = Some(nac),
                            AttributeValue_::Value(_) => {
                                context.add_diag(diag!(
                                    Declarations::InvalidAttribute,
                                    (arg_loc, err_msg("module name"))
                                ));
                                // Bail early
                                return vec![];
                            }
                        },
                        _ => {} // Should not happen due to allowed_keys filtering.
                    }
                }
            }
            // Parameterized form is not allowed here.
            PA::Parameterized(_, _) => {
                let msg = make_attribute_format_error(
                    &arg_value,
                    &format!(
                        "expected a failure kind or an assignment (e.g. '{} = <value>')",
                        KA::TestingAttribute::ABORT_CODE_NAME
                    ),
                );
                context.add_diag(diag!(Declarations::InvalidAttribute, (arg_loc, msg)));
            }
        }
    }
    if let Some(failure_kind) = failure_kind {
        let expected_failure_attr = sp(
            outer_loc,
            Attribute_::ExpectedFailure {
                failure_kind: Box::new(failure_kind),
                minor_status,
                location: location_field,
            },
        );
        vec![expected_failure_attr]
    } else {
        if !any_failure_kind {
            let msg = format!(
                "Invalid '#[expected_failure(...)]' attribute, no failure kind found. Expected {}",
                format_one_of(KA::TestingAttribute::expected_failure_cases())
            );
            context.add_diag(diag!(Attributes::InvalidValue, (outer_loc, msg)));
        }
        vec![]
    }
}

fn parse_syntax(context: &mut Context, attribute: ParsedAttribute) -> Vec<Attribute> {
    use ParsedAttribute_ as PA;
    let sp!(loc, attr) = attribute;
    match attr {
        // The syntax attribute must be parameterized.
        PA::Name(_) | PA::Assigned(_, _) => {
            let msg = make_attribute_format_error(
                &attr,
                "parameterized attribute as '#[syntax(<kind>)]'",
            );
            context.add_diag(diag!(Declarations::InvalidAttribute, (loc, msg)));
            vec![]
        }
        PA::Parameterized(_name, inner_attrs) => {
            let sp!(inner_loc, attrs) = inner_attrs;
            if attrs.len() != 1 {
                let msg = format!(
                    "Attribute {} expects exactly one argument, found {}.",
                    KA::SyntaxAttribute::SYNTAX,
                    attrs.len()
                );
                context.add_diag(diag!(Declarations::InvalidAttribute, (inner_loc, msg)));
                return vec![];
            }
            let inner_attr = attrs.into_iter().next().unwrap();
            if let Some(kind) = expect_name_attr(context, inner_attr) {
                let syntax_attr = sp(loc, Attribute_::Syntax { kind });
                vec![syntax_attr]
            } else {
                vec![]
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------------------------------------

fn report_duplicate_fields(context: &mut Context, attr: &ParsedAttribute) {
    use ParsedAttribute_ as PA;
    let sp!(_loc, parsed) = attr;
    if let PA::Parameterized(_, sp!(_, subattrs)) = parsed {
        // Track first occurrence of each sub-attribute name.
        let mut seen: BTreeMap<String, Loc> = BTreeMap::new();
        for sub in subattrs {
            // Extract the name of this sub-attribute.
            let name_str = match &sub.value {
                PA::Name(n) => n.value.to_string(),
                PA::Assigned(n, _) => n.value.to_string(),
                PA::Parameterized(n, _) => n.value.to_string(),
            };
            let this_loc = sub.loc;
            if let Some(prev_loc) = seen.get(&name_str) {
                // Duplicate found: report it.
                let msg = format!(
                    "Duplicate attribute '{}' attached to the same item",
                    name_str
                );
                context.add_diag(diag!(
                    Declarations::DuplicateItem,
                    (this_loc, msg),
                    (*prev_loc, "Attribute previously given here")
                ));
            } else {
                seen.insert(name_str, this_loc);
            }
        }
        // Recurse into each sub-attribute in case of nested parameterized attributes.
        for sub in subattrs {
            report_duplicate_fields(context, sub);
        }
    }
}

// -----------------------------------------------
// Sub-attribute parsing

fn expect_name_attr(context: &mut Context, attr: ParsedAttribute) -> Option<Name> {
    use ParsedAttribute_ as PA;
    let sp!(loc, attr) = attr;
    match &attr {
        PA::Name(name) => Some(*name),
        PA::Assigned(name, _) | PA::Parameterized(name, _) => {
            let msg =
                make_attribute_format_error(&attr, &format!("name only, as '{}'", name.clone()));
            context.add_diag(diag!(Declarations::InvalidAttribute, (loc, msg)));
            None
        }
    }
}

fn expect_assigned_attr_value(
    context: &mut Context,
    attr: ParsedAttribute,
    expected: &BTreeSet<String>,
) -> Option<(Name, AttributeValue)> {
    use ParsedAttribute_ as PA;
    let sp!(loc, attr) = attr;
    match attr {
        PA::Assigned(key, value) if expected.contains(key.value.as_ref()) => Some((key, *value)),
        PA::Name(key) | PA::Parameterized(key, _) if expected.contains(key.value.as_ref()) => {
            let name = attr_name(&attr);
            let msg = make_attribute_format_error(
                &attr,
                &format!("an assignment, e.g. '{} = <value>'", name),
            );
            context.add_diag(diag!(Declarations::InvalidAttribute, (loc, msg)));
            None
        }
        PA::Name(key) | PA::Assigned(key, _) | PA::Parameterized(key, _) => {
            let msg = format!(
                "Unexpected field '{}' -- expected {}",
                key.value.as_ref(),
                format_one_of(expected)
            );
            context.add_diag(diag!(Declarations::InvalidAttribute, (key.loc, msg)));
            None
        }
    }
}

fn warn_expect_assigned_attr_value(
    context: &mut Context,
    parent_attr_name: &str,
    attr: ParsedAttribute,
    expected: &BTreeSet<String>,
) -> Option<(Name, AttributeValue)> {
    use ParsedAttribute_ as PA;
    let sp!(loc, attr) = attr;
    match attr {
        PA::Assigned(key, value) if expected.contains(key.value.as_ref()) => Some((key, *value)),
        _ => {
            let msg = format!("Unused attribute for '{parent_attr_name}'");
            context.add_diag(diag!(Attributes::ValueWarning, (loc, msg)));
            None
        }
    }
}

// -----------------------------------------------
// Error Helpers

/// Generates a standardized error message for attribute formatting issues.
///
/// * `current_attr` - the current parsed attribute form (e.g., PA::Assigned, PA::Parameterized).
/// * `expected`     - the syntax the attribute is expected to adhere to.
fn make_attribute_format_error(current_attr: &ParsedAttribute_, expectation: &str) -> String {
    use ParsedAttribute_ as PA;
    // Extract the attribute's name.
    let name = match current_attr {
        PA::Name(n) => n,
        PA::Assigned(n, _) => n,
        PA::Parameterized(n, _) => n,
    };

    // Describe the usage that was encountered.
    let encountered = usage_kind(current_attr);

    format!(
        "Attribute '{}' does not support {}. Expected {}",
        name, encountered, expectation
    )
}

fn attr_name(attr: &ParsedAttribute_) -> &str {
    match attr {
        ParsedAttribute_::Name(name)
        | ParsedAttribute_::Assigned(name, _)
        | ParsedAttribute_::Parameterized(name, _) => name.value.as_ref(),
    }
}

fn usage_kind(attr: &ParsedAttribute_) -> &'static str {
    use ParsedAttribute_ as PA;
    match &attr {
        PA::Name(_) => "name-only usage",
        PA::Assigned(_, _) => "assignment",
        PA::Parameterized(_, _) => "parameters",
    }
}
