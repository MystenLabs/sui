// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

#[macro_use(sp)]
extern crate move_ir_types;

pub mod compiled;
pub mod display;
pub mod model;

pub use compiled::ModuleId;
pub use compiled::QualifiedMemberId;
pub use compiled::TModuleId;
