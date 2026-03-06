// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module contains code supporting handling of module extensions.
//! The main problem we are trying to solve is that extensions are defined
//! in user code but the compiler "inlines" them into an AST at expansion.
//! This causes two different, though related, problems: one for extended
//! dependency modules, and one for extended user-space modules.
//!
//! Let's consider dependency modules first. When there are no extensions,
//! we cache dependencies as pre-compiled libraries (shared between different packages),
//! and also cache analysis results for these modules. Imagine a developer
//! creating and then modifying an extension for one of these modules.
//! This extension must be compiled along with the extended module for
//! the definitions introduced in the extension to be available for analysis
//! at typing. When compiling with pre-compiled libs, however, this will
//! not happen as pre-compiled libs to not actually contain ASTs for dependency
//! modules. We could of course invalidate the cache and do full compilation
//! in this case, but this would likely make the experience of developing extensions
//! unacceptable from the performance perspective (this is why we introduced
//! caching after all). Instead, we identify which dependency modules are extended,
//! exclude them from pre-compiled libs (which can still be shared between packages),
//! add them to the set of fully compiled modules, and re-analyze them on each run.
//! This retains most of the benefits of caching while providing correct support
//! for extensions. In reality, we have to be a bit more conservative here
//! as the compiler can only compile individual files and not individual modules.
//! As a result, we may exclude more modules from pre-compiled libs than necessary
//! (and recompile more modules than necessary), if multiple modules are defined
//! in the same file.
//!
//! For user-space extended modules, the problem is similar but subtly different.
//! Similarly to dependency modules, we need to identify which user-space modules are extended,
//! and include them in full compilation. However, we also need to include actual extensions
//! in full compilation. This is to ensure that both sides of the extension are fully
//! compiled and analyzed regardless of which was modified. Otherwise, we may have
//! a situation when user-level extended module is modified (and fully compiled),
//! but the extension is not. While extension's ASTs would be cached, they would
//! not be used during compilation (only during analysis), resulting in extended
//! module "inlining" extension's functions without their bodies (due to incremental
//! compilation only fully compiling modified code).

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::PathBuf,
    sync::Arc,
};

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

/// Information about modules found in a parsed file.
struct FileModuleInfo {
    /// Non-extension module definitions: (module_ident, file_path)
    module_defs: Vec<(E::ModuleIdent, Symbol)>,
    /// Extension targets (modules being extended by extensions in this file)
    extension_targets: Vec<E::ModuleIdent>,
}

/// Information about extensions in an package.
pub struct ExtensionsInfo {
    /// File paths for the user package containing module extensions
    pub extension_files: BTreeSet<PathBuf>,
    /// File paths of user modules that are extended
    pub extended_user_files: BTreeSet<PathBuf>,
    /// Module identifiers of dependency modules that are extended
    pub extended_dep_modules: BTreeSet<E::ModuleIdent>,
}

/// Collects module extensions info
///
/// This function parses all root source files to:
/// - collect file paths for user package modules that contain extensions
/// - collect file paths for user package modules that are extended
/// - collect module identifiers of dependency modules that are extended
pub fn collect_extensions_info(
    root_source_files: &[Symbol],
    overlay_fs_root: &VfsPath,
    edition: Edition,
    named_address_map: Arc<NamedAddressMap>,
    pre_compiled_deps: &PreCompiledProgramInfo,
) -> ExtensionsInfo {
    // Build user module map and collect extension targets in one pass
    let mut user_module_to_file: BTreeMap<(E::Address, P::ModuleName), PathBuf> = BTreeMap::new();
    let mut all_extension_targets: Vec<E::ModuleIdent> = Vec::new();
    let mut extension_files: BTreeSet<PathBuf> = BTreeSet::new();

    for file_path in root_source_files {
        if let Some(info) = parse_file_for_modules(
            file_path.as_str(),
            overlay_fs_root,
            edition,
            named_address_map.clone(),
        ) {
            // Record non-extension module definitions
            for (mident, fpath) in info.module_defs {
                user_module_to_file.insert(
                    (mident.value.address, mident.value.module),
                    PathBuf::from(fpath.as_str()),
                );
            }
            // Collect extension targets and track files containing extensions
            if !info.extension_targets.is_empty() {
                extension_files.insert(PathBuf::from(file_path.as_str()));
            }
            all_extension_targets.extend(info.extension_targets);
        }
    }

    // Categorize extension targets
    let mut extended_user_files = BTreeSet::new();
    let mut extended_dep_modules = BTreeSet::new();

    for target in all_extension_targets {
        if pre_compiled_deps.module_info(&target).is_some() {
            // Target is a dependency module
            extended_dep_modules.insert(target);
        } else if let Some(file_path) =
            user_module_to_file.get(&(target.value.address, target.value.module))
        {
            // Target is a user module
            extended_user_files.insert(file_path.clone());
        }
    }

    ExtensionsInfo {
        extension_files,
        extended_user_files,
        extended_dep_modules,
    }
}

/// Parse a single file to find module definitions and extension targets.
fn parse_file_for_modules(
    file_path: &str,
    overlay_fs_root: &VfsPath,
    edition: Edition,
    named_address_map: Arc<NamedAddressMap>,
) -> Option<FileModuleInfo> {
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

    // Extract module info
    let mut module_defs = Vec::new();
    let mut extension_targets = Vec::new();
    let file_symbol = Symbol::from(file_path);

    for def in definitions {
        extract_module_info(
            &def,
            None,
            named_address_map.clone(),
            file_symbol,
            &mut module_defs,
            &mut extension_targets,
        );
    }

    Some(FileModuleInfo {
        module_defs,
        extension_targets,
    })
}

/// Extract module definitions and extension targets from a definition.
/// Handles both top-level modules and those inside address blocks.
fn extract_module_info(
    def: &P::Definition,
    inherited_addr: Option<&P::LeadingNameAccess>,
    named_address_map: Arc<NamedAddressMap>,
    file_path: Symbol,
    module_defs: &mut Vec<(E::ModuleIdent, Symbol)>,
    extension_targets: &mut Vec<E::ModuleIdent>,
) {
    match def {
        P::Definition::Module(mdef) => {
            extract_module_info_internal(
                mdef,
                inherited_addr,
                named_address_map,
                file_path,
                module_defs,
                extension_targets,
            );
        }
        P::Definition::Address(adef) => {
            for mdef in &adef.modules {
                extract_module_info_internal(
                    mdef,
                    Some(&adef.addr),
                    named_address_map.clone(),
                    file_path,
                    module_defs,
                    extension_targets,
                );
            }
        }
    }
}

/// Extract module definitions and extension targets from a module definition.
fn extract_module_info_internal(
    mdef: &P::ModuleDefinition,
    inherited_addr: Option<&P::LeadingNameAccess>,
    named_address_map: Arc<NamedAddressMap>,
    file_path: Symbol,
    module_defs: &mut Vec<(E::ModuleIdent, Symbol)>,
    extension_targets: &mut Vec<E::ModuleIdent>,
) {
    if let Some(addr) = mdef.address.as_ref().or(inherited_addr) {
        // name_conflict's value does not matter
        // as it's not used for ModuleIdent comparison
        let e_address = parsed_address(*addr, named_address_map, /* name_conflict */ false);
        let mident = sp(mdef.name_loc, E::ModuleIdent_::new(e_address, mdef.name));
        if mdef.is_extension {
            // This is an extension - record what it extends
            extension_targets.push(mident);
        } else {
            // This is a regular module definition - record it
            module_defs.push((mident, file_path));
        }
    }
}
