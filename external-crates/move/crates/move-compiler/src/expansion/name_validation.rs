// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    diagnostics::{Diagnostic, DiagnosticReporter},
    parser::ast::{self as P, ModuleName, Var, MACRO_MODIFIER},
    shared::*,
};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::collections::BTreeSet;

// Implicit aliases for the Move Stdlib:
// use std::vector;
// use std::option::{Self, Option};
pub const IMPLICIT_STD_MODULES: &[Symbol] = &[symbol!("option"), symbol!("vector")];
pub const IMPLICIT_STD_MEMBERS: &[(Symbol, Symbol, ModuleMemberKind)] = &[(
    symbol!("option"),
    symbol!("Option"),
    ModuleMemberKind::Struct,
)];

// Implicit aliases for Sui mode:
// use sui::object::{Self, ID, UID};
// use sui::transfer;
// use sui::tx_context::{Self, TxContext};
pub const IMPLICIT_SUI_MODULES: &[Symbol] = &[
    symbol!("object"),
    symbol!("transfer"),
    symbol!("tx_context"),
];
pub const IMPLICIT_SUI_MEMBERS: &[(Symbol, Symbol, ModuleMemberKind)] = &[
    (symbol!("object"), symbol!("ID"), ModuleMemberKind::Struct),
    (symbol!("object"), symbol!("UID"), ModuleMemberKind::Struct),
    (
        symbol!("tx_context"),
        symbol!("TxContext"),
        ModuleMemberKind::Struct,
    ),
];

#[derive(Copy, Clone, Debug)]
pub enum ModuleMemberKind {
    Constant,
    Function,
    Struct,
    Enum,
}

#[derive(Copy, Clone, Debug)]
pub enum NameCase {
    Constant,
    Function,
    Struct,
    Enum,
    Module,
    ModuleMemberAlias(ModuleMemberKind),
    ModuleAlias,
    Variable,
    Address,
    TypeParameter,
}

//**************************************************************************************************
// impls
//**************************************************************************************************

impl ModuleMemberKind {
    pub fn case(self) -> NameCase {
        match self {
            ModuleMemberKind::Constant => NameCase::Constant,
            ModuleMemberKind::Function => NameCase::Function,
            ModuleMemberKind::Struct => NameCase::Struct,
            ModuleMemberKind::Enum => NameCase::Enum,
        }
    }
}

impl NameCase {
    pub const fn name(&self) -> &'static str {
        match self {
            NameCase::Constant => "constant",
            NameCase::Function => "function",
            NameCase::Struct => "struct",
            NameCase::Enum => "enum",
            NameCase::Module => "module",
            NameCase::ModuleMemberAlias(ModuleMemberKind::Function) => "function alias",
            NameCase::ModuleMemberAlias(ModuleMemberKind::Constant) => "constant alias",
            NameCase::ModuleMemberAlias(ModuleMemberKind::Struct) => "struct alias",
            NameCase::ModuleMemberAlias(ModuleMemberKind::Enum) => "enum alias",
            NameCase::ModuleAlias => "module alias",
            NameCase::Variable => "variable",
            NameCase::Address => "address",
            NameCase::TypeParameter => "type parameter",
        }
    }
}

//**************************************************************************************************
// Valid names
//**************************************************************************************************

#[allow(clippy::result_unit_err)]
pub fn check_valid_address_name(
    reporter: &DiagnosticReporter,
    sp!(_, ln_): &P::LeadingNameAccess,
) -> Result<(), ()> {
    use P::LeadingNameAccess_ as LN;
    match ln_ {
        LN::AnonymousAddress(_) => Ok(()),
        LN::GlobalAddress(n) | LN::Name(n) => {
            check_restricted_name_all_cases(reporter, NameCase::Address, n)
        }
    }
}

pub fn valid_local_variable_name(s: Symbol) -> bool {
    s.starts_with('_') || s.starts_with(|c: char| c.is_ascii_lowercase())
}

