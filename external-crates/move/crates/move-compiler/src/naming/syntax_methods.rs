// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module verifies the usage of the "fake native" functions. These functions are declared
//! as 'native`, but do not appear in the compiled module. For developer sanity, they must be marked
//! with the `FAKE_NATIVE_ATTR`

use std::collections::BTreeSet;

use crate::{
    diag,
    editions::FeatureGate,
    expansion::ast::{self as E, Attribute, Attribute_, ModuleIdent},
    naming::{
        ast::{
            self as N, SyntaxMethod, SyntaxMethodKind, SyntaxMethodKind_, SyntaxMethods, TypeName,
        },
        translate::Context,
    },
    parser::ast::FunctionName,
    shared::{known_attributes::SyntaxAttribute, CompilationEnv},
};
use move_ir_types::location::*;

#[derive(PartialEq, Eq, Ord, PartialOrd)]
enum SyntaxMethodPrekind_ {
    For,
    Index,
    Assign,
}

type SyntaxMethodPrekind = Spanned<SyntaxMethodPrekind_>;

//-------------------------------------------------------------------------------------------------
// Resolution and recording
//-------------------------------------------------------------------------------------------------

/// validate and record syntax methods
pub(in crate::naming) fn resolve_syntax_attributes(
    context: &mut Context,
    syntax_methods: &mut SyntaxMethods,
    module_name: &ModuleIdent,
    function_name: &FunctionName,
    function: &N::Function,
) -> Option<()> {
    let attr = function.attributes.get_(&SyntaxAttribute::Syntax.into())?;
    let attr_loc = attr.loc;

    let syntax_method_prekinds = resolve_syntax_method_prekind(context.env, &attr)?;

    if !context.env.check_feature(
        context.current_package,
        FeatureGate::SyntaxMethods,
        attr_loc,
    ) {
        return None;
    }

    let param_ty = get_first_type(context, &attr_loc, &function.signature)?;
    let Some(type_name) = determine_subject_type_name(context, &module_name, &attr_loc, &param_ty)
    else {
        assert!(context.env.has_errors());
        return None;
    };

    if syntax_method_prekinds.is_empty() {
        assert!(context.env.has_errors());
        return None;
    }

    // For loops may need to change this, but for now we disallow this.
    if let Some(macro_loc) = function.macro_ {
        let msg = "Syntax attributes may not appear on macro definitions";
        let fn_msg = "This function is a macro";
        context.env.add_diag(diag!(
            Declarations::InvalidSyntaxMethod,
            (attr_loc, msg),
            (macro_loc, fn_msg)
        ));
        return None;
    }

    let public_visibility = match function.visibility {
        E::Visibility::Public(loc) => loc,
        E::Visibility::Friend(_) | E::Visibility::Package(_) | E::Visibility::Internal => {
            let msg = "Syntax attributes may only appear on public functions";
            let fn_msg = "This function is not public";
            context.env.add_diag(diag!(
                Declarations::InvalidVisibilityModifier,
                (attr_loc, msg),
                (function_name.0.loc, fn_msg)
            ));
            return None;
        }
    };

    let method_entry = syntax_methods.entry(type_name.clone()).or_default();

    for prekind in syntax_method_prekinds {
        let Some(kind) = determine_valid_kind(context, prekind, &param_ty) else {
            assert!(context.env.has_errors());
            continue;
        };
        if !valid_return_type(context, &kind, &function.signature.return_type) {
            assert!(context.env.has_errors());
            continue;
        }
        let new_syntax_method = SyntaxMethod {
            loc: function_name.0.loc,
            public_visibility,
            kind,
            tname: type_name.clone(),
            target_function: (*module_name, *function_name),
        };
        let kind_loc: &mut Option<Box<SyntaxMethod>> = method_entry.lookup_kind_entry(&kind);
        if let Some(previous) = kind_loc {
            prev_syntax_defn_error(context, &previous, kind, &type_name)
        } else {
            *kind_loc = Some(Box::new(new_syntax_method));
        }
    }
    Some(())
}

fn prev_syntax_defn_error(
    context: &mut Context,
    prev: &SyntaxMethod,
    sp!(sloc, method_kind): SyntaxMethodKind,
    sp!(_, type_name): &TypeName,
) {
    let msg = format!(
        "Redefined syntax method with kind '{}' for '{}'",
        method_kind.make_name(sloc),
        type_name
    );
    let prev_msg = "This syntax method was previously defined here.";
    context.env.add_diag(diag!(
        Declarations::InvalidAttribute,
        (sloc, msg),
        (prev.loc, prev_msg)
    ));
}

