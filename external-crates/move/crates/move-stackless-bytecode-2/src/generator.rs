// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::stackless;

use move_binary_format::CompiledModule;
use move_model_2::{
    model::Model as Model2,
    source_kind::{SourceKind, WithoutSource},
};

use anyhow::Ok;

use std::collections::BTreeMap;

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

pub struct StacklessBytecodeGenerator<S: SourceKind> {
    pub(crate) model: Model2<S>,
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl StacklessBytecodeGenerator<WithoutSource> {
    pub fn new(modules: Vec<CompiledModule>) -> Self {
        Self {
            model: Model2::from_compiled(&BTreeMap::new(), modules),
        }
    }
}

impl<S: SourceKind> StacklessBytecodeGenerator<S> {
    pub fn from_model(model: Model2<S>) -> Self {
        Self { model }
    }

    pub fn generate_stackless_bytecode(
        &self,
        optimize: bool,
    ) -> anyhow::Result<Vec<stackless::ast::Package>> {
        stackless::translate::packages(&self.model, optimize)
    }

    pub fn execute(&self, optimize: bool) -> anyhow::Result<String> {
        let packages = self.generate_stackless_bytecode(optimize)?;
        let out_string = packages
            .iter()
            .map(|package| package.to_string())
            .collect::<Vec<String>>()
            .join("\n");
        Ok(out_string)
    }
}
