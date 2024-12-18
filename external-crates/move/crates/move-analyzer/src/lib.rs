// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#[macro_use(sp)]
extern crate move_ir_types;

pub mod analysis;
pub mod analyzer;
pub mod compiler_info;
pub mod completions;
pub mod context;
pub mod diagnostics;
pub mod inlay_hints;
pub mod symbols;
pub mod utils;
pub mod vfs;
