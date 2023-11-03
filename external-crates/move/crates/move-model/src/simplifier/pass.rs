// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use std::collections::BTreeMap;

use move_binary_format::file_format::CodeOffset;

use crate::{
    ast::Spec,
    model::{FunId, GlobalEnv, ModuleId, QualifiedId},
};

/// A generic trait for rewriting the specifications in the `GlobalEnv`. A rewriter is expected to
/// implement at least one `rewrite_*` function, depending on which type(s) of specs the rewriter
/// targets. All the `rewrite_*` function should follow the convention on return value:
/// - `Ok(None)`           --> nothing to rewrite on this spec
/// - `Ok(Some(new_spec))` --> the spec is re-written and the `new_spec` is the output
/// - `Err(..)`            --> something wrong (invariant violation) happened in the rewriting
pub trait SpecRewriter {
    /// Rewrite a module-level specification
    fn rewrite_module_spec(
        &mut self,
        _env: &GlobalEnv,
        _module_id: ModuleId,
        _spec: &Spec,
    ) -> Result<Option<Spec>> {
        Ok(None)
    }

    /// Rewrite a function-level specification
    fn rewrite_function_spec(
        &mut self,
        _env: &GlobalEnv,
        _fun_id: QualifiedId<FunId>,
        _spec: &Spec,
    ) -> Result<Option<Spec>> {
        Ok(None)
    }

    /// Rewrite a code-level specification
    fn rewrite_inline_spec(
        &mut self,
        _env: &GlobalEnv,
        _fun_id: QualifiedId<FunId>,
        _code_offset: CodeOffset,
        _spec: &Spec,
    ) -> Result<Option<Spec>> {
        Ok(None)
    }

    /// Iterate over the specs in the `GlobalEnv`, rewrite each spec, and apply changes back to the
    /// `GlobalEnv`.
    fn override_with_rewrite(&mut self, env: &mut GlobalEnv) -> Result<()> {
        // convert all module specs found in the model
        let mut new_specs = BTreeMap::new();
        for menv in env.get_modules() {
            let mid = menv.get_id();
            if let Some(new_spec) = self.rewrite_module_spec(env, mid, menv.get_spec())? {
                new_specs.insert(mid, new_spec);
            }
        }
        for (mid, spec) in new_specs {
            env.override_module_spec(mid, spec);
        }

        // convert all functional specs found in the model
        let mut new_specs = BTreeMap::new();
        for menv in env.get_modules() {
            for fenv in menv.get_functions() {
                let fid = fenv.get_qualified_id();
                if let Some(new_spec) = self.rewrite_function_spec(env, fid, fenv.get_spec())? {
                    new_specs.insert(fid, new_spec);
                }
            }
        }
        for (fid, spec) in new_specs {
            env.override_function_spec(fid, spec);
        }

        // convert all code-level specs found in the model
        let mut new_specs = BTreeMap::new();
        for menv in env.get_modules() {
            for fenv in menv.get_functions() {
                let fid = fenv.get_qualified_id();
                for (offset, spec) in &fenv.get_spec().on_impl {
                    if let Some(new_spec) = self.rewrite_inline_spec(env, fid, *offset, spec)? {
                        new_specs.insert((fid, *offset), new_spec);
                    }
                }
            }
        }
        for ((fid, offset), spec) in new_specs {
            env.override_inline_spec(fid, offset, spec);
        }
        Ok(())
    }
}
