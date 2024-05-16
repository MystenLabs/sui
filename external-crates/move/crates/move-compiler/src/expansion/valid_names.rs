use std::collections::BTreeSet;

use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;

use crate::diagnostics::{codes, Diagnostic};
use crate::parser::ast::{self as P, MACRO_MODIFIER};
use crate::{diag, ice};

use crate::expansion::translate::Context;
use crate::shared::{CompilationEnv, Name};

//**************************************************************************************************
// Valid names
//**************************************************************************************************

pub fn check_valid_address_name(
    env: &mut CompilationEnv,
    sp!(_, ln_): &P::LeadingNameAccess,
) -> Result<(), ()> {
    use P::LeadingNameAccess_ as LN;
    match ln_ {
        LN::AnonymousAddress(_) => Ok(()),
        LN::GlobalAddress(n) | LN::Name(n) => {
            check_restricted_name_all_cases(env, NameCase::Address, n)
        }
    }
}

fn valid_local_variable_name(s: Symbol) -> bool {
    s.starts_with('_') || s.starts_with(|c: char| c.is_ascii_lowercase())
}

pub fn check_valid_function_parameter_name(
    env: &mut CompilationEnv,
    is_macro: Option<Loc>,
    v: &P::Var,
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
            env.add_diag(diag);
        }
    } else if is_syntax_identifier {
        let msg = format!(
            "Invalid parameter name '{}'. Non-'{}' parameter names cannot start with '$'",
            v, MACRO_MODIFIER,
        );
        let mut diag = diag!(Declarations::InvalidName, (v.loc(), msg));
        diag.add_note(SYNTAX_IDENTIFIER_NOTE);
        env.add_diag(diag);
    } else if !is_valid_local_variable_name(v.value()) {
        let msg = format!(
            "Invalid parameter name '{}'. Local variable names must start with 'a'..'z', '_', \
            or be a valid name quoted with backticks (`name`)",
            v,
        );
        env.add_diag(diag!(Declarations::InvalidName, (v.loc(), msg)));
    }
    let _ = check_restricted_name_all_cases(env, NameCase::Variable, &v.0);
}

pub fn check_valid_local_name(env: &mut CompilationEnv, v: &P::Var) {
    if !is_valid_local_variable_name(v.value()) {
        let msg = format!(
            "Invalid local name '{}'. Local variable names must start with 'a'..'z', '_', \
            or be a valid name quoted with backticks (`name`)",
            v,
        );
        env.add_diag(diag!(Declarations::InvalidName, (v.loc(), msg)));
    }
    let _ = check_restricted_name_all_cases(env, NameCase::Variable, &v.0);
}

fn is_valid_local_variable_name(s: Symbol) -> bool {
    P::Var::is_valid_name(s) && !P::Var::is_syntax_identifier_name(s)
}

#[derive(Copy, Clone, Debug)]
pub enum NameCase {
    Constant,
    Function,
    Struct,
    Enum,
    Module,
    ModuleAlias,
    Variable,
    Variant,
    Address,
    TypeParameter,
}

impl NameCase {
    pub const fn name(&self) -> &'static str {
        match self {
            NameCase::Constant => "constant",
            NameCase::Function => "function",
            NameCase::Struct => "struct",
            NameCase::Enum => "enum",
            NameCase::Module => "module",
            NameCase::ModuleAlias => "module alias",
            NameCase::Variable => "variable",
            NameCase::Variant => "variant",
            NameCase::Address => "address",
            NameCase::TypeParameter => "type parameter",
        }
    }

    pub const fn error_code(&self) -> codes::NameResolution {
        use codes::NameResolution;
        match self {
            NameCase::Constant => NameResolution::UnboundModuleMember,
            NameCase::Function => NameResolution::UnboundModuleMember,
            NameCase::Struct => NameResolution::UnboundModuleMember,
            NameCase::Enum => NameResolution::UnboundModuleMember,
            NameCase::Module => NameResolution::UnboundModule,
            NameCase::ModuleAlias => NameResolution::UnboundModule,
            NameCase::Variable => NameResolution::UnboundVariable,
            NameCase::Variant => NameResolution::UnboundVariant,
            NameCase::Address => NameResolution::UnboundAddress,
            NameCase::TypeParameter => NameResolution::UnboundTypeParameter,
        }
    }
}

pub fn check_valid_module_member_name(
    env: &mut CompilationEnv,
    member: NameCase,
    name: Name,
) -> Option<Name> {
    match check_valid_module_member_name_impl(context, member, &name, member) {
        Err(()) => None,
        Ok(()) => Some(name),
    }
}

pub fn check_valid_module_member_alias(
    env: &mut CompilationEnv,
    member: NameCase,
    alias: Name,
) -> Option<Name> {
    match check_valid_module_member_name_impl(
        env,
        member,
        &alias,
        NameCase::ModuleMemberAlias(member),
    ) {
        Err(()) => None,
        Ok(()) => Some(alias),
    }
}

