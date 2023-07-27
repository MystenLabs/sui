// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::command_line::compiler::Visitor;
use crate::hlir::ast as H;
use crate::shared::CompilationEnv;

pub type HlirVisitorObj = Box<dyn HlirVisitor>;

pub trait HlirVisitor {
    fn visit(&mut self, env: &mut CompilationEnv, program: &mut H::Program);

    fn visitor(self) -> Visitor
    where
        Self: 'static + Sized,
    {
        Visitor::HlirVisitor(Box::new(self))
    }
}

impl<V: HlirVisitor + 'static> From<V> for HlirVisitorObj {
    fn from(value: V) -> Self {
        Box::new(value)
    }
}
