// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Module extension detection for incremental dependency compilation.
//!
//! This module contains code supporting handling of module extensions.
//! The general ides for module extension handling is that we detect
//! which dependent modules are extended in user code, and then we
//! force re-compilation of these modules so that the extension members
//! are inlined into the extended module. We may end up recompiling more
//! modules than necessary if extended dependent module resides in a file
//! that contains another (not extended) dependent module.
use std::{collections::BTreeSet, io::Read, sync::Arc};

use move_command_line_common::files::FileHash;
use move_compiler::{
    Flags, PreCompiledProgramInfo,
    editions::Edition,
    expansion::ast as E,
    parser::{ast as P, syntax::parse_file_string},
    shared::{CompilationEnv, NamedAddressMap, PackageConfig},
};
use move_ir_types::location::sp;
use move_symbol_pool::Symbol;
use vfs::VfsPath;

use super::parsed_address;

/// Detects module extensions in user source files that extend dependency modules.
/// Returns the set of extended modules as E::ModuleIdent values.
pub fn detect_extended_dependency_modules(
    root_source_files: &[Symbol],
    overlay_fs_root: &VfsPath,
    edition: Edition,
    named_address_map: Arc<NamedAddressMap>,
    pre_compiled_deps: &PreCompiledProgramInfo,
) -> BTreeSet<E::ModuleIdent> {
    let mut extended_dep_modules = BTreeSet::new();

    // Parse each file and find extensions
    for file_path in root_source_files {
        if let Some(extended) = parse_file_for_extensions(
            file_path.as_str(),
            overlay_fs_root,
            edition,
            named_address_map.clone(),
        ) {
            for mident in extended {
                // Only include if the module exists in pre-compiled dependencies
                if pre_compiled_deps.module_info(&mident).is_some() {
                    extended_dep_modules.insert(mident);
                }
            }
        }
    }

    extended_dep_modules
}

/// Parse a single file to find extension target modules using the compiler's parser.
fn parse_file_for_extensions(
    file_path: &str,
    overlay_fs_root: &VfsPath,
    edition: Edition,
    named_address_map: Arc<NamedAddressMap>,
) -> Option<Vec<E::ModuleIdent>> {
    // Read file contents
    let vfs_path = overlay_fs_root.join(file_path).ok()?;
    let mut file = vfs_path.open_file().ok()?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).ok()?;
    let file_hash = FileHash::new(&contents);

    // Create minimal CompilationEnv for parsing
    let env = CompilationEnv::new(
        Flags::empty(),
        vec![],
        vec![],
        None,
        std::collections::BTreeMap::new(),
        Some(PackageConfig {
            edition,
            ..Default::default()
        }),
        None,
    );

    // Parse file using the compiler's parser
    let definitions = parse_file_string(&env, file_hash, &contents, None).ok()?;

    // Extract extension targets
    let mut result = Vec::new();
    for def in definitions {
        extract_extension_targets(&def, None, named_address_map.clone(), &mut result);
    }

    Some(result)
}

/// Extract target module identifiers from extension definitions.
/// Handles both top-level module extensions and those inside address blocks.
fn extract_extension_targets(
    def: &P::Definition,
    inherited_addr: Option<&P::LeadingNameAccess>,
    named_address_map: Arc<NamedAddressMap>,
    result: &mut Vec<E::ModuleIdent>,
) {
    match def {
        P::Definition::Module(mdef) if mdef.is_extension => {
            if let Some(addr) = mdef.address.as_ref().or(inherited_addr) {
                let e_address = parsed_address(*addr, named_address_map);
                result.push(sp(
                    mdef.name_loc,
                    E::ModuleIdent_::new(e_address, mdef.name),
                ));
            }
        }
        P::Definition::Address(adef) => {
            for mdef in &adef.modules {
                extract_extension_targets(
                    &P::Definition::Module(mdef.clone()),
                    Some(&adef.addr),
                    named_address_map.clone(),
                    result,
                );
            }
        }
        _ => {}
    }
}
