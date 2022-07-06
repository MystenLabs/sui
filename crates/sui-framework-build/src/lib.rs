// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::CompiledModule;
use move_compiler::compiled_unit::{CompiledUnit, NamedCompiledModule};
use move_core_types::{account_address::AccountAddress, ident_str, language_storage::ModuleId};
use move_package::BuildConfig;
use std::{collections::HashSet, path::Path};
use sui_types::error::{SuiError, SuiResult};
use sui_verifier::verifier as sui_bytecode_verifier;

const SUI_PACKAGE_NAME: &str = "Sui";
const MOVE_STDLIB_PACKAGE_NAME: &str = "MoveStdlib";

pub fn build_move_stdlib_modules(lib_dir: &Path) -> SuiResult<Vec<CompiledModule>> {
    let denylist = vec![
        ident_str!("capability").to_owned(),
        ident_str!("event").to_owned(),
        ident_str!("guid").to_owned(),
        #[cfg(not(test))]
        ident_str!("debug").to_owned(),
    ];
    let build_config = BuildConfig::default();
    let modules: Vec<CompiledModule> = build_move_package(lib_dir, build_config)?
        .into_iter()
        .filter(|m| !denylist.contains(&m.self_id().name().to_owned()))
        .collect();
    verify_modules(&modules)?;
    Ok(modules)
}

pub fn verify_modules(modules: &[CompiledModule]) -> SuiResult {
    for m in modules {
        move_bytecode_verifier::verify_module(m).map_err(|err| {
            SuiError::ModuleVerificationFailure {
                error: err.to_string(),
            }
        })?;
        sui_bytecode_verifier::verify_module(m)?;
    }
    Ok(())
    // TODO(https://github.com/MystenLabs/sui/issues/69): Run Move linker
}
/// Given a `path` and a `build_config`, build the package in that path.
/// If we are building the Sui framework, we skip the check that the addresses should be 0
pub fn build_move_package(
    path: &Path,
    build_config: BuildConfig,
) -> SuiResult<Vec<CompiledModule>> {
    match build_config.compile_package_no_exit(path, &mut Vec::new()) {
        Err(error) => Err(SuiError::ModuleBuildFailure {
            error: error.to_string(),
        }),
        Ok(package) => {
            let compiled_modules = package.root_modules_map();
            let package_name = package.compiled_package_info.package_name.as_str();
            let is_framework =
                package_name == SUI_PACKAGE_NAME || package_name == MOVE_STDLIB_PACKAGE_NAME;
            if !is_framework {
                if let Some(m) = compiled_modules
                    .iter_modules()
                    .iter()
                    .find(|m| m.self_id().address() != &AccountAddress::ZERO)
                {
                    return Err(SuiError::ModulePublishFailure {
                        error: format!(
                            "Modules must all have 0x0 as their addresses. Violated by module {:?}",
                            m.self_id()
                        ),
                    });
                }
            }
            // Collect all module IDs from the current package to be
            // published (module names are not sufficient as we may
            // have modules with the same names in user code and in
            // Sui framework which would result in the latter being
            // pulled into a set of modules to be published).
            // For each transitive dependent module, if they are not to be published,
            // they must have a non-zero address (meaning they are already published on-chain).
            // TODO: Shall we also check if they are really on-chain in the future?
            let self_modules: HashSet<ModuleId> = compiled_modules
                .iter_modules()
                .iter()
                .map(|m| m.self_id())
                .collect();
            if let Some(m) =
                package
                    .deps_compiled_units
                    .iter()
                    .find_map(|(_, unit)| match &unit.unit {
                        CompiledUnit::Module(NamedCompiledModule { module: m, .. })
                            if !self_modules.contains(&m.self_id())
                                && m.self_id().address() == &AccountAddress::ZERO =>
                        {
                            Some(m)
                        }
                        _ => None,
                    })
            {
                return Err(SuiError::ModulePublishFailure { error: format!("Dependent modules must have been published on-chain with non-0 addresses, unlike module {:?}", m.self_id()) });
            }
            Ok(package
                .all_modules_map()
                .compute_dependency_graph()
                .compute_topological_order()
                .unwrap()
                .filter(|m| self_modules.contains(&m.self_id()))
                .cloned()
                .collect())
        }
    }
}