//-------------------------------------------------------------------------------------------------
// Syntax method attribute and kind handling
//-------------------------------------------------------------------------------------------------

fn attr_param_from_str(loc: Loc, name_str: &str) -> Option<SyntaxMethodPrekind> {
    match name_str {
        SyntaxAttribute::FOR => Some(sp(loc, SyntaxMethodPrekind_::For)),
        SyntaxAttribute::INDEX => Some(sp(loc, SyntaxMethodPrekind_::Index)),
        SyntaxAttribute::ASSIGN => Some(sp(loc, SyntaxMethodPrekind_::Assign)),
        _ => None,
    }
}

/// Resolve the mapping for a function + syntax attribute into a SyntaxMethodKind.
fn resolve_syntax_method_prekind(
    env: &mut CompilationEnv,
    sp!(loc, attr_): &Attribute,
) -> Option<BTreeSet<SyntaxMethodPrekind>> {
    match attr_ {
        Attribute_::Name(_) | Attribute_::Assigned(_, _) => {
            let msg = format!(
                "Expected a parameter list of syntax method usage forms, e.g., '{}({})'",
                SyntaxAttribute::SYNTAX,
                SyntaxAttribute::INDEX
            );
            env.add_diag(diag!(Declarations::InvalidAttribute, (*loc, msg)));
            None
        }
        Attribute_::Parameterized(_, inner) => {
            let mut kinds = BTreeSet::new();
            for (loc, _, sp!(argloc, arg)) in inner.iter() {
                match arg {
                    Attribute_::Name(name) => {
                        if let Some(kind) = attr_param_from_str(*argloc, name.value.as_str()) {
                            if let Some(prev_kind) = kinds.replace(kind) {
                                let msg = format!("Repeated syntax method identifier");
                                let prev = format!("Initially defined here");
                                env.add_diag(diag!(
                                    Declarations::InvalidAttribute,
                                    (loc, msg),
                                    (prev_kind.loc, prev)
                                ));
                            }
                        } else {
                        let msg = format!(
                            "Invalid syntax method identifier '{}'",
                            name
                        );
                        env.add_diag(diag!(Declarations::InvalidAttribute, (loc, msg)));
                    }
                    }
                    Attribute_::Assigned(n, _) => {
                        let msg = format!(
                            "Expected a standalone syntax method identifier, e.g., '{}({})'",
                            SyntaxAttribute::SYNTAX,
                            n
                        );
                        env.add_diag(diag!(Declarations::InvalidAttribute, (loc, msg)));
                    }
                    Attribute_::Parameterized(n, _) => {
                        let msg = format!(
                            "Expected a standalone syntax method identifier, e.g., '{}({})'",
                            SyntaxAttribute::SYNTAX,
                            n
                        );
                        env.add_diag(diag!(Declarations::InvalidAttribute, (loc, msg)));
                    }
                }
            }
            Some(kinds)
        }
    }
}

fn determine_valid_kind(
    context: &mut Context,
    sp!(sloc, kind): SyntaxMethodPrekind,
    subject_type: &N::Type,
) -> Option<SyntaxMethodKind> {
    use SyntaxMethodKind_ as SK;
    let sk = match kind {
        SyntaxMethodPrekind_::Index => {
            if valid_imm_ref(subject_type) {
                SK::Index
            } else if valid_mut_ref(subject_type) {
                SK::IndexMut
            } else {
                let msg = format!(
                    "'{}' is only valid if the first parameter's type is a reference as '&' or '&mut'",
                    SyntaxAttribute::INDEX,
                );
                let ty_msg = "This type is not a reference";
                context.env.add_diag(diag!(
                    Declarations::InvalidAttribute,
                    (sloc, msg),
                    (subject_type.loc, ty_msg)
                ));
                return None;
            }
        }
        SyntaxMethodPrekind_::For => {
            let msg = "'for' syntax attributes are not currently supported";
            context
                .env
                .add_diag(diag!(Declarations::InvalidAttribute, (sloc, msg),));
            return None;
        }
        // SyntaxMethodPrekind_::For => match mut_opt {
        //     Some((loc, true)) => SK::ForMut,
        //     Some((loc, false)) => SK::ForImm,
        //     None => SK::ForVal,
        // },
        SyntaxMethodPrekind_::Assign => {
            let msg = "'assign' syntax attributes are not currently supported";
            context
                .env
                .add_diag(diag!(Declarations::InvalidAttribute, (sloc, msg),));
            return None;
        } // SyntaxMethodPrekind_::Assign => match mut_opt {
          //     Some((loc, true)) => SK::Assign,
          //     _ => {
          //         let msg = format!(
          //         "'{}' is only valid if the first parameter's type is a mutable reference as '&mut'",
          //         SyntaxAttribute::INDEX,
          //     );
          //         let ty_msg = "This type is not a reference";
          //         context.env.add_diag(diag!(
          //             Declarations::InvalidAttribute,
          //             (sloc, msg),
          //             (*ty_loc, msg)
          //         ));
          //         return None;
          //     }
          // },
    };
    Some(sp(sloc, sk))
}

