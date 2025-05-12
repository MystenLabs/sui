// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module contains code for handling information about definitions
//! for source-level identifiers.

use crate::symbols::ide_strings::{
    abilities_to_ide_string, datatype_type_args_to_ide_string, fun_type_to_ide_string,
    mod_ident_to_ide_string, ret_type_to_ide_str, type_args_to_ide_string, type_list_to_ide_string,
    type_to_ide_string, typed_id_list_to_ide_string, variant_to_ide_string,
    visibility_to_ide_string,
};

use std::fmt;

use move_compiler::{
    expansion::ast::{AbilitySet, ModuleIdent_, Visibility},
    naming::ast::Type,
    shared::Name,
};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;

#[derive(Debug, Clone, Eq, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum DefInfo {
    /// Type of an identifier
    Type(Type),
    Function(
        /// Defining module
        ModuleIdent_,
        /// Visibility
        Visibility,
        /// For example, a macro or entry function
        FunType,
        /// Name
        Symbol,
        /// Type args
        Vec<Type>,
        /// Arg names
        Vec<Name>,
        /// Arg types
        Vec<Type>,
        /// Ret type
        Type,
        /// Doc string
        Option<String>,
    ),
    Struct(
        /// Defining module
        ModuleIdent_,
        /// Name
        Symbol,
        /// Visibility
        Visibility,
        /// Type args
        Vec<(Type, bool /* phantom */)>,
        /// Abilities
        AbilitySet,
        /// Field names
        Vec<Name>,
        /// Field types
        Vec<Type>,
        /// Doc string
        Option<String>,
    ),
    Enum(
        /// Defining module
        ModuleIdent_,
        /// Name
        Symbol,
        /// Visibility
        Visibility,
        /// Type args
        Vec<(Type, bool /* phantom */)>,
        /// Abilities
        AbilitySet,
        /// Info about variants
        Vec<VariantInfo>,
        /// Doc string
        Option<String>,
    ),
    Variant(
        /// Defining module of the containing enum
        ModuleIdent_,
        /// Name of the containing enum
        Symbol,
        /// Variant name
        Symbol,
        /// Positional fields?
        bool,
        /// Field names
        Vec<Name>,
        /// Field types
        Vec<Type>,
        /// Doc string
        Option<String>,
    ),
    Field(
        /// Defining module of the containing struct
        ModuleIdent_,
        /// Name of the containing struct
        Symbol,
        /// Field name
        Symbol,
        /// Field type
        Type,
        /// Doc string
        Option<String>,
    ),
    Local(
        /// Name
        Symbol,
        /// Type
        Type,
        /// Should displayed definition be preceded by `let`?
        bool,
        /// Should displayed definition be preceded by `mut`?
        bool,
        /// Location of enum's guard expression (if any) in case
        /// this local definition represents match pattern's variable
        Option<Loc>,
    ),
    Const(
        /// Defining module
        ModuleIdent_,
        /// Name
        Symbol,
        /// Type
        Type,
        /// Value
        Option<String>,
        /// Doc string
        Option<String>,
    ),
    Module(
        /// pkg::mod
        String,
        /// Doc string
        Option<String>,
    ),
}
/// Type of a function
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum FunType {
    Macro,
    Entry,
    Regular,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct VariantInfo {
    pub name: Name,
    pub empty: bool,
    pub positional: bool,
}

impl fmt::Display for DefInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Type(t) => {
                // Technically, we could use error_format function here to display the "regular"
                // type, but the original intent of this function is subtly different that we need
                // (i.e., to be used by compiler error messages) which, for example, results in
                // verbosity that is not needed here.
                //
                // It also seems like a reasonable idea to be able to tune user experience in the
                // IDE independently on how compiler error messages are generated.
                write!(f, "{}", type_to_ide_string(t, /* verbose */ true))
            }
            Self::Function(
                mod_ident,
                visibility,
                fun_type,
                name,
                type_args,
                arg_names,
                arg_types,
                ret_type,
                _,
            ) => {
                const SINGLE_LINE_TYPE_ARGS_NUM: usize = 2;
                // The strategy for displaying function signature is as follows:
                // - if there are more than SINGLE_LINE_TYPE_ARGS_NUM type args,
                //   they are displayed on separate lines
                // - "regular" args are always displayed on separate lines, which
                //   which is motivated by the fact that datatypes are displayed
                //   in a fully-qualified form (i.e., with package and module name),
                //   and that makes the function name already long and (likely)
                //   the length of each individual type also long (modulo primitive
                //   types of course, but I think we can live with that)
                let type_args_str = type_args_to_ide_string(
                    type_args,
                    /* separate_lines */ type_args.len() > SINGLE_LINE_TYPE_ARGS_NUM,
                    /* verbose */ true,
                );
                let args_str = typed_id_list_to_ide_string(
                    arg_names, arg_types, '(', ')', /* separate_lines */ true,
                    /* verbose */ true,
                );
                let ret_type_str = ret_type_to_ide_str(ret_type, /* verbose */ true);
                write!(
                    f,
                    "{}{}fun {}{}{}{}{}",
                    visibility_to_ide_string(visibility),
                    fun_type_to_ide_string(fun_type),
                    mod_ident_to_ide_string(mod_ident, None, true),
                    name,
                    type_args_str,
                    args_str,
                    ret_type_str,
                )
            }
            Self::Struct(
                mod_ident,
                name,
                visibility,
                type_args,
                abilities,
                field_names,
                field_types,
                _,
            ) => {
                let type_args_str =
                    datatype_type_args_to_ide_string(type_args, /* verbose */ true);
                let abilities_str = abilities_to_ide_string(abilities);
                if field_names.is_empty() {
                    write!(
                        f,
                        "{}struct {}{}{}{} {{}}",
                        visibility_to_ide_string(visibility),
                        mod_ident_to_ide_string(mod_ident, Some(name), true),
                        name,
                        type_args_str,
                        abilities_str,
                    )
                } else {
                    write!(
                        f,
                        "{}struct {}{}{}{} {}",
                        visibility_to_ide_string(visibility),
                        mod_ident_to_ide_string(mod_ident, Some(name), true),
                        name,
                        type_args_str,
                        abilities_str,
                        typed_id_list_to_ide_string(
                            field_names,
                            field_types,
                            '{',
                            '}',
                            /* separate_lines */ true,
                            /* verbose */ true
                        ),
                    )
                }
            }
            Self::Enum(mod_ident, name, visibility, type_args, abilities, variants, _) => {
                let type_args_str =
                    datatype_type_args_to_ide_string(type_args, /* verbose */ true);
                let abilities_str = abilities_to_ide_string(abilities);
                if variants.is_empty() {
                    write!(
                        f,
                        "{}enum {}{}{}{} {{}}",
                        visibility_to_ide_string(visibility),
                        mod_ident_to_ide_string(mod_ident, Some(name), true),
                        name,
                        type_args_str,
                        abilities_str,
                    )
                } else {
                    write!(
                        f,
                        "{}enum {}{}{}{} {{\n{}\n}}",
                        visibility_to_ide_string(visibility),
                        mod_ident_to_ide_string(mod_ident, Some(name), true),
                        name,
                        type_args_str,
                        abilities_str,
                        variant_to_ide_string(variants)
                    )
                }
            }
            Self::Variant(mod_ident, enum_name, name, positional, field_names, field_types, _) => {
                if field_types.is_empty() {
                    write!(
                        f,
                        "{}{}::{}",
                        mod_ident_to_ide_string(mod_ident, Some(enum_name), true),
                        enum_name,
                        name
                    )
                } else if *positional {
                    write!(
                        f,
                        "{}{}::{}({})",
                        mod_ident_to_ide_string(mod_ident, Some(enum_name), true),
                        enum_name,
                        name,
                        type_list_to_ide_string(
                            field_types,
                            /* separate_lines */ false,
                            /* verbose */ true
                        )
                    )
                } else {
                    write!(
                        f,
                        "{}{}::{}{}",
                        mod_ident_to_ide_string(mod_ident, Some(enum_name), true),
                        enum_name,
                        name,
                        typed_id_list_to_ide_string(
                            field_names,
                            field_types,
                            '{',
                            '}',
                            /* separate_lines */ false,
                            /* verbose */ true,
                        ),
                    )
                }
            }
            Self::Field(mod_ident, struct_name, name, t, _) => {
                write!(
                    f,
                    "{}{}\n{}: {}",
                    mod_ident_to_ide_string(mod_ident, Some(struct_name), true),
                    struct_name,
                    name,
                    type_to_ide_string(t, /* verbose */ true)
                )
            }
            Self::Local(name, t, is_decl, is_mut, _) => {
                let mut_str = if *is_mut { "mut " } else { "" };
                if *is_decl {
                    write!(
                        f,
                        "let {}{}: {}",
                        mut_str,
                        name,
                        type_to_ide_string(t, /* verbose */ true)
                    )
                } else {
                    write!(
                        f,
                        "{}{}: {}",
                        mut_str,
                        name,
                        type_to_ide_string(t, /* verbose */ true)
                    )
                }
            }
            Self::Const(mod_ident, name, t, value, _) => {
                if let Some(v) = value {
                    write!(
                        f,
                        "const {}::{}: {} = {}",
                        mod_ident,
                        name,
                        type_to_ide_string(t, /* verbose */ true),
                        v
                    )
                } else {
                    write!(
                        f,
                        "const {}::{}: {}",
                        mod_ident,
                        name,
                        type_to_ide_string(t, /* verbose */ true)
                    )
                }
            }
            Self::Module(mod_ident_str, _) => write!(f, "module {mod_ident_str}"),
        }
    }
}
