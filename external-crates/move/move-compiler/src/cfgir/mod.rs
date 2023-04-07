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

mod optimize;

use crate::{
    expansion::ast::{AbilitySet, ModuleIdent},
    hlir::ast::*,
    parser::ast::StructName,
    shared::{unique_map::UniqueMap, CompilationEnv},
};
use cfg::*;
use move_ir_types::location::*;
use optimize::optimize;
use std::collections::{BTreeMap, BTreeSet};

pub fn refine_inference_and_verify(
    compilation_env: &mut CompilationEnv,
    struct_declared_abilities: &UniqueMap<ModuleIdent, UniqueMap<StructName, AbilitySet>>,
    signature: &FunctionSignature,
    acquires: &BTreeMap<StructName, Loc>,
    locals: &UniqueMap<Var, SingleType>,
    cfg: &mut BlockCFG,
    infinite_loop_starts: &BTreeSet<Label>,
) {
    liveness::last_usage(compilation_env, locals, cfg, infinite_loop_starts);
    let locals_states = locals::verify(
        compilation_env,
        struct_declared_abilities,
        signature,
        acquires,
        locals,
        cfg,
    );

    liveness::release_dead_refs(&locals_states, locals, cfg, infinite_loop_starts);
    borrows::verify(compilation_env, signature, acquires, locals, cfg);
}
