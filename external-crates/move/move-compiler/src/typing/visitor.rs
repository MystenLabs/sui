// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::expansion::ast::ModuleIdent;
use crate::shared::unique_map::UniqueMap;
use crate::shared::CompilationEnv;
use crate::typing::{ast as T, core::ModuleInfo};

pub type TypingVisitorObj = Box<dyn TypingVisitor>;

pub trait TypingVisitor {
    fn visit(
        &mut self,
        env: &mut CompilationEnv,
        module_info: &UniqueMap<ModuleIdent, ModuleInfo>,
        program: &mut T::Program,
    );
}
