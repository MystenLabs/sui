// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use crate::function_target_pipeline::FunctionTargetsHolder;
use move_model::model::GlobalEnv;
use std::fmt::Write;

pub mod access_path;
pub mod access_path_trie;
pub mod annotations;
pub mod ast;
pub mod borrow_analysis;
pub mod clean_and_optimize;
pub mod compositional_analysis;
pub mod data_invariant_instrumentation;
pub mod dataflow_analysis;
pub mod dataflow_domains;
pub mod debug_instrumentation;
pub mod eliminate_imm_refs;
pub mod escape_analysis;
pub mod exp_generator;
pub mod exp_rewriter;
pub mod function_data_builder;
pub mod function_target;
pub mod function_target_pipeline;
pub mod global_invariant_analysis;
pub mod global_invariant_instrumentation;
pub mod graph;
pub mod inconsistency_check;
pub mod livevar_analysis;
pub mod loop_analysis;
pub mod memory_instrumentation;
pub mod mono_analysis;
pub mod move_loop_invariants;
pub mod mut_ref_instrumentation;
pub mod mutation_tester;
pub mod number_operation;
pub mod number_operation_analysis;
pub mod options;
pub mod packed_types_analysis;
pub mod pipeline_factory;
pub mod reaching_def_analysis;
pub mod spec_global_variable_analysis;
pub mod spec_instrumentation;
pub mod spec_translator;
pub mod stackless_bytecode;
pub mod stackless_bytecode_generator;
pub mod stackless_control_flow_graph;
pub mod type_invariant_analysis;
pub mod usage_analysis;
pub mod verification_analysis;
pub mod well_formed_instrumentation;

/// Print function targets for testing and debugging.
pub fn print_targets_for_test(
    env: &GlobalEnv,
    header: &str,
    targets: &FunctionTargetsHolder,
) -> String {
    let mut text = String::new();
    writeln!(&mut text, "============ {} ================", header).unwrap();
    for module_env in env.get_modules() {
        for func_env in module_env.get_functions() {
            for (variant, target) in targets.get_targets(&func_env) {
                if !target.data.code.is_empty() || target.func_env.is_native() {
                    target.register_annotation_formatters_for_test();
                    writeln!(&mut text, "\n[variant {}]\n{}", variant, target).unwrap();
                }
            }
        }
    }
    text
}
