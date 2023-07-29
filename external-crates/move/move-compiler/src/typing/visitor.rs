// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::command_line::compiler::Visitor;
use crate::shared::CompilationEnv;
use crate::typing::{ast as T, core::ProgramInfo};

pub type TypingVisitorObj = Box<dyn TypingVisitor>;

pub trait TypingVisitor {
    fn visit(
        &mut self,
        env: &mut CompilationEnv,
        program_info: &ProgramInfo,
        program: &mut T::Program,
    );

    fn visitor(self) -> Visitor
    where
        Self: 'static + Sized,
    {
        Visitor::TypingVisitor(Box::new(self))
    }
}

impl<V: TypingVisitor + 'static> From<V> for TypingVisitorObj {
    fn from(value: V) -> Self {
        Box::new(value)
    }
}
