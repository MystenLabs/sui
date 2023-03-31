// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use move_binary_format::file_format::CompiledModule;
use move_bytecode_utils::Modules;
use move_core_types::{
    identifier::{IdentStr, Identifier},
    language_storage::ModuleId,
};
use move_model::model::{FunctionEnv, GlobalEnv};
use move_read_write_set_types::ReadWriteSet;
use prover_bytecode::{
    function_target_pipeline::{FunctionTargetPipeline, FunctionTargetsHolder, FunctionVariant},
    read_write_set_analysis::{ReadWriteSetProcessor, ReadWriteSetState},
};
use read_write_set_dynamic::NormalizedReadWriteSetAnalysis;
use std::collections::BTreeMap;

pub struct ReadWriteSetAnalysis {
    targets: FunctionTargetsHolder,
    env: GlobalEnv,
}

/// Infer read/write set results for `modules`.
/// The `modules` list must be topologically sorted by the dependency relation
/// (i.e., a child node in the dependency graph should appear earlier in the
/// vector than its parents), and all dependencies of each module must be
/// included.
pub fn analyze<'a>(
    modules: impl IntoIterator<Item = &'a CompiledModule>,
) -> Result<ReadWriteSetAnalysis> {
    let module_map = Modules::new(modules);
    let dep_graph = module_map.compute_dependency_graph();
    let topo_order = dep_graph.compute_topological_order()?;
    analyze_sorted(topo_order)
}

/// Like analyze_unsorted, but assumes that `modules` is already topologically sorted
pub fn analyze_sorted<'a>(
    modules: impl IntoIterator<Item = &'a CompiledModule>,
) -> Result<ReadWriteSetAnalysis> {
    let env = move_model::run_bytecode_model_builder(modules)?;
    let mut pipeline = FunctionTargetPipeline::default();
    pipeline.add_processor(ReadWriteSetProcessor::new());
    let mut targets = FunctionTargetsHolder::default();
    for module_env in env.get_modules() {
        for func_env in module_env.get_functions() {
            targets.add_target(&func_env)
        }
    }
    pipeline.run(&env, &mut targets);

    Ok(ReadWriteSetAnalysis { targets, env })
}

impl ReadWriteSetAnalysis {
    /// Return an overapproximation access paths read/written by `module`::`fun`.
    /// Returns `None` if the function or module does not exist.
    pub fn get_summary(&self, module: &ModuleId, fun: &IdentStr) -> Option<&ReadWriteSetState> {
        self.get_function_env(module, fun).and_then(|fenv| {
            self.targets
                .get_data(&fenv.get_qualified_id(), &FunctionVariant::Baseline)
                .and_then(|data| data.annotations.get::<ReadWriteSetState>())
        })
    }

    /// Returns the FunctionEnv for `module`::`fun`
    /// Returns `None` if this function does not exist
    pub fn get_function_env(&self, module: &ModuleId, fun: &IdentStr) -> Option<FunctionEnv> {
        self.env
            .find_function_by_language_storage_id_name(module, fun)
    }

    /// Normalize the analysis result computed from the move-prover pipeline.
    ///
    /// This will include all the script functions and the list function names provided in `add_ons`
    /// that are required by the adapter, such as prologues, epilogues, etc.
    pub fn normalize_all_scripts(
        &self,
        add_ons: Vec<(ModuleId, Identifier)>,
    ) -> NormalizedReadWriteSetAnalysis {
        let mut result: BTreeMap<ModuleId, BTreeMap<Identifier, ReadWriteSet>> = BTreeMap::new();
        for module in self.env.get_modules() {
            let module_id = module.get_verified_module().self_id();
            let module_entry = result.entry(module_id.clone()).or_default();
            for func in module.get_functions() {
                let func_name = func.get_identifier();
                if func.is_script() || add_ons.contains(&(module_id.clone(), func_name.clone())) {
                    module_entry.insert(
                        func.get_identifier(),
                        self.targets
                            .get_data(&func.get_qualified_id(), &FunctionVariant::Baseline)
                            .and_then(|data| data.annotations.get::<ReadWriteSetState>())
                            .unwrap()
                            .normalize(&self.env),
                    );
                }
            }
        }
        NormalizedReadWriteSetAnalysis::new(result)
    }
}
