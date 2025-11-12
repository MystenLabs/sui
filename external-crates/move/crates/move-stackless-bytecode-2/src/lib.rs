// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::CompiledModule;
use move_model_2::{
    model::Model,
    source_kind::{SourceKind, WithoutSource},
};

use std::collections::BTreeMap;

pub mod ast;
pub(crate) mod optimizations;
pub mod translate;
pub(crate) mod utils;

// -------------------------------------------------------------------------------------------------
// Public API

pub fn from_compiled_modules(
    modules: Vec<CompiledModule>,
    optimize: bool,
) -> anyhow::Result<(
    move_model_2::model::Model<WithoutSource>,
    ast::StacklessBytecode,
)> {
    let model =
        move_model_2::model::Model::<WithoutSource>::from_compiled(&BTreeMap::new(), modules);
    let packages = translate::packages(&model, optimize)?;
    Ok((model, ast::StacklessBytecode { packages }))
}

pub fn from_model<S: SourceKind>(
    model: &Model<S>,
    optimize: bool,
) -> anyhow::Result<ast::StacklessBytecode> {
    let packages = translate::packages(model, optimize)?;
    Ok(ast::StacklessBytecode { packages })
}
