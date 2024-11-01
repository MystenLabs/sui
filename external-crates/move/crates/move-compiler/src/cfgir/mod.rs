// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod absint;
pub mod ast;
mod borrows;
pub mod cfg;
mod liveness;
mod locals;
mod remove_no_ops;
pub(crate) mod translate;
pub mod visitor;

mod optimize;

use crate::{
    diagnostics::DiagnosticReporter,
    expansion::ast::{Attributes, ModuleIdent, Mutability},
    hlir::ast::{FunctionSignature, Label, SingleType, Var, Visibility},
    shared::{program_info::TypingProgramInfo, unique_map::UniqueMap, CompilationEnv, Name},
};
use cfg::*;
use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;
use optimize::optimize;
use std::collections::BTreeSet;

pub struct CFGContext<'a> {
    pub env: &'a CompilationEnv,
    pub reporter: &'a DiagnosticReporter<'a>,
    pub info: &'a TypingProgramInfo,
    pub package: Option<Symbol>,
    pub module: ModuleIdent,
    pub member: MemberName,
    pub attributes: &'a Attributes,
    pub entry: Option<Loc>,
    pub visibility: Visibility,
    pub signature: &'a FunctionSignature,
    pub locals: &'a UniqueMap<Var, (Mutability, SingleType)>,
    pub infinite_loop_starts: &'a BTreeSet<Label>,
}

pub enum MemberName {
    Constant(Name),
    Function(Name),
}

pub fn refine_inference_and_verify(context: &CFGContext, cfg: &mut MutForwardCFG) {
    liveness::last_usage(context, cfg);
    let locals_states = locals::verify(context, cfg);

    liveness::release_dead_refs(context, &locals_states, cfg);
    borrows::verify(context, cfg);
}

impl CFGContext<'_> {
    fn add_diag(&self, diag: crate::diagnostics::Diagnostic) {
        self.reporter.add_diag(diag);
    }

    fn add_diags(&self, diags: crate::diagnostics::Diagnostics) {
        self.reporter.add_diags(diags);
    }
}

impl MemberName {
    pub fn name(&self) -> Name {
        match self {
            MemberName::Constant(n) | MemberName::Function(n) => *n,
        }
    }
}
