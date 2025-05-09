// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module contains the implementation of functions supporting conversion of various
//! constructs to their IDE-friendly string representations.

use crate::symbols::def_info::{FunType, VariantInfo};
use move_compiler::{
    expansion::{
        ast::{self as E, AbilitySet, ModuleIdent_, Value, Value_, Visibility},
        name_validation::{
            IMPLICIT_STD_MEMBERS, IMPLICIT_STD_MODULES, IMPLICIT_SUI_MEMBERS, IMPLICIT_SUI_MODULES,
            ModuleMemberKind,
        },
    },
    naming::ast::{Type, Type_, TypeName_},
    shared::{Identifier, Name},
    typing::ast::{Exp, ExpListItem, SequenceItem, SequenceItem_, UnannotatedExp_},
};
use move_core_types::{account_address::AccountAddress, parsing::address::NumericalAddress};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;

const STD_LIB_PKG_ADDRESS: &str = "0x1";
const SUI_LIB_PKG_ADDRESS: &str = "0x2";

pub fn visibility_to_ide_string(visibility: &Visibility) -> String {
    let mut visibility_str = "".to_string();

    if visibility != &Visibility::Internal {
        visibility_str.push_str(format!("{} ", visibility).as_str());
    }
    visibility_str
}

pub fn type_args_to_ide_string(type_args: &[Type], separate_lines: bool, verbose: bool) -> String {
    let mut type_args_str = "".to_string();
    if !type_args.is_empty() {
        type_args_str.push('<');
        if separate_lines {
            type_args_str.push('\n');
        }
        type_args_str.push_str(&type_list_to_ide_string(type_args, separate_lines, verbose));
        if separate_lines {
            type_args_str.push('\n');
        }
        type_args_str.push('>');
    }
    type_args_str
}

pub fn datatype_type_args_to_ide_string(type_args: &[(Type, bool)], verbose: bool) -> String {
    let mut type_args_str = "".to_string();
    if !type_args.is_empty() {
        type_args_str.push('<');
        type_args_str.push_str(&datatype_type_list_to_ide_string(type_args, verbose));
        type_args_str.push('>');
    }
    type_args_str
}

pub fn typed_id_list_to_ide_string(
    names: &[Name],
    types: &[Type],
    list_start: char,
    list_end: char,
    separate_lines: bool,
    verbose: bool,
) -> String {
    let list = names
        .iter()
        .zip(types.iter())
        .map(|(n, t)| {
            if separate_lines {
                format!("\t{}: {}", n.value, type_to_ide_string(t, verbose))
            } else {
                format!("{}: {}", n.value, type_to_ide_string(t, verbose))
            }
        })
        .collect::<Vec<_>>()
        .join(if separate_lines { ",\n" } else { ", " });
    if separate_lines && !list.is_empty() {
        format!("{}\n{}\n{}", list_start, list, list_end)
    } else {
        format!("{}{}{}", list_start, list, list_end)
    }
}

pub fn type_to_ide_string(sp!(_, t): &Type, verbose: bool) -> String {
    match t {
        Type_::Unit => "()".to_string(),
        Type_::Ref(m, r) => format!(
            "&{}{}",
            if *m { "mut " } else { "" },
            type_to_ide_string(r, verbose)
        ),
        Type_::Param(tp) => {
            format!("{}", tp.user_specified_name)
        }
        Type_::Apply(_, sp!(_, type_name), ss) => match type_name {
            TypeName_::Multiple(_) => {
                format!(
                    "({})",
                    type_list_to_ide_string(ss, /* separate_lines */ false, verbose)
                )
            }
            TypeName_::Builtin(name) => {
                if ss.is_empty() {
                    format!("{}", name)
                } else {
                    format!(
                        "{}<{}>",
                        name,
                        type_list_to_ide_string(ss, /* separate_lines */ false, verbose)
                    )
                }
            }
            TypeName_::ModuleType(sp!(_, mod_ident), datatype_name) => {
                let type_args = if ss.is_empty() {
                    "".to_string()
                } else {
                    format!(
                        "<{}>",
                        type_list_to_ide_string(ss, /* separate_lines */ false, verbose)
                    )
                };
                if verbose {
                    format!(
                        "{}{}{}",
                        mod_ident_to_ide_string(mod_ident, Some(&datatype_name.value()), true),
                        datatype_name,
                        type_args
                    )
                } else {
                    datatype_name.to_string()
                }
            }
        },
        Type_::Fun(args, ret) => {
            format!(
                "|{}| -> {}",
                type_list_to_ide_string(args, /* separate_lines */ false, verbose),
                type_to_ide_string(ret, verbose)
            )
        }
        Type_::Anything => "_".to_string(),
        Type_::Var(_) => "invalid type (var)".to_string(),
        Type_::UnresolvedError => "unknown type (unresolved)".to_string(),
    }
}