#[allow(clippy::result_unit_err)]
pub fn check_valid_function_parameter_name(
    reporter: &DiagnosticReporter,
    is_macro: Option<Loc>,
    v: &Var,
) {
    const SYNTAX_IDENTIFIER_NOTE: &str =
        "'macro' parameters start with '$' to indicate that their arguments are not evaluated \
        before the macro is expanded, meaning the entire expression is substituted. \
        This is different from regular function parameters that are evaluated before the \
        function is called.";
    let is_syntax_identifier = v.is_syntax_identifier();
    if let Some(macro_loc) = is_macro {
        if !is_syntax_identifier && !v.is_underscore() {
            let msg = format!(
                "Invalid parameter name '{}'. '{}' parameter names must start with '$' (or must be '_')",
                v, MACRO_MODIFIER,
            );
            let macro_msg = format!("Declared '{}' here", MACRO_MODIFIER);
            let mut diag = diag!(
                Declarations::InvalidName,
                (v.loc(), msg),
                (macro_loc, macro_msg),
            );
            diag.add_note(SYNTAX_IDENTIFIER_NOTE);
            reporter.add_diag(diag);
        }
    } else if is_syntax_identifier {
        let msg = format!(
            "Invalid parameter name '{}'. Non-'{}' parameter names cannot start with '$'",
            v, MACRO_MODIFIER,
        );
        let mut diag = diag!(Declarations::InvalidName, (v.loc(), msg));
        diag.add_note(SYNTAX_IDENTIFIER_NOTE);
        reporter.add_diag(diag);
    } else if !is_valid_local_variable_name(v.value()) {
        let msg = format!(
            "Invalid parameter name '{}'. Local variable names must start with 'a'..'z', '_', \
            or be a valid name quoted with backticks (`name`)",
            v,
        );
        reporter.add_diag(diag!(Declarations::InvalidName, (v.loc(), msg)));
    }
    let _ = check_restricted_name_all_cases(reporter, NameCase::Variable, &v.0);
}

pub fn check_valid_local_name(reporter: &DiagnosticReporter, v: &Var) {
    if !is_valid_local_variable_name(v.value()) {
        let msg = format!(
            "Invalid local name '{}'. Local variable names must start with 'a'..'z', '_', \
            or be a valid name quoted with backticks (`name`)",
            v,
        );
        reporter.add_diag(diag!(Declarations::InvalidName, (v.loc(), msg)));
    }
    let _ = check_restricted_name_all_cases(reporter, NameCase::Variable, &v.0);
}

fn is_valid_local_variable_name(s: Symbol) -> bool {
    Var::is_valid_name(s) && !Var::is_syntax_identifier_name(s)
}

pub fn check_valid_module_member_name(
    reporter: &DiagnosticReporter,
    member: ModuleMemberKind,
    name: Name,
) -> Option<Name> {
    match check_valid_module_member_name_impl(reporter, member, &name, member.case()) {
        Err(()) => None,
        Ok(()) => Some(name),
    }
}

pub fn check_valid_module_member_alias(
    reporter: &DiagnosticReporter,
    member: ModuleMemberKind,
    alias: Name,
) -> Option<Name> {
    match check_valid_module_member_name_impl(
        reporter,
        member,
        &alias,
        NameCase::ModuleMemberAlias(member),
    ) {
        Err(()) => None,
        Ok(()) => Some(alias),
    }
}

fn check_valid_module_member_name_impl(
    reporter: &DiagnosticReporter,
    member: ModuleMemberKind,
    n: &Name,
    case: NameCase,
) -> Result<(), ()> {
    use ModuleMemberKind as M;
    fn upper_first_letter(s: &str) -> String {
        let mut chars = s.chars();
        match chars.next() {
            None => String::new(),
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        }
    }
    match member {
        M::Function => {
            if n.value.starts_with('_') {
                let msg = format!(
                    "Invalid {} name '{}'. {} names cannot start with '_'",
                    case.name(),
                    n,
                    upper_first_letter(case.name()),
                );
                reporter.add_diag(diag!(Declarations::InvalidName, (n.loc, msg)));
                return Err(());
            }
        }
        M::Constant | M::Struct | M::Enum => {
            if !is_valid_datatype_or_constant_name(&n.value) {
                let msg = format!(
                    "Invalid {} name '{}'. {} names must start with 'A'..'Z'",
                    case.name(),
                    n,
                    upper_first_letter(case.name()),
                );
                reporter.add_diag(diag!(Declarations::InvalidName, (n.loc, msg)));
                return Err(());
            }
        }
    }

    // TODO move these names to a more central place?
    check_restricted_names(
        reporter,
        case,
        n,
        crate::naming::ast::BuiltinFunction_::all_names(),
    )?;
    check_restricted_names(
        reporter,
        case,
        n,
        crate::naming::ast::BuiltinTypeName_::all_names(),
    )?;

    // Restricting Self for now in the case where we ever have impls
    // Otherwise, we could allow it
    check_restricted_name_all_cases(reporter, case, n)?;

    Ok(())
}

