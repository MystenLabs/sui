// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This analysis flags freezing instances of structs containing, transitively or not, other structs
//! with the key ability.

use move_compiler::{
    diag,
    diagnostics::codes::{custom, DiagnosticInfo, Severity},
    expansion::ast as E,
    naming::ast as N,
    parser::ast as P,
    shared::{unique_map::UniqueMap, CompilationEnv, Identifier, Name},
    typing::{ast as T, core::ProgramInfo, visitor::TypingVisitor},
};
use move_ir_types::location::*;

use super::{
    LinterDiagCategory, FREEZE_FUN, LINTER_DEFAULT_DIAG_CODE, LINT_WARNING_PREFIX,
    PUBLIC_FREEZE_FUN, SUI_PKG_NAME, TRANSFER_MOD_NAME,
};

const FREEZE_KEY_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagCategory::FreezeWrapped as u8,
    LINTER_DEFAULT_DIAG_CODE,
    "attempting to freeze wrapped objects",
);

const FREEZE_FUNCTIONS: &[(&str, &str, &str)] = &[
    (SUI_PKG_NAME, TRANSFER_MOD_NAME, PUBLIC_FREEZE_FUN),
    (SUI_PKG_NAME, TRANSFER_MOD_NAME, FREEZE_FUN),
];

pub struct FreezeWrappedVisitor;

impl TypingVisitor for FreezeWrappedVisitor {
    fn visit(
        &mut self,
        env: &mut CompilationEnv,
        _program_info: &ProgramInfo,
        program: &mut T::Program,
    ) {
        for (_, _, mdef) in program.modules.iter() {
            env.add_warning_filter_scope(mdef.warning_filter.clone());

            for (_, _, fdef) in mdef.functions.iter() {
                env.add_warning_filter_scope(fdef.warning_filter.clone());

                if let T::FunctionBody_::Defined(seq) = &fdef.body.value {
                    visit_seq(seq, env, &program.modules);
                }

                env.pop_warning_filter_scope();
            }

            env.pop_warning_filter_scope();
        }
    }
}

fn visit_seq_item(
    sp!(_, seq_item): &T::SequenceItem,
    env: &mut CompilationEnv,
    modules: &UniqueMap<E::ModuleIdent, T::ModuleDefinition>,
) {
    use T::SequenceItem_ as SI;
    match seq_item {
        SI::Seq(e) => visit_exp(e, env, modules),
        SI::Declare(_) => (),
        SI::Bind(_, _, e) => visit_exp(e, env, modules),
    }
}

fn visit_exp(
    exp: &T::Exp,
    env: &mut CompilationEnv,
    modules: &UniqueMap<E::ModuleIdent, T::ModuleDefinition>,
) {
    use T::UnannotatedExp_ as E;
    let sp!(_, uexp) = &exp.exp;
    match uexp {
        E::ModuleCall(fun) => {
            if FREEZE_FUNCTIONS.iter().any(|(addr, module, fname)| {
                fun.module.value.is(*addr, *module) && &fun.name.value().as_str() == fname
            }) {
                let Some(bt) = base_type(&fun.type_arguments[0]) else {
                    // not an (potentially dereferenced) N::Type_::Apply nor N::Type_::Param
                    return;
                };
                let N::Type_::Apply(_,tname, _) = &bt.value else {
                    // not a struct type
                    return;
                };
                let N::TypeName_::ModuleType(mident, sname) = tname.value else {
                    // struct with a given name not found
                    return;
                };
                let Some((sfields, sloc)) = struct_fields(&sname, &mident, modules) else {
                    // fields for a given struct could not be located
                    return;
                };
                for (_, f, (_, t)) in sfields {
                    if let Some((nested_tname, nested)) =
                        contains_key(t, modules, /*field_depth*/ 0)
                    {
                        let msg = "Freezing an object containing other objects will prevent the wrapped objects from being unwrapped in the future.";
                        let uid_msg = format!(
                            "The field '{}' of '{}' contains {} wrapped objects",
                            f,
                            sname.value(),
                            if nested { "indirectly" } else { "" }
                        );
                        let mut d = diag!(
                            FREEZE_KEY_DIAG,
                            (fun.arguments.exp.loc, msg),
                            (sloc, uid_msg)
                        );

                        if nested {
                            d.add_secondary_label((
                                nested_tname.loc,
                                "Indirectly wrapped object is of this type",
                            ));
                        }
                        env.add_diag(d);
                    }
                }
            }
        }
        E::Builtin(_, e) => visit_exp(e, env, modules),
        E::Vector(_, _, _, e) => visit_exp(e, env, modules),
        E::IfElse(e1, e2, e3) => {
            visit_exp(e1, env, modules);
            visit_exp(e2, env, modules);
            visit_exp(e3, env, modules);
        }
        E::While(e1, e2) => {
            visit_exp(e1, env, modules);
            visit_exp(e2, env, modules);
        }
        E::Loop { has_break: _, body } => visit_exp(body, env, modules),
        E::Block(seq) => visit_seq(seq, env, modules),
        E::Assign(_, _, e) => visit_exp(e, env, modules),
        E::Mutate(e1, e2) => {
            visit_exp(e1, env, modules);
            visit_exp(e2, env, modules);
        }
        E::Return(e) => visit_exp(e, env, modules),
        E::Abort(e) => visit_exp(e, env, modules),
        E::Dereference(e) => visit_exp(e, env, modules),
        E::UnaryExp(_, e) => visit_exp(e, env, modules),
        E::BinopExp(e1, _, _, e2) => {
            visit_exp(e1, env, modules);
            visit_exp(e2, env, modules);
        }
        E::Pack(_, _, _, fields) => fields
            .iter()
            .for_each(|(_, _, (_, (_, e)))| visit_exp(e, env, modules)),
        E::ExpList(list) => {
            for l in list {
                match l {
                    T::ExpListItem::Single(e, _) => visit_exp(e, env, modules),
                    T::ExpListItem::Splat(_, e, _) => visit_exp(e, env, modules),
                }
            }
        }
        E::Borrow(_, e, _) => visit_exp(e, env, modules),
        E::TempBorrow(_, e) => visit_exp(e, env, modules),
        E::Cast(e, _) => visit_exp(e, env, modules),
        E::Annotate(e, _) => visit_exp(e, env, modules),
        E::Unit { .. }
        | E::Value(_)
        | E::Move { .. }
        | E::Copy { .. }
        | E::Use(_)
        | E::Constant(..)
        | E::Break
        | E::Continue
        | E::BorrowLocal(..)
        | E::Spec(..)
        | E::UnresolvedError => (),
    }
}

