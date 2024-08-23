// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod ast;
pub mod core;
mod dependency_ordering;
mod deprecation_warnings;
mod expand;
mod infinite_instantiations;
mod macro_expand;
mod match_analysis;
mod recursive_datatypes;
mod syntax_methods;
pub(crate) mod translate;
pub mod visitor;