pub fn check_valid_module_member_name_impl(
    env: &mut CompilationEnv,
    member: NameCase,
    n: &Name,
    case: NameCase,
) -> Result<(), ()> {
    use NameCase as N;
    fn upper_first_letter(s: &str) -> String {
        let mut chars = s.chars();
        match chars.next() {
            None => String::new(),
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        }
    }
    match member {
        N::Function => {
            if n.value.starts_with(|c| c == '_') {
                let msg = format!(
                    "Invalid {} name '{}'. {} names cannot start with '_'",
                    case.name(),
                    n,
                    upper_first_letter(case.name()),
                );
                env.add_diag(diag!(Declarations::InvalidName, (n.loc, msg)));
                return Err(());
            }
        }
        N::Constant | N::Struct | N::Enum => {
            if !is_valid_datatype_or_constant_name(&n.value) {
                let msg = format!(
                    "Invalid {} name '{}'. {} names must start with 'A'..'Z'",
                    case.name(),
                    n,
                    upper_first_letter(case.name()),
                );
                env.add_diag(diag!(Declarations::InvalidName, (n.loc, msg)));
                return Err(());
            }
        }
        _ => {
            env.add_diag(ice!((
                n.loc,
                format!("Called check_valid_module_member_name_impl with {case}")
            )));
        }
    }

    // TODO move these names to a more central place?
    check_restricted_names(
        env,
        case,
        n,
        crate::naming::ast::BuiltinFunction_::all_names(),
    )?;
    check_restricted_names(
        env,
        case,
        n,
        crate::naming::ast::BuiltinTypeName_::all_names(),
    )?;

    // Restricting Self for now in the case where we ever have impls
    // Otherwise, we could allow it
    check_restricted_name_all_cases(env, case, n)?;

    Ok(())
}

pub fn is_valid_datatype_or_constant_name(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_uppercase())
}

pub fn check_valid_type_parameter_name(
    env: &mut CompilationEnv,
    is_macro: Option<Loc>,
    n: &Name,
) -> Result<(), ()> {
    // TODO move these names to a more central place?
    if n.value == symbol!("_") {
        let diag = restricted_name_error(NameCase::TypeParameter, n.loc, "_");
        env.add_diag(diag);
        return Err(());
    }

    const SYNTAX_IDENTIFIER_NOTE: &str = "Type parameter names starting with '$' indicate that \
        their arguments do not have to satisfy certain constraints before the macro is expanded, \
        meaning types like '&mut u64' or '(bool, u8)' may be used as arguments.";

    let is_syntax_ident = P::Var::is_syntax_identifier_name(n.value);
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
            env.add_diag(diag);
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
                env.add_diag(diag);
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
        env.add_diag(diag);
    }

    // TODO move these names to a more central place?
    check_restricted_names(
        env,
        NameCase::TypeParameter,
        n,
        crate::naming::ast::BuiltinFunction_::all_names(),
    )?;
    check_restricted_names(
        env,
        NameCase::TypeParameter,
        n,
        crate::naming::ast::BuiltinTypeName_::all_names(),
    )?;

    check_restricted_name_all_cases(env, NameCase::TypeParameter, n)
}

// Checks for a restricted name in any decl case
// Self and vector are not allowed
pub fn check_restricted_name_all_cases(
    env: &mut CompilationEnv,
    case: NameCase,
    n: &Name,
) -> Result<(), ()> {
    match case {
        NameCase::Constant
        | NameCase::Function
        | NameCase::Struct
        | NameCase::Enum
        | NameCase::Module
        | NameCase::ModuleAlias
        | NameCase::Variant
        | NameCase::Address => {
            if P::Var::is_syntax_identifier_name(n.value) {
                let msg = format!(
                    "Invalid {} name '{}'. Identifiers starting with '$' can be used only for \
                    parameters and type paramters",
                    case.name(),
                    n,
                );
                env.add_diag(diag!(Declarations::InvalidName, (n.loc, msg)));
                return Err(());
            }
        }
        NameCase::Variable | NameCase::TypeParameter => (),
    }

    let n_str = n.value.as_str();
    let can_be_vector = matches!(case, NameCase::Module | NameCase::ModuleAlias);
    if n_str == P::ModuleName::SELF_NAME
        || (!can_be_vector && n_str == crate::naming::ast::BuiltinTypeName_::VECTOR)
    {
        env.add_diag(restricted_name_error(case, n.loc, n_str));
        Err(())
    } else {
        Ok(())
    }
}

pub fn check_restricted_names(
    env: &mut CompilationEnv,
    case: NameCase,
    sp!(loc, n_): &Name,
    all_names: &BTreeSet<Symbol>,
) -> Result<(), ()> {
    if all_names.contains(n_) {
        env.add_diag(restricted_name_error(case, *loc, n_));
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