//-------------------------------------------------------------------------------------------------
// Types
//-------------------------------------------------------------------------------------------------

const INVALID_MODULE_MSG: &str = "Invalid 'syntax' definition";
const INVALID_MODULE_TYPE_MSG: &str = "This type is defined in a different module";

fn determine_subject_type_name(
    context: &mut Context,
    cur_module: &ModuleIdent,
    ann_loc: &Loc,
    sp!(loc, ty_): &N::Type,
) -> Option<TypeName> {
    match ty_ {
        N::Type_::Apply(_, type_name, _) => {
            match &type_name.value {
                N::TypeName_::Multiple(_) => {
                    let msg = "Invalid type for syntax method definition";
                    let mut diag = diag!(Declarations::InvalidSyntaxMethod, (*loc, msg));
                    diag.add_note("Syntax methods may only be defined for single base types");
                    context.env.add_diag(diag);
                    None
                }
                N::TypeName_::Builtin(builtin_type_name) => {
                    if let Some(definer) = context.env.primitive_definer(builtin_type_name.value) {
                        if cur_module == definer {
                            Some(type_name.clone())
                        } else {
                            context.env.add_diag(diag!(
                                Declarations::InvalidSyntaxMethod,
                                (*ann_loc, INVALID_MODULE_MSG),
                                (*loc, INVALID_MODULE_TYPE_MSG)
                            ));
                            None
                        }
                    } else {
                        context.env.add_diag(diag!(
                            Declarations::InvalidSyntaxMethod,
                            (*ann_loc, "The subject type is a Move builtin type"),
                        ));
                        None
                    }
                } // something like fake_natives?
                N::TypeName_::ModuleType(defining_module, _n) => {
                    if cur_module == defining_module {
                        Some(type_name.clone())
                    } else {
                        context.env.add_diag(diag!(
                            Declarations::InvalidSyntaxMethod,
                            (*ann_loc, INVALID_MODULE_MSG),
                            (*loc, INVALID_MODULE_TYPE_MSG)
                        ));
                        None
                    }
                }
            }
        }
        N::Type_::Ref(_, inner) => determine_subject_type_name(context, cur_module, ann_loc, inner),
        N::Type_::Param(param) => {
            let msg = format!(
                "Invalid {} annotation. Cannot associate a syntax method with a type parameter",
                SyntaxAttribute::SYNTAX
            );
            let tmsg = format!(
                "But '{}' was declared as a type parameter here",
                param.user_specified_name
            );
            context.env.add_diag(diag!(
                Declarations::InvalidSyntaxMethod,
                (*ann_loc, msg),
                (*loc, tmsg)
            ));
            None
        }
        N::Type_::Var(_) | N::Type_::Anything | N::Type_::UnresolvedError => {
            assert!(context.env.has_errors());
            None
        }
        N::Type_::Unit | N::Type_::Fun(_, _) => {
            let msg = "Invalid type for syntax method definition";
            let mut diag = diag!(Declarations::InvalidSyntaxMethod, (*loc, msg));
            diag.add_note("Syntax methods may only be defined for single base types");
            context.env.add_diag(diag);
            None
        }
    }
}

