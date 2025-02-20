// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    compilation::{build_plan::BuildPlan, compiled_package::CompiledUnitWithSource},
    resolution::resolution_graph::ResolvedGraph,
};
use anyhow::Result;
use move_compiler::shared::{SaveFlag, SaveHook};
use move_model_2::source_model;
use std::io::Write;

// NOTE: If there are now renamings, then the root package has the global resolution of all named
// addresses in the package graph in scope. So we can simply grab all of the source files
// across all packages and build the Move model from that.
// TODO: In the future we will need a better way to do this to support renaming in packages
// where we want to support building a Move model.
pub fn build<W: Write>(
    resolved_graph: ResolvedGraph,
    writer: &mut W,
) -> Result<source_model::Model> {
    let root_package_name = resolved_graph.root_package();
    let build_plan = BuildPlan::create(resolved_graph)?;
    let program_info_hook = SaveHook::new([SaveFlag::TypingInfo]);
    let compiled_package = build_plan.compile_no_exit(writer, |compiler| {
        compiler.add_save_hook(&program_info_hook)
    })?;
    let program_info = program_info_hook.take_typing_info();
    let root_named_address_map = compiled_package
        .compiled_package_info
        .address_alias_instantiation
        .clone();
    let all_compiled_units = compiled_package
        .all_compiled_units_with_source()
        .cloned()
        .map(|CompiledUnitWithSource { unit, source_path }| (source_path, unit))
        .collect::<Vec<_>>();
    source_model::Model::new(
        compiled_package.file_map,
        Some(root_package_name),
        root_named_address_map,
        program_info,
        all_compiled_units,
    )
}