fn visit_seq(
    seq: &T::Sequence,
    env: &mut CompilationEnv,
    modules: &UniqueMap<E::ModuleIdent, T::ModuleDefinition>,
) {
    for s in seq {
        visit_seq_item(s, env, modules);
    }
}

fn struct_fields<'a>(
    sname: &P::StructName,
    mident: &E::ModuleIdent,
    modules: &'a UniqueMap<E::ModuleIdent, T::ModuleDefinition>,
) -> Option<(&'a E::Fields<N::Type>, Loc)> {
    if let Some(mdef) = modules.get(mident) {
        if let Some(sdef) = mdef.structs.get(sname) {
            if let N::StructFields::Defined(sfields) = &sdef.fields {
                // unwrap is safe since we know that mdef.structs.get succeeded
                return Some((sfields, *mdef.structs.get_loc(sname).unwrap()));
            }
        }
    }
    None
}

fn contains_key(
    t: &N::Type,
    modules: &UniqueMap<E::ModuleIdent, T::ModuleDefinition>,
    field_depth: usize,
) -> Option<(Name, bool)> {
    use N::Type_ as T;
    let Some(bt) = base_type(t) else{
        return None;
    };
    let sp!(_, bt) = bt;
    match bt {
        T::Param(p) => {
            if p.abilities.has_ability_(P::Ability_::Key) {
                return Some((p.user_specified_name, field_depth > 0));
            }
            None
        }
        T::Apply(abilities, tname, _) => {
            if let N::TypeName_::ModuleType(mident, sname) = tname.value {
                // don't have to check all variants of H::TypeName_ as only H::TypeName_::ModuleType
                // can be a struct or have abilities
                if let Some((sfields, sloc)) = struct_fields(&sname, &mident, modules) {
                    // we could take out the ability set check out of the if condition but it should
                    // not matter as only struct can have abilities defined on them and having it
                    // here allows us to return the location of the struct type (rather than the
                    // location of struct name)
                    if let Some(ability_set) = abilities {
                        if ability_set.has_ability_(P::Ability_::Key) {
                            return Some((sp(sloc, (*sname.value()).into()), field_depth > 0));
                        }
                    }
                    return sfields.iter().find_map(|(_, _, (_, ftype))| {
                        contains_key(ftype, modules, field_depth + 1)
                    });
                }
            }
            None
        }
        T::Unit | T::Ref(_, _) | T::Var(_) | T::Anything | T::UnresolvedError => None,
    }
}

fn base_type(t: &N::Type) -> Option<&N::Type> {
    use N::Type_ as T;
    match &t.value {
        T::Ref(_, inner_t) => base_type(inner_t),
        T::Apply(_, _, _) | T::Param(_) => Some(t),
        T::Unit | T::Var(_) | T::Anything | T::UnresolvedError => None,
    }
}