fn valid_return_type(
    context: &mut Context,
    sp!(loc, kind_): &SyntaxMethodKind,
    ty: &N::Type,
) -> bool {
    match kind_ {
        SyntaxMethodKind_::Index if valid_imm_ref(ty) => valid_index_return_type(context, &loc, ty),
        SyntaxMethodKind_::Index if valid_mut_ref(ty) => {
            let msg = format!("Invalid {} annotation", SyntaxAttribute::SYNTAX);
            let tmsg =
                "This syntax method must return an immutable reference to match its subject type";
            context.env.add_diag(diag!(
                Declarations::InvalidSyntaxMethod,
                (*loc, msg),
                (ty.loc, tmsg)
            ));
            false
        }
        SyntaxMethodKind_::Index => {
            let msg = format!(
                "Invalid {} annotation. This syntax method must return an immutable reference",
                SyntaxAttribute::SYNTAX
            );
            let tmsg = "This is not an immutable reference";
            context.env.add_diag(diag!(
                Declarations::InvalidSyntaxMethod,
                (*loc, msg),
                (ty.loc, tmsg)
            ));
            false
        }

        SyntaxMethodKind_::IndexMut if valid_mut_ref(ty) => {
            let ret_ty = valid_index_return_type(context, &loc, ty);
            println!("valid ret type: {ret_ty}");
            ret_ty
        }
        SyntaxMethodKind_::IndexMut if valid_imm_ref(ty) => {
            let msg = format!("Invalid {} annotation", SyntaxAttribute::SYNTAX);
            let tmsg =
                "This syntax method must return a mutable reference to match its subject type";
            context.env.add_diag(diag!(
                Declarations::InvalidSyntaxMethod,
                (*loc, msg),
                (ty.loc, tmsg)
            ));
            false
        }
        SyntaxMethodKind_::IndexMut => {
            let msg = format!(
                "Invalid {} annotation. This syntax method must return a mutable reference",
                SyntaxAttribute::SYNTAX
            );
            let tmsg = "This is not a mutable reference";
            context.env.add_diag(diag!(
                Declarations::InvalidSyntaxMethod,
                (*loc, msg),
                (ty.loc, tmsg)
            ));
            false
        }
    }
}

fn valid_imm_ref(sp!(_, type_): &N::Type) -> bool {
    use N::Type_ as T;
    match type_ {
        T::Ref(false, _) => true,
        T::Ref(true, _) => false,
        T::Unit
        | T::Param(_)
        | T::Apply(_, _, _)
        | T::Fun(_, _)
        | T::Var(_)
        | T::Anything
        | T::UnresolvedError => false,
    }
}

fn valid_mut_ref(sp!(_, type_): &N::Type) -> bool {
    use N::Type_ as T;
    match type_ {
        T::Ref(true, _) => true,
        T::Ref(false, _) => false,
        T::Unit
        | T::Param(_)
        | T::Apply(_, _, _)
        | T::Fun(_, _)
        | T::Var(_)
        | T::Anything
        | T::UnresolvedError => false,
    }
}

fn valid_index_return_type(
    context: &mut Context,
    kind_loc: &Loc,
    sp!(tloc, type_): &N::Type,
) -> bool {
    match type_ {
        N::Type_::Apply(_, _, _) | N::Type_::Param(_) => true,
        N::Type_::Ref(_, inner) => valid_index_return_type(context, kind_loc, inner),
        N::Type_::Unit => {
            let msg = format!(
                "Invalid {} annotation. This syntax method cannot return a unit type",
                SyntaxAttribute::SYNTAX
            );
            let tmsg = "Unit type occurs as the return type for this function";
            context.env.add_diag(diag!(
                Declarations::InvalidSyntaxMethod,
                (*kind_loc, msg),
                (*tloc, tmsg)
            ));
            false
        }
        N::Type_::Fun(_, _) => {
            let msg = format!(
                "Invalid {} annotation. A syntax method cannot return a function",
                SyntaxAttribute::SYNTAX
            );
            let tmsg = "But a function type appears in this return type";
            context.env.add_diag(diag!(
                Declarations::InvalidSyntaxMethod,
                (*kind_loc, msg),
                (*tloc, tmsg)
            ));
            false
        }
        N::Type_::Var(_) | N::Type_::Anything | N::Type_::UnresolvedError => {
            // Already an error state, so pass
            assert!(context.env.has_errors());
            false
        }
    }
}

fn get_first_type(
    context: &mut Context,
    attr_loc: &Loc,
    fn_signature: &N::FunctionSignature,
) -> Option<N::Type> {
    if let Some((_, _, ty)) = fn_signature.parameters.get(0) {
        Some(ty.clone())
    } else {
        let msg = format!(
            "Invalid attribute. {} is only valid if the function takes at least one parameter",
            SyntaxAttribute::SYNTAX
        );
        context
            .env
            .add_diag(diag!(Declarations::InvalidAttribute, (*attr_loc, msg)));
        None
    }
}