#[allow(clippy::result_unit_err)]
pub fn check_valid_type_parameter_name(
    reporter: &DiagnosticReporter,
    is_macro: Option<Loc>,
    n: &Name,
) -> Result<(), ()> {
    // TODO move these names to a more central place?
    if n.value == symbol!("_") {
        let diag = restricted_name_error(NameCase::TypeParameter, n.loc, "_");
        reporter.add_diag(diag);
        return Err(());
    }

    const SYNTAX_IDENTIFIER_NOTE: &str = "Type parameter names starting with '$' indicate that \
        their arguments do not have to satisfy certain constraints before the macro is expanded, \
        meaning types like '&mut u64' or '(bool, u8)' may be used as arguments.";

    let is_syntax_ident = Var::is_syntax_identifier_name(n.value);
    if let Some(macro_loc) = is_macro {
        if !is_syntax_ident {
            let msg = format!(
                "Invalid type parameter name. \
                '{} fun' type parameter names must start with '$'",
                MACRO_MODIFIER
            );
            let macro_msg = format!("Declared '{}' here", MACRO_MODIFIER);
            let mut diag = diag!(
                Declarations::InvalidName,
                (n.loc, msg),
                (macro_loc, macro_msg),
            );
            diag.add_note(SYNTAX_IDENTIFIER_NOTE);
            reporter.add_diag(diag);
        } else {
            let next_char = n.value.chars().nth(1).unwrap();
            if !next_char.is_ascii_alphabetic() {
                let msg = format!(
                    "Invalid type parameter name '{}'. \
                    Following the '$', the '{} fun' type parameter must be have a valid type \
                    parameter name starting with a letter 'a'..'z' or 'A'..'Z'",
                    n, MACRO_MODIFIER
                );
                let mut diag = diag!(Declarations::InvalidName, (n.loc, msg));
                diag.add_note(SYNTAX_IDENTIFIER_NOTE);
                reporter.add_diag(diag);
            }
        }
    } else if is_syntax_ident {
        let msg = format!(
            "Invalid type parameter name. \
                Only '{} fun' type parameter names cat start with '$'",
            MACRO_MODIFIER
        );
        let mut diag = diag!(Declarations::InvalidName, (n.loc, msg));
        diag.add_note(SYNTAX_IDENTIFIER_NOTE);
        reporter.add_diag(diag);
    }

    // TODO move these names to a more central place?
    check_restricted_names(
        reporter,
        NameCase::TypeParameter,
        n,
        crate::naming::ast::BuiltinFunction_::all_names(),
    )?;
    check_restricted_names(
        reporter,
        NameCase::TypeParameter,
        n,
        crate::naming::ast::BuiltinTypeName_::all_names(),
    )?;

    check_restricted_name_all_cases(reporter, NameCase::TypeParameter, n)
}

pub fn is_valid_datatype_or_constant_name(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_uppercase())
}

#[allow(clippy::result_unit_err)]
// Checks for a restricted name in any decl case
// Self and vector are not allowed
pub fn check_restricted_name_all_cases(
    reporter: &DiagnosticReporter,
    case: NameCase,
    n: &Name,
) -> Result<(), ()> {
    match case {
        NameCase::Constant
        | NameCase::Function
        | NameCase::Struct
        | NameCase::Enum
        | NameCase::Module
        | NameCase::ModuleMemberAlias(_)
        | NameCase::ModuleAlias
        | NameCase::Address => {
            if Var::is_syntax_identifier_name(n.value) {
                let msg = format!(
                    "Invalid {} name '{}'. Identifiers starting with '$' can be used only for \
                    parameters and type paramters",
                    case.name(),
                    n,
                );
                reporter.add_diag(diag!(Declarations::InvalidName, (n.loc, msg)));
                return Err(());
            }
        }
        NameCase::Variable | NameCase::TypeParameter => (),
    }

    let n_str = n.value.as_str();
    let can_be_vector = matches!(case, NameCase::Module | NameCase::ModuleAlias);
    if n_str == ModuleName::SELF_NAME
        || (!can_be_vector && n_str == crate::naming::ast::BuiltinTypeName_::VECTOR)
    {
        reporter.add_diag(restricted_name_error(case, n.loc, n_str));
        Err(())
    } else {
        Ok(())
    }
}

fn check_restricted_names(
    reporter: &DiagnosticReporter,
    case: NameCase,
    sp!(loc, n_): &Name,
    all_names: &BTreeSet<Symbol>,
) -> Result<(), ()> {
    if all_names.contains(n_) {
        reporter.add_diag(restricted_name_error(case, *loc, n_));
        Err(())
    } else {
        Ok(())
    }
}

fn restricted_name_error(case: NameCase, loc: Loc, restricted: &str) -> Diagnostic {
    let a_or_an = match case.name().chars().next().unwrap() {
        // TODO this is not exhaustive to the indefinite article rules in English
        // but 'case' is never user generated, so it should be okay for a while/forever...
        'a' | 'e' | 'i' | 'o' | 'u' => "an",
        _ => "a",
    };
    let msg = format!(
        "Invalid {case} name '{restricted}'. '{restricted}' is restricted and cannot be used to \
         name {a_or_an} {case}",
        a_or_an = a_or_an,
        case = case.name(),
        restricted = restricted,
    );
    diag!(NameResolution::ReservedName, (loc, msg))
}
