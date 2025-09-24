// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_stackless_bytecode_2::ast as SB;

pub fn module(module_: SB::Module) -> crate::ast::Module {
    crate::translate::module(module_)
}
