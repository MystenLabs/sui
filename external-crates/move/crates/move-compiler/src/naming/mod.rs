// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod address;
pub mod ast;
pub(crate) mod fake_natives;
pub(crate) mod resolve_use_funs;
pub(crate) mod syntax_methods;
mod name_resolver;
mod aliases;
mod alias_map_builder;
mod legacy_aliases;
pub(crate) mod translate;
