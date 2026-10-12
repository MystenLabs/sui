// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_symbol_pool::Symbol;

use std::collections::BTreeMap;

use crate::{
    diagnostics::DiagnosticReporter,
    expansion::ast::{ModuleIdent, Mutability},
    hlir::ast::{Exp, FunctionSignature, SingleType, UnannotatedExp_, Value, Value_, Var},
    parser::ast::ConstantName,
    shared::{CompilationEnv, unique_map::UniqueMap},
};

pub struct Context<'a> {
    pub env: &'a CompilationEnv,
    pub reporter: &'a DiagnosticReporter<'a>,
    package: Option<Symbol>,
    pub signature: &'a FunctionSignature,
    #[allow(dead_code)]
    pub locals: &'a UniqueMap<Var, (Mutability, SingleType)>,
    pub constants: &'a BTreeMap<(ModuleIdent, ConstantName), Value>,
}

impl<'a> Context<'a> {
    pub fn new(
        env: &'a CompilationEnv,
        reporter: &'a DiagnosticReporter<'a>,
        package: Option<Symbol>,
        signature: &'a FunctionSignature,
        locals: &'a UniqueMap<Var, (Mutability, SingleType)>,
        constants: &'a BTreeMap<(ModuleIdent, ConstantName), Value>,
    ) -> Self {
        Self {
            env,
            reporter,
            package,
            signature,
            locals,
            constants,
        }
    }

    pub fn package(&self) -> Option<Symbol> {
        self.package
    }

    /// The value of `e` if it is statically known, i.e. a `Value` or a `Constant` with a known
    /// value.
    pub fn foldable_exp<'b>(&self, e: &'b Exp) -> Option<&'b Value_>
    where
        'a: 'b,
    {
        use UnannotatedExp_ as E;
        match &e.exp.value {
            E::Constant(module, name) => self.constants.get(&(*module, *name)).map(|sp!(_, v_)| v_),
            E::Value(sp!(_, v_)) => Some(v_),
            _ => None,
        }
    }
}
