// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    build_config::BuildConfig,
    build_plan::BuildPlan,
    compiled_package::{BuildNamedAddresses, CompiledUnitWithSource},
};
use move_compiler::shared::{SaveFlag, SaveHook};
use move_model_2::source_model;
use move_package_alt::{flavor::MoveFlavor, package::RootPackage};
use move_symbol_pool::Symbol;
use std::io::Write;

// NOTE: If there are now renamings, then the root package has the global resolution of all named
// addresses in the package graph in scope. So we can simply grab all of the source files
// across all packages and build the Move model from that.
// TODO: In the future we will need a better way to do this to support renaming in packages
// where we want to support building a Move model.
pub fn build<W: Write + Send, F: MoveFlavor>(
    writer: &mut W,
    root_pkg: &RootPackage<F>,
    build_config: &BuildConfig,
) -> anyhow::Result<source_model::Model> {
    // TODO: does this also need to be `name_root` like in compilation?
    let root_package_name = Symbol::from(root_pkg.name().as_str());
    let build_named_addresses: BuildNamedAddresses =
        root_pkg.package_info().named_addresses()?.into();
    let root_named_address_map = build_named_addresses
        .inner
        .into_iter()
        .map(|(pkg, address)| (pkg, address.into_inner()))
        .collect();

    let build_plan = BuildPlan::create(root_pkg, build_config)?;
    let program_info_hook = SaveHook::new([SaveFlag::TypingInfo]);
    let compiled_package = build_plan.compile_no_exit(writer, |compiler| {
        compiler.add_save_hook(&program_info_hook)
    })?;
    let program_info = program_info_hook.take_typing_info();
    let all_compiled_units = compiled_package
        .all_compiled_units_with_source()
        .cloned()
        .map(|CompiledUnitWithSource { unit, source_path }| (source_path, unit))
        .collect::<Vec<_>>();
    source_model::Model::from_source(
        compiled_package.file_map,
        Some(root_package_name),
        root_named_address_map,
        program_info,
        all_compiled_units,
    )
}
