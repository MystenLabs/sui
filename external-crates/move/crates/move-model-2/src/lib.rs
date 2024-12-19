// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

#[macro_use(sp)]
extern crate move_ir_types;

pub mod compiled_model;
pub mod display;
pub mod source_model;

pub use compiled_model::ModuleId;
pub use compiled_model::QualifiedMemberId;
pub use compiled_model::TModuleId;
