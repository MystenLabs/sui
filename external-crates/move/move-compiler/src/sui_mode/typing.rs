// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_ir_types::location::Loc;

use crate::{
    diag,
    editions::Flavor,
    expansion::ast::{AbilitySet, ModuleIdent},
    naming::ast::{BuiltinTypeName_, FunctionSignature, TParam, Type, TypeName_, Type_, Var},
    parser::ast::{Ability_, FunctionName},
    shared::CompilationEnv,
    sui_mode::{
        ASCII_MODULE_NAME, ASCII_TYPE_NAME, CLOCK_MODULE_NAME, CLOCK_TYPE_NAME,
        ENTRY_FUN_SIGNATURE_DIAG, ID_TYPE_NAME, OBJECT_MODULE_NAME, OPTION_MODULE_NAME,
        OPTION_TYPE_NAME, SCRIPT_DIAG, STD_ADDR_NAME, SUI_ADDR_NAME, UTF_MODULE_NAME,
        UTF_TYPE_NAME,
    },
    typing::{
        ast as T,
        core::{ability_not_satisfied_tips, ProgramInfo, Subst},
        visitor::TypingVisitor,
    },
};

use super::{TX_CONTEXT_MODULE_NAME, TX_CONTEXT_TYPE_NAME};

//**************************************************************************************************
// Visitor
//**************************************************************************************************

pub struct SuiTypeChecks;

impl TypingVisitor for SuiTypeChecks {
    fn visit(&mut self, env: &mut CompilationEnv, info: &ProgramInfo, prog: &mut T::Program) {
        program(env, info, prog)
    }
}

//**************************************************************************************************
// Context
//**************************************************************************************************

#[allow(unused)]
struct Context<'a> {
    env: &'a mut CompilationEnv,
    info: &'a ProgramInfo,
    current_module: ModuleIdent,
    in_test: bool,
}

impl<'a> Context<'a> {
    fn new(
        env: &'a mut CompilationEnv,
        info: &'a ProgramInfo,
        current_module: ModuleIdent,
    ) -> Self {
        Context {
            env,
            current_module,
            info,
            in_test: false,
        }
    }
}

//**************************************************************************************************
// Entry
//**************************************************************************************************

pub fn program(env: &mut CompilationEnv, info: &ProgramInfo, prog: &T::Program) {
    let T::Program { modules, scripts } = prog;
    for script in scripts.values() {
        let config = env.package_config(script.package_name);
        if config.flavor != Flavor::Sui {
            continue;
        }

        // TODO point to PTB docs?
        let msg = "'scripts' are not supported on Sui. \
        Consider removing or refactoring into a 'module'";
        env.add_diag(diag!(SCRIPT_DIAG, (script.loc, msg)))
    }
    for (mident, mdef) in modules.key_cloned_iter() {
        module(env, info, mident, mdef);
    }
}

fn module(
    env: &mut CompilationEnv,
    info: &ProgramInfo,
    mident: ModuleIdent,
    mdef: &T::ModuleDefinition,
) {
    let config = env.package_config(mdef.package_name);
    if config.flavor != Flavor::Sui {
        return;
    }

    // Skip non-source, dependency modules
    if !mdef.is_source_module {
        return;
    }

    let mut context = Context::new(env, info, mident);
    for (name, fdef) in mdef.functions.key_cloned_iter() {
        function(&mut context, name, fdef);
    }
}

//**************************************************************************************************
// Functions
//**************************************************************************************************

fn function(context: &mut Context, name: FunctionName, fdef: &T::Function) {
    let T::Function {
        visibility: _,
        signature,
        acquires: _,
        body: _,
        warning_filter: _,
        index: _,
        attributes: _,
        entry,
    } = fdef;
    if let Some(entry_loc) = entry {
        entry_signature(context, *entry_loc, name, signature);
    }
}

//**************************************************************************************************
// entry types
//**************************************************************************************************

fn entry_signature(
    context: &mut Context,
    entry_loc: Loc,
    name: FunctionName,
    signature: &FunctionSignature,
) {
    let FunctionSignature {
        type_parameters: _,
        parameters,
        return_type,
    } = signature;
    let all_non_ctx_parameters = match parameters.last() {
        Some((_, last_param_ty)) if tx_context_kind(last_param_ty) != TxContextKind::None => {
            &parameters[0..parameters.len() - 1]
        }
        _ => &parameters,
    };
    entry_param(context, entry_loc, name, all_non_ctx_parameters);
    entry_return(context, entry_loc, name, return_type);
}

