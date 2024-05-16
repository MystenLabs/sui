// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod address;
mod alias_map_builder;
mod aliases;
pub mod ast;
pub(crate) mod fake_natives;
mod legacy_aliases;
mod name_resolver;
pub(crate) mod resolve_use_funs;
pub(crate) mod syntax_methods;
pub(crate) mod translate;
