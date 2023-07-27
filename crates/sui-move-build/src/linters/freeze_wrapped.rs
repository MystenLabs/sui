// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This analysis flags freezing instances of structs containing, transitively or not, other structs
//! with the key ability.

use move_compiler::{
    diag,
    diagnostics::codes::{custom, DiagnosticInfo, Severity},
    expansion::ast as E,
    hlir::{ast as H, visitor::HlirVisitor},
    parser::ast as P,
    shared::{unique_map::UniqueMap, CompilationEnv, Identifier},
};
use move_ir_types::location::*;

use super::{LinterDiagCategory, LINTER_DEFAULT_DIAG_CODE, LINT_WARNING_PREFIX};

const FREEZE_KEY_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagCategory::FreezeWrapped as u8,
    LINTER_DEFAULT_DIAG_CODE,
    "attempting to freeze wrapped objects",
);

const FREEZE_FUNCTIONS: &[(&str, &str, &str)] = &[
    ("sui", "transfer", "public_freeze_object"),
    ("sui", "transfer", "freeze_object"),
];

pub struct FreezeWrappedVisitor;

impl HlirVisitor for FreezeWrappedVisitor {
    fn visit(&mut self, env: &mut CompilationEnv, program: &mut H::Program) {
        for (_, _, mdef) in program.modules.iter() {
            env.add_warning_filter_scope(mdef.warning_filter.clone());

            for (_, _, fdef) in mdef.functions.iter() {
                env.add_warning_filter_scope(fdef.warning_filter.clone());

                if let H::FunctionBody_::Defined { locals: _, body } = &fdef.body.value {
                    visit_block(body, env, &program.modules);
                }

                env.pop_warning_filter_scope();
            }

            env.pop_warning_filter_scope();
        }
    }
}

fn visit_stmt(
    sp!(_, stmt): &H::Statement,
    env: &mut CompilationEnv,
    modules: &UniqueMap<E::ModuleIdent, H::ModuleDefinition>,
) {
    use H::Statement_ as S;
    match stmt {
        S::Command(cmd) => visit_cmd(cmd, env, modules),
        S::IfElse {
            cond,
            if_block,
            else_block,
        } => {
            visit_exp(cond, env, modules);
            visit_block(if_block, env, modules);
            visit_block(else_block, env, modules);
        }
        S::While {
            cond: (c, b),
            block,
        } => {
            visit_block(c, env, modules);
            visit_exp(b, env, modules);
            visit_block(block, env, modules);
        }
        S::Loop {
            block,
            has_break: _,
        } => visit_block(block, env, modules),
    }
}

fn visit_block(
    block: &H::Block,
    env: &mut CompilationEnv,
    modules: &UniqueMap<E::ModuleIdent, H::ModuleDefinition>,
) {
    for s in block {
        visit_stmt(s, env, modules);
    }
}

fn visit_cmd(
    sp!(_, cmd): &H::Command,
    env: &mut CompilationEnv,
    modules: &UniqueMap<E::ModuleIdent, H::ModuleDefinition>,
) {
    use H::Command_ as C;
    match cmd {
        C::Assign(_, e) => visit_exp(e, env, modules),
        C::Mutate(e1, e2) => {
            visit_exp(e1, env, modules);
            visit_exp(e2, env, modules);
        }
        C::Abort(e) => visit_exp(e, env, modules),
        C::Return { from_user: _, exp } => visit_exp(exp, env, modules),
        C::IgnoreAndPop { pop_num: _, exp } => visit_exp(exp, env, modules),
        C::JumpIf {
            cond,
            if_true: _,
            if_false: _,
        } => visit_exp(cond, env, modules),
        C::Break | C::Continue | C::Jump { .. } => (),
    }
}

