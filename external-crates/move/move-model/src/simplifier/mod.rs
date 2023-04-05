// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

use move_binary_format::file_format::CodeOffset;

use crate::{
    ast::Spec,
    model::{FunId, GlobalEnv, ModuleId, QualifiedId},
};

mod pass;
mod pass_inline;

pub use pass::SpecRewriter;
use pass_inline::SpecPassInline;

/// Available simplifications passes to run after tbe model is built
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SimplificationPass {
    Inline,
}

impl FromStr for SimplificationPass {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let r = match s {
            "inline" => SimplificationPass::Inline,
            _ => return Err(s.to_string()),
        };
        Ok(r)
    }
}

impl fmt::Display for SimplificationPass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inline => write!(f, "inline"),
        }
    }
}

/// A rewriter pipeline that is composed of a chain of spec rewriters. Note that this composite
/// rewriter is also a spec rewriter.
pub struct SpecRewriterPipeline {
    rewriters: Vec<Box<dyn SpecRewriter>>,
}

impl SpecRewriterPipeline {
    /// Construct a pipeline rewriter by a list of passes
    pub fn new(pipeline: &[SimplificationPass]) -> Self {
        let mut result = Self { rewriters: vec![] };
        for entry in pipeline {
            match entry {
                SimplificationPass::Inline => {
                    result.rewriters.push(Box::new(SpecPassInline::default()))
                }
            }
        }
        result
    }
}

impl SpecRewriter for SpecRewriterPipeline {
    fn rewrite_module_spec(
        &mut self,
        env: &GlobalEnv,
        module_id: ModuleId,
        spec: &Spec,
    ) -> Result<Option<Spec>> {
        let mut current_spec = None;
        for rewriter in self.rewriters.iter_mut() {
            if let Some(new_spec) = rewriter.rewrite_module_spec(
                env,
                module_id,
                current_spec.as_ref().unwrap_or(spec),
            )? {
                current_spec = Some(new_spec);
            }
        }
        Ok(current_spec)
    }

    fn rewrite_function_spec(
        &mut self,
        env: &GlobalEnv,
        fun_id: QualifiedId<FunId>,
        spec: &Spec,
    ) -> Result<Option<Spec>> {
        let mut current_spec = None;
        for rewriter in self.rewriters.iter_mut() {
            if let Some(new_spec) = rewriter.rewrite_function_spec(
                env,
                fun_id,
                current_spec.as_ref().unwrap_or(spec),
            )? {
                current_spec = Some(new_spec);
            }
        }
        Ok(current_spec)
    }

    fn rewrite_inline_spec(
        &mut self,
        env: &GlobalEnv,
        fun_id: QualifiedId<FunId>,
        code_offset: CodeOffset,
        spec: &Spec,
    ) -> Result<Option<Spec>> {
        let mut current_spec = None;
        for rewriter in self.rewriters.iter_mut() {
            if let Some(new_spec) = rewriter.rewrite_inline_spec(
                env,
                fun_id,
                code_offset,
                current_spec.as_ref().unwrap_or(spec),
            )? {
                current_spec = Some(new_spec);
            }
        }
        Ok(current_spec)
    }
}