fn tx_context_kind(sp!(_, last_param_ty_): &Type) -> TxContextKind {
    let Type_::Ref(is_mut, inner_ty) = last_param_ty_ else {
        return TxContextKind::None
    };
    let Type_::Apply(_, sp!(_, inner_name), _) = &inner_ty.value else {
        return TxContextKind::None
    };
    if inner_name.is(SUI_ADDR_NAME, TX_CONTEXT_MODULE_NAME, TX_CONTEXT_TYPE_NAME) {
        if *is_mut {
            TxContextKind::Mutable
        } else {
            TxContextKind::Immutable
        }
    } else {
        TxContextKind::None
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum TxContextKind {
    // No TxContext
    None,
    // &mut TxContext
    Mutable,
    // &TxContext
    Immutable,
}

fn entry_param(
    context: &mut Context,
    entry_loc: Loc,
    name: FunctionName,
    parameters: &[(Var, Type)],
) {
    for (var, ty) in parameters {
        entry_param_ty(context, entry_loc, name, var, ty);
    }
}

/// A valid entry param type is
/// - A primitive (including strings, ID, and object)
/// - A vector of primitives (including nested vectors)
///
/// - An object
/// - A reference to an object
/// - A vector of objects
fn entry_param_ty(
    context: &mut Context,
    entry_loc: Loc,
    name: FunctionName,
    param: &Var,
    param_ty: &Type,
) {
    let is_mut_clock = is_mut_clock(param_ty);
    // TODO better error message for cases such as `MyObject<InnerTypeWithoutStore>`
    // which should give a contextual error about `MyObject` having `key`, but the instantiation
    // `MyObject<InnerTypeWithoutStore>` not having `key` due to `InnerTypeWithoutStore` not having
    // `store`
    let is_valid = is_entry_primitive_ty(param_ty) || is_entry_object_ty(param_ty);
    if is_mut_clock || !is_valid {
        let pmsg = format!(
            "Invalid 'entry' parameter type for parameter '{}'",
            param.value.name
        );
        let tmsg = if is_mut_clock {
            format!(
                "{a}::{m}::{n} must be passed by immutable reference, e.g. '&{a}::{m}::{n}'",
                a = SUI_ADDR_NAME,
                m = CLOCK_MODULE_NAME,
                n = CLOCK_TYPE_NAME,
            )
        } else {
            "'entry' parameters must be primitives (by-value), vectors of primitives, objects \
            (by-reference or by-value), or vectors of objects"
                .to_owned()
        };
        let emsg = format!("'{name}' was declared 'entry' here");
        context.env.add_diag(diag!(
            ENTRY_FUN_SIGNATURE_DIAG,
            (param.loc, pmsg),
            (param_ty.loc, tmsg),
            (entry_loc, emsg)
        ));
    }
}

fn is_mut_clock(param_ty: &Type) -> bool {
    match &param_ty.value {
        Type_::Ref(/* mut */ false, _) => false,
        Type_::Ref(/* mut */ true, t) => is_mut_clock(t),
        Type_::Apply(_, sp!(_, n_), _) => n_.is(SUI_ADDR_NAME, CLOCK_MODULE_NAME, CLOCK_TYPE_NAME),
        Type_::Unit
        | Type_::Param(_)
        | Type_::Var(_)
        | Type_::Anything
        | Type_::UnresolvedError => false,
    }
}

fn is_entry_primitive_ty(param_ty: &Type) -> bool {
    use BuiltinTypeName_ as B;
    use TypeName_ as N;

    match &param_ty.value {
        // A bit of a hack since no primitive has key
        Type_::Param(tp) => !tp.abilities.has_ability_(Ability_::Key),
        // nonsensical, but no error needed
        Type_::Apply(_, sp!(_, N::Multiple(_)), ts) => ts.iter().all(is_entry_primitive_ty),
        // Simple recursive cases
        Type_::Ref(_, t) => is_entry_primitive_ty(t),
        Type_::Apply(_, sp!(_, N::Builtin(sp!(_, B::Vector))), targs) => {
            debug_assert!(targs.len() == 1);
            is_entry_primitive_ty(&targs[0])
        }

        // custom "primitives"
        Type_::Apply(_, sp!(_, n), targs)
            if n.is(STD_ADDR_NAME, ASCII_MODULE_NAME, ASCII_TYPE_NAME)
                || n.is(STD_ADDR_NAME, UTF_MODULE_NAME, UTF_TYPE_NAME)
                || n.is(SUI_ADDR_NAME, OBJECT_MODULE_NAME, ID_TYPE_NAME) =>
        {
            debug_assert!(targs.is_empty());
            true
        }
        Type_::Apply(_, sp!(_, n), targs)
            if n.is(STD_ADDR_NAME, OPTION_MODULE_NAME, OPTION_TYPE_NAME) =>
        {
            debug_assert!(targs.len() == 1);
            is_entry_primitive_ty(&targs[0])
        }

        // primitives
        Type_::Apply(_, sp!(_, N::Builtin(_)), targs) => {
            debug_assert!(targs.is_empty());
            true
        }

        // Non primitive
        Type_::Apply(_, sp!(_, N::ModuleType(_, _)), _) => false,
        Type_::Unit => false,

        // Error case nothing to do
        Type_::UnresolvedError | Type_::Anything | Type_::Var(_) => true,
    }
}

fn is_entry_object_ty(param_ty: &Type) -> bool {
    use BuiltinTypeName_ as B;
    use TypeName_ as N;
    match &param_ty.value {
        Type_::Ref(_, t) => is_entry_object_ty_inner(t),
        Type_::Apply(_, sp!(_, N::Builtin(sp!(_, B::Vector))), targs) => {
            debug_assert!(targs.len() == 1);
            is_entry_object_ty_inner(&targs[0])
        }
        _ => is_entry_object_ty_inner(param_ty),
    }
}

fn is_entry_object_ty_inner(param_ty: &Type) -> bool {
    use TypeName_ as N;
    match &param_ty.value {
        Type_::Param(tp) => tp.abilities.has_ability_(Ability_::Key),
        // nonsensical, but no error needed
        Type_::Apply(_, sp!(_, N::Multiple(_)), ts) => ts.iter().all(is_entry_object_ty_inner),
        // Simple recursive cases, shouldn't be hit but no need to error
        Type_::Ref(_, t) => is_entry_object_ty_inner(t),

        // Objects
        Type_::Apply(Some(abilities), _, _) => abilities.has_ability_(Ability_::Key),

        // Error case nothing to do
        Type_::UnresolvedError | Type_::Anything | Type_::Var(_) | Type_::Unit => true,
        // Unreachable cases
        Type_::Apply(None, _, _) => unreachable!("ICE abilities should have been expanded"),
    }
}

fn entry_return(
    context: &mut Context,
    entry_loc: Loc,
    name: FunctionName,
    return_type @ sp!(tloc, return_type_): &Type,
) {
    match return_type_ {
        // unit is fine, nothing to do
        Type_::Unit => (),
        Type_::Ref(_, _) => {
            let fmsg = format!("Invalid return type for entry function '{}'", name);
            let tmsg = "Expected a non-reference type";
            context.env.add_diag(diag!(
                ENTRY_FUN_SIGNATURE_DIAG,
                (entry_loc, fmsg),
                (*tloc, tmsg)
            ))
        }
        Type_::Param(tp) => {
            if !tp.abilities.has_ability_(Ability_::Drop) {
                let declared_loc_opt = Some(tp.user_specified_name.loc);
                let declared_abilities = tp.abilities.clone();
                invalid_entry_return_ty(
                    context,
                    entry_loc,
                    name,
                    return_type,
                    declared_loc_opt,
                    &declared_abilities,
                    std::iter::empty(),
                )
            }
        }
        Type_::Apply(Some(abilities), sp!(_, tn_), ty_args) => {
            if !abilities.has_ability_(Ability_::Drop) {
                let (declared_loc_opt, declared_abilities) = match tn_ {
                    TypeName_::Multiple(_) => (None, AbilitySet::collection(*tloc)),
                    TypeName_::ModuleType(m, n) => (
                        Some(context.info.struct_declared_loc(m, n)),
                        context.info.struct_declared_abilities(m, n).clone(),
                    ),
                    TypeName_::Builtin(b) => (None, b.value.declared_abilities(b.loc)),
                };
                invalid_entry_return_ty(
                    context,
                    entry_loc,
                    name,
                    return_type,
                    declared_loc_opt,
                    &declared_abilities,
                    ty_args.iter().map(|ty_arg| (ty_arg, get_abilities(ty_arg))),
                )
            }
        }
        // Error case nothing to do
        Type_::UnresolvedError | Type_::Anything | Type_::Var(_) => (),
        // Unreachable cases
        Type_::Apply(None, _, _) => unreachable!("ICE abilities should have been expanded"),
    }
}

fn get_abilities(sp!(loc, ty_): &Type) -> AbilitySet {
    use Type_ as T;
    let loc = *loc;
    match ty_ {
        T::UnresolvedError | T::Anything => AbilitySet::all(loc),
        T::Unit => AbilitySet::collection(loc),
        T::Ref(_, _) => AbilitySet::references(loc),
        T::Param(TParam { abilities, .. }) | Type_::Apply(Some(abilities), _, _) => {
            abilities.clone()
        }
        T::Var(_) | Type_::Apply(None, _, _) => {
            unreachable!("ICE abilities should have been expanded")
        }
    }
}

fn invalid_entry_return_ty<'a>(
    context: &mut Context,
    entry_loc: Loc,
    name: FunctionName,
    ty: &Type,
    declared_loc_opt: Option<Loc>,
    declared_abilities: &AbilitySet,
    ty_args: impl IntoIterator<Item = (&'a Type, AbilitySet)>,
) {
    let fmsg = format!("Invalid return type for entry function '{}'", name);
    let mut diag = diag!(ENTRY_FUN_SIGNATURE_DIAG, (entry_loc, fmsg));
    ability_not_satisfied_tips(
        &Subst::empty(),
        &mut diag,
        Ability_::Drop,
        ty,
        declared_loc_opt,
        declared_abilities,
        ty_args,
    );
    context.env.add_diag(diag)
}