fn visit_exp(
    exp: &H::Exp,
    env: &mut CompilationEnv,
    modules: &UniqueMap<E::ModuleIdent, H::ModuleDefinition>,
) {
    use H::UnannotatedExp_ as E;
    let sp!(_, uexp) = &exp.exp;
    match uexp {
        E::ModuleCall(fun) => {
            if FREEZE_FUNCTIONS
                .iter()
                .any(|(addr, module, fname)| fun.is(addr, module, fname))
            {
                // single argument passed by value
                let H::Type_::Single(st) = &fun.arguments.ty.value else {
                    return;
                };
                let bt = match &st.value {
                    H::SingleType_::Base(bt) => bt,
                    H::SingleType_::Ref(_, bt) => bt,
                };
                let H::BaseType_::Apply(_, tname ,_ ) = &bt.value else {
                    return;
                };
                if let H::TypeName_::ModuleType(mident, sname) = tname.value {
                    if let Some((sfields, sloc)) = struct_fields(&sname, &mident, modules) {
                        for (f, t) in sfields {
                            if let Some((nested_sname, nested)) =
                                contains_key(t, modules, /*field_depth*/ 0)
                            {
                                let msg = "Freezing an object containing other wrapped objects will disallow unwrapping these objects in the future.";
                                let uid_msg = format!(
                                    "The field {} of {} contains {} wrapped objects",
                                    f.value(),
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
                                        nested_sname.loc(),
                                        "Indirectly wrapped object is of this type",
                                    ));
                                }
                                env.add_diag(d);
                            }
                        }
                    }
                }
            }
        }
        E::Builtin(_, e) => visit_exp(e, env, modules),
        E::Freeze(e) => visit_exp(e, env, modules),
        E::Vector(_, _, _, e) => visit_exp(e, env, modules),
        E::Dereference(e) => visit_exp(e, env, modules),
        E::UnaryExp(_, e) => visit_exp(e, env, modules),
        E::BinopExp(e1, _, e2) => {
            visit_exp(e1, env, modules);
            visit_exp(e2, env, modules);
        }
        E::Pack(_, _, fields) => fields
            .iter()
            .for_each(|(_, _, e)| visit_exp(e, env, modules)),
        E::ExpList(list) => {
            for l in list {
                match l {
                    H::ExpListItem::Single(e, _) => visit_exp(e, env, modules),
                    H::ExpListItem::Splat(_, e, _) => visit_exp(e, env, modules),
                }
            }
        }
        E::Borrow(_, e, _) => visit_exp(e, env, modules),
        E::Cast(e, _) => visit_exp(e, env, modules),
        E::Unit { .. }
        | E::Value(_)
        | E::Move { .. }
        | E::Copy { .. }
        | E::Constant(..)
        | E::BorrowLocal(..)
        | E::Unreachable
        | E::Spec(..)
        | E::UnresolvedError => (),
    }
}

fn struct_fields<'a>(
    sname: &P::StructName,
    mident: &E::ModuleIdent,
    modules: &'a UniqueMap<E::ModuleIdent, H::ModuleDefinition>,
) -> Option<(&'a Vec<(P::Field, H::BaseType)>, Loc)> {
    if let Some(mdef) = modules.get(mident) {
        if let Some(sdef) = mdef.structs.get(sname) {
            if let H::StructFields::Defined(sfields) = &sdef.fields {
                // unwrap is safe since we know that mdef.structs.get succeeded
                return Some((sfields, *mdef.structs.get_loc(sname).unwrap()));
            }
        }
    }
    None
}

fn contains_key(
    sp!(_, bt): &H::BaseType,
    modules: &UniqueMap<E::ModuleIdent, H::ModuleDefinition>,
    field_depth: usize,
) -> Option<(P::StructName, bool)> {
    let H::BaseType_::Apply(ability_set, tname ,_ ) = bt else {
        return None;
    };

    let sp!(_, tname) = tname;
    if let H::TypeName_::ModuleType(mident, sname) = tname {
        // don't have to check all variants of H::TypeName_ as only H::TypeName_::ModuleType can be
        // a struct or have abilities
        if let Some((sfields, sloc)) = struct_fields(sname, mident, modules) {
            // we could take out the ability set check out of the if condition but it should not
            // matter as only struct can have abilities defined on them and having it here allows us
            // to return the location of the struct type (rather than the location of struct
            // name)
            if ability_set.has_ability_(P::Ability_::Key) {
                return Some((
                    P::StructName(sp(sloc, (*sname.value()).into())),
                    field_depth > 0,
                ));
            }
            return sfields
                .iter()
                .find_map(|(_, ftype)| contains_key(ftype, modules, field_depth + 1));
        }
    }
    None
}