pub fn type_list_to_ide_string(types: &[Type], separate_lines: bool, verbose: bool) -> String {
    types
        .iter()
        .map(|t| {
            if separate_lines {
                format!("\t{}", type_to_ide_string(t, verbose))
            } else {
                type_to_ide_string(t, verbose)
            }
        })
        .collect::<Vec<_>>()
        .join(if separate_lines { ",\n" } else { ", " })
}

pub fn datatype_type_list_to_ide_string(types: &[(Type, bool)], verbose: bool) -> String {
    types
        .iter()
        .map(|(t, phantom)| {
            if *phantom {
                format!("phantom {}", type_to_ide_string(t, verbose))
            } else {
                type_to_ide_string(t, verbose)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn ret_type_to_ide_str(ret_type: &Type, verbose: bool) -> String {
    match ret_type {
        sp!(_, Type_::Unit) => "".to_string(),
        _ => format!(": {}", type_to_ide_string(ret_type, verbose)),
    }
}
/// Conversions of constant values to strings is currently best-effort which is why this function
/// returns an Option (in the worst case we will display constant name and type but no value).
pub fn const_val_to_ide_string(exp: &Exp) -> Option<String> {
    ast_exp_to_ide_string(exp)
}

pub fn ast_exp_to_ide_string(exp: &Exp) -> Option<String> {
    use UnannotatedExp_ as UE;
    let sp!(_, e) = &exp.exp;
    match e {
        UE::Constant(mod_ident, name) => Some(format!("{mod_ident}::{name}")),
        UE::Value(v) => Some(ast_value_to_ide_string(v)),
        UE::Vector(_, _, _, exp) => ast_exp_to_ide_string(exp).map(|s| format!("[{s}]")),
        UE::Block((_, seq)) | UE::NamedBlock(_, (_, seq)) => {
            let seq_items = seq
                .iter()
                .map(ast_seq_item_to_ide_string)
                .collect::<Vec<_>>();
            if seq_items.iter().any(|o| o.is_none()) {
                // even if only one element cannot be turned into string, don't try displaying block content at all
                return None;
            }
            Some(
                seq_items
                    .into_iter()
                    .map(|o| o.unwrap())
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        }
        UE::ExpList(list) => {
            let items = list
                .iter()
                .map(|i| match i {
                    ExpListItem::Single(exp, _) => ast_exp_to_ide_string(exp),
                    ExpListItem::Splat(_, exp, _) => ast_exp_to_ide_string(exp),
                })
                .collect::<Vec<_>>();
            if items.iter().any(|o| o.is_none()) {
                // even if only one element cannot be turned into string, don't try displaying expression list at all
                return None;
            }
            Some(
                items
                    .into_iter()
                    .map(|o| o.unwrap())
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        }
        UE::UnaryExp(op, exp) => ast_exp_to_ide_string(exp).map(|s| format!("{op}{s}")),

        UE::BinopExp(lexp, op, _, rexp) => {
            let Some(ls) = ast_exp_to_ide_string(lexp) else {
                return None;
            };
            let Some(rs) = ast_exp_to_ide_string(rexp) else {
                return None;
            };
            Some(format!("{ls} {op} {rs}"))
        }
        _ => None,
    }
}

pub fn ast_seq_item_to_ide_string(sp!(_, seq_item): &SequenceItem) -> Option<String> {
    use SequenceItem_ as SI;
    match seq_item {
        SI::Seq(exp) => ast_exp_to_ide_string(exp),
        _ => None,
    }
}

pub fn ast_value_to_ide_string(sp!(_, val): &Value) -> String {
    use Value_ as V;
    match val {
        V::Address(addr) => format!("@{}", addr),
        V::InferredNum(u) => format!("{}", u),
        V::U8(u) => format!("{}", u),
        V::U16(u) => format!("{}", u),
        V::U32(u) => format!("{}", u),
        V::U64(u) => format!("{}", u),
        V::U128(u) => format!("{}", u),
        V::U256(u) => format!("{}", u),
        V::Bool(b) => format!("{}", b),
        V::Bytearray(vec) => format!(
            "[{}]",
            vec.iter()
                .map(|v| format!("{}", v))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Creates a string representing a module ID, either on it's owne as in `pkg::module`
/// or as part of a datatype or function type, in which it should be `pkg::module::`.
/// If it's part of the datatype, name of the datatype is passed in `datatype_name_opt`.
pub fn mod_ident_to_ide_string(
    mod_ident: &ModuleIdent_,
    datatype_name_opt: Option<&Symbol>,
    is_access_chain_prefix: bool, // part of access chaing that should end with `::`
) -> String {
    use E::Address as A;
    // the module ID is to be a prefix to a data
    let suffix = if is_access_chain_prefix { "::" } else { "" };
    match mod_ident.address {
        A::Numerical { name, value, .. } => {
            fn strip_prefix(
                addr_name: Option<Name>,
                addr_value: Spanned<NumericalAddress>,
                mod_ident: &ModuleIdent_,
                datatype_name_opt: Option<&Symbol>,
                suffix: &str,
                pkg_addr: &str,
                implicit_modules: &[Symbol],
                implicit_members: &[(Symbol, Symbol, ModuleMemberKind)],
            ) -> (bool, String) {
                let pkg_name = match addr_name {
                    Some(n) => n.to_string(),
                    None => addr_value.to_string(),
                };

                let Ok(std_lib_pkg_address) = AccountAddress::from_hex_literal(pkg_addr) else {
                    // getting stdlib address did not work - use the whole thing
                    return (false, format!("{pkg_name}::{}{}", mod_ident.module, suffix));
                };
                if addr_value.value.into_inner() != std_lib_pkg_address {
                    // it's not a stdlib package - use the whole thing
                    return (false, format!("{pkg_name}::{}{}", mod_ident.module, suffix));
                }
                // try stripping both package and module if this conversion
                // is for a datatype, oherwise try only stripping package
                if let Some(datatype_name) = datatype_name_opt {
                    if implicit_members.iter().any(
                        |(implicit_mod_name, implicit_datatype_name, _)| {
                            mod_ident.module.value() == *implicit_mod_name
                                && datatype_name == implicit_datatype_name
                        },
                    ) {
                        // strip both package and module (whether its meant to be
                        // part of access chain or not, if there is not module,
                        // there should be no `::` at the end)
                        return (true, "".to_string());
                    }
                }
                if implicit_modules
                    .iter()
                    .any(|implicit_mod_name| mod_ident.module.value() == *implicit_mod_name)
                {
                    // strip package
                    return (true, format!("{}{}", mod_ident.module.value(), suffix));
                }
                // stripping prefix didn't work - use the whole thing
                (true, format!("{pkg_name}::{}{}", mod_ident.module, suffix))
            }

            let (strippable, mut res) = strip_prefix(
                name,
                value,
                mod_ident,
                datatype_name_opt,
                suffix,
                STD_LIB_PKG_ADDRESS,
                IMPLICIT_STD_MODULES,
                IMPLICIT_STD_MEMBERS,
            );

            // check for Sui implicits only if the previous call determined
            // that the prefix was not strippable with respect to implicits
            // passed as its arguments
            if !strippable {
                (_, res) = strip_prefix(
                    name,
                    value,
                    mod_ident,
                    datatype_name_opt,
                    suffix,
                    SUI_LIB_PKG_ADDRESS,
                    IMPLICIT_SUI_MODULES,
                    IMPLICIT_SUI_MEMBERS,
                );
            }
            res
        }
        A::NamedUnassigned(n) => format!("{n}::{}", mod_ident.module).to_string(),
    }
}

pub fn fun_type_to_ide_string(fun_type: &FunType) -> String {
    match fun_type {
        FunType::Entry => "entry ",
        FunType::Macro => "macro ",
        FunType::Regular => "",
    }
    .to_string()
}

pub fn abilities_to_ide_string(abilities: &AbilitySet) -> String {
    if abilities.is_empty() {
        "".to_string()
    } else {
        format!(
            " has {}",
            abilities
                .iter()
                .map(|a| format!("{a}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

pub fn variant_to_ide_string(variants: &[VariantInfo]) -> String {
    // how many variant lines (including optional ellipsis if there
    // are too many of them) are printed
    const NUM_PRINTED: usize = 7;
    let mut vstrings = variants
        .iter()
        .enumerate()
        .map(|(idx, info)| {
            if idx >= NUM_PRINTED - 1 {
                "\t/* ... */".to_string()
            } else if info.empty {
                format!("\t{}", info.name)
            } else if info.positional {
                format!("\t{}( /* ... */ )", info.name)
            } else {
                format!("\t{}{{ /* ... */ }}", info.name)
            }
        })
        .collect::<Vec<_>>();
    vstrings.truncate(NUM_PRINTED);
    vstrings.join(",\n")
}
