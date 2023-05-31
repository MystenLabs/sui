// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod absint;
pub mod ast;
mod borrows;
pub(crate) mod cfg;
mod liveness;
mod locals;
mod remove_no_ops;
pub(crate) mod translate;
pub mod visitor;

mod optimize;

use crate::{
    diagnostics::Diagnostics,
    expansion::ast::{AbilitySet, ModuleIdent},
    hlir::ast::*,
    parser::ast::StructName,
    shared::{unique_map::UniqueMap, CompilationEnv, Name},
};
use cfg::*;
use move_ir_types::location::*;
use optimize::optimize;
use std::collections::{BTreeMap, BTreeSet};

pub struct CFGContext<'a> {
    pub module: Option<ModuleIdent>,
    pub member: MemberName,
    pub struct_declared_abilities: &'a UniqueMap<ModuleIdent, UniqueMap<StructName, AbilitySet>>,
    pub signature: &'a FunctionSignature,
    pub acquires: &'a BTreeMap<StructName, Loc>,
    pub locals: &'a UniqueMap<Var, SingleType>,
    pub infinite_loop_starts: &'a BTreeSet<Label>,
}

pub enum MemberName {
    Constant(Name),
    Function(Name),
}

pub fn refine_inference_and_verify(
    env: &mut CompilationEnv,
    context: &CFGContext,
    cfg: &mut BlockCFG,
) {
    liveness::last_usage(env, context, cfg);
    let locals_states = locals::verify(env, context, cfg);

    liveness::release_dead_refs(context, &locals_states, cfg);
    borrows::verify(env, context, cfg);
    let mut ds = Diagnostics::new();
    for visitor in &env.visitors().abs_int {
        let mut f = visitor.borrow_mut();
        ds.extend(f(context, cfg));
    }
    env.add_diags(ds)
}

impl MemberName {
    pub fn name(&self) -> Name {
        match self {
            MemberName::Constant(n) | MemberName::Function(n) => *n,
        }
    }
}
