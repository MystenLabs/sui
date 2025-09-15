// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{ast, translate};

use move_model_2::{model::Model, source_kind::SourceKind};
use move_stackless_bytecode_2::ast as SB;

pub fn module(module_: SB::Module) -> crate::ast::Module {
    crate::translate::module(module_)
}
