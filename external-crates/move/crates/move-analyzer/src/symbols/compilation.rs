// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module contains code responsible for compiling Move sources
//! to a represenatation that can be used for computing symbols.

use crate::{
    compiler_info::CompilerInfo,
    diagnostics::{lsp_diagnostics, lsp_empty_diagnostics},
    symbols::{
        def_info::DefInfo,
        mod_defs::ModuleDefs,
        use_def::{UseDefMap, UseLoc},
    },
};

use anyhow::Result;
use lsp_types::{Diagnostic, Position};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    vec,
};
use tempfile::tempdir;
use vfs::{
    VfsPath,
    impls::{memory::MemoryFS, overlay::OverlayFS, physical::PhysicalFS},
};

use move_command_line_common::files::FileHash;
use move_compiler::{
    PASS_CFGIR, PASS_PARSER, PASS_TYPING, PreCompiledProgramInfo, construct_pre_compiled_lib,
    editions::{Edition, Flavor},
    expansion::ast::ModuleIdent,
    linters::LintLevel,
    parser::ast as P,
    shared::{files::MappedFiles, unique_map::UniqueMap},
    typing::ast::ModuleDefinition,
};
use move_ir_types::location::Loc;
use move_package::{
    compilation::{build_plan::BuildPlan, compiled_package::ModuleFormat},
    resolution::resolution_graph::ResolvedGraph,
    source_package::parsed_manifest::Dependencies,
};

pub const MANIFEST_FILE_NAME: &str = "Move.toml";

/// Information about parsed definitions
#[derive(Clone)]
pub struct ParsedDefinitions {
    pub source_definitions: Vec<P::PackageDefinition>,
    pub lib_definitions: Vec<P::PackageDefinition>,
}

/// Information about compiled program (ASTs at different levels)
#[derive(Clone)]
pub struct CompiledProgram {
    pub parsed_definitions: ParsedDefinitions,
    pub typed_modules: UniqueMap<ModuleIdent, ModuleDefinition>,
}

/// Information about the compiled package and data structures
/// computed during compilation and analysis
#[derive(Clone)]
pub struct CompiledPkgInfo {
    /// Package path
    pub path: PathBuf,
    /// Manifest hash
    pub manifest_hash: Option<FileHash>,
    /// A combined hash for manifest files of the dependencies
    pub deps_hash: String,
    /// Information about cached dependencies
    pub cached_deps: Option<AnalyzedPkgInfo>,
    /// Compiled user program
    pub program: CompiledProgram,
    /// Maped files
    pub mapped_files: MappedFiles,
    /// Edition of the compiler
    pub edition: Option<Edition>,
    /// Compiler info
    pub compiler_info: Option<CompilerInfo>,
    /// IDE diagnostics related to the package
    pub lsp_diags: Arc<BTreeMap<PathBuf, Vec<Diagnostic>>>,
}

/// Precomputed information about the package and its dependencies
/// cached with the purpose of being re-used during the analysis.
#[derive(Clone)]
pub struct PrecomputedPkgInfo {
    /// Hash of the manifest file for a given package
    pub manifest_hash: Option<FileHash>,
    /// Hash of dependency source files
    pub deps_hash: String,
    /// Precompiled deps
    pub deps: Arc<PreCompiledProgramInfo>,
    /// Symbols computation data
    pub deps_symbols_data: Arc<SymbolsComputationData>,
    /// Compiled user program
    pub program: Arc<CompiledProgram>,
    /// Mapping from file hashes to file paths
    pub file_paths: Arc<BTreeMap<FileHash, PathBuf>>,
    /// Hashes of dependencies
    pub dep_hashes: BTreeSet<FileHash>,
    /// Edition of the compiler used to build this package
    pub edition: Option<Edition>,
    /// Compiler info
    pub compiler_info: Option<CompilerInfo>,
    /// IDE diagnostics related to the package
    pub lsp_diags: Arc<BTreeMap<PathBuf, Vec<Diagnostic>>>,
}

/// Package data used during compilation and analysis
#[derive(Clone)]
pub struct AnalyzedPkgInfo {
    /// Cached fully compiled program representing dependencies
    pub program_deps: Arc<PreCompiledProgramInfo>,
    /// Cached symbols computation data for dependencies
    pub symbols_data: Option<Arc<SymbolsComputationData>>,
    /// Compiled user program
    pub program: Option<Arc<CompiledProgram>>,
    /// Mapping from file hashes to file paths
    pub file_paths: Arc<BTreeMap<FileHash, PathBuf>>,
    /// Hashes of dependencies
    pub dep_hashes: BTreeSet<FileHash>,
}

/// Data used during symbols computation
#[derive(Clone)]
pub struct SymbolsComputationData {
    /// Outermost definitions in a module (structs, consts, functions), keyed on a ModuleIdent
    /// string
    pub mod_outer_defs: BTreeMap<String, ModuleDefs>,
    /// A UseDefMap for a given module (needs to be appropriately set before the module
    /// processing starts) keyed on a ModuleIdent string
    pub mod_use_defs: BTreeMap<String, UseDefMap>,
    /// Uses (references) for a definition at a given location
    pub references: BTreeMap<Loc, BTreeSet<UseLoc>>,
    /// Additional information about a definitions at a given location
    pub def_info: BTreeMap<Loc, DefInfo>,
    /// Module name lengths in access paths for a given module (needs to be appropriately
    /// set before the module processing starts) keyed on a ModuleIdent string
    pub mod_to_alias_lengths: BTreeMap<String, BTreeMap<Position, usize>>,
}

impl Default for SymbolsComputationData {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolsComputationData {
    pub fn new() -> Self {
        Self {
            mod_outer_defs: BTreeMap::new(),
            mod_use_defs: BTreeMap::new(),
            references: BTreeMap::new(),
            def_info: BTreeMap::new(),
            mod_to_alias_lengths: BTreeMap::new(),
        }
    }
}

/// Builds a package at a given path and, if successful, returns parsed AST
/// and typed AST as well as (regardless of success) diagnostics.
/// See `get_symbols` for explanation of what `modified_files` parameter is.
pub fn get_compiled_pkg(
    packages_info: Arc<Mutex<BTreeMap<PathBuf, PrecomputedPkgInfo>>>,
    ide_files_root: VfsPath,
    pkg_path: &Path,
    modified_files: Option<Vec<PathBuf>>,
    lint: LintLevel,
    implicit_deps: Dependencies,
) -> Result<(Option<CompiledPkgInfo>, BTreeMap<PathBuf, Vec<Diagnostic>>)> {
    let cached_deps_exist = has_precompiled_deps(pkg_path, packages_info.clone());
    let build_config = move_package::BuildConfig {
        test_mode: true,
        install_dir: Some(tempdir().unwrap().path().to_path_buf()),
        default_flavor: Some(Flavor::Sui),
        lint_flag: lint.into(),
        force_lock_file: cached_deps_exist,
        skip_fetch_latest_git_deps: cached_deps_exist,
        implicit_dependencies: implicit_deps,
        ..Default::default()
    };

    eprintln!("symbolicating {:?}", pkg_path);

    // resolution graph diagnostics are only needed for CLI commands so ignore them by passing a
    // vector as the writer
    let resolution_graph =
        build_config.resolution_graph_for_package(pkg_path, None, &mut Vec::new())?;
    let root_pkg_name = resolution_graph.graph.root_package_name;

    let overlay_fs_root = VfsPath::new(OverlayFS::new(&[
        VfsPath::new(MemoryFS::new()),
        ide_files_root.clone(),
        VfsPath::new(PhysicalFS::new("/")),
    ]));

    let manifest_file = overlay_fs_root
        .join(pkg_path.to_string_lossy())
        .and_then(|p| p.join(MANIFEST_FILE_NAME))
        .and_then(|p| p.open_file());

    let manifest_hash = if let Ok(mut f) = manifest_file {
        let mut contents = String::new();
        let _ = f.read_to_string(&mut contents);
        Some(FileHash::new(&contents))
    } else {
        None
    };

    // Hash dependencies so we can check if something has changed.
    let (mapped_files, deps_hash, dep_hashes) =
        compute_mapped_files(&resolution_graph, overlay_fs_root.clone());
    let file_paths: Arc<BTreeMap<FileHash, PathBuf>> = Arc::new(
        mapped_files
            .file_name_mapping()
            .iter()
            .map(|(fhash, fpath)| (*fhash, fpath.clone()))
            .collect(),
    );
    let compiler_flags = resolution_graph.build_options.compiler_flags().clone();
    let build_plan =
        BuildPlan::create(&resolution_graph)?.set_compiler_vfs_root(overlay_fs_root.clone());
    let mut parsed_ast = None;
    let mut typed_ast = None;
    let mut diagnostics = None;

    let mut dependencies = build_plan.compute_dependencies();
    let (cached_info_opt, mut edition, mut compiler_info, other_diags, compiled_libs) =
        if let Ok(deps_package_paths) = dependencies.make_deps_for_compiler() {
            // Partition deps_package according whether src is available
            let src_deps = deps_package_paths
                .iter()
                .filter_map(|(p, b)| {
                    if let ModuleFormat::Source = b {
                        Some(p.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            let src_names = src_deps
                .iter()
                .filter_map(|p| p.name.as_ref().map(|(n, _)| *n))
                .collect::<BTreeSet<_>>();

            let pkg_info = packages_info.lock().unwrap();
            let (pkg_cached_deps, edition, compiler_info, compiled_libs) =
                match pkg_info.get(pkg_path) {
                    Some(d)
                        if manifest_hash.is_some()
                            && manifest_hash == d.manifest_hash
                            && deps_hash == d.deps_hash =>
                    {
                        eprintln!("found cached deps for {:?}", pkg_path);
                        (
                            Some(AnalyzedPkgInfo {
                                program_deps: d.deps.clone(),
                                symbols_data: Some(d.deps_symbols_data.clone()),
                                program: Some(d.program.clone()),
                                file_paths: d.file_paths.clone(),
                                dep_hashes: d.dep_hashes.clone(),
                            }),
                            d.edition,
                            d.compiler_info.clone(),
                            Some(d.deps.clone()),
                        )
                    }
                    _ => (
                        construct_pre_compiled_lib(
                            src_deps,
                            None,
                            compiler_flags,
                            Some(overlay_fs_root.clone()),
                        )
                        .ok()
                        .and_then(|pprog_and_comments_res| pprog_and_comments_res.ok())
                        .map(|libs| {
                            eprintln!("created pre-compiled libs for {:?}", pkg_path);
                            AnalyzedPkgInfo {
                                program_deps: Arc::new(libs),
                                symbols_data: None,
                                program: None,
                                file_paths: file_paths.clone(),
                                dep_hashes: dep_hashes.clone(),
                            }
                        }),
                        None,
                        None,
                        None,
                    ),
                };
            if compiled_libs.is_some() {
                // if successful, remove only source deps but keep bytecode deps as they
                // were not used to construct pre-compiled lib in the first place
                dependencies.remove_deps(src_names);
            }
            let other_diags = pkg_info
                .iter()
                .filter_map(|(p, d)| {
                    if p != pkg_path {
                        Some(d.lsp_diags.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            (
                pkg_cached_deps,
                edition,
                compiler_info,
                other_diags,
                compiled_libs,
            )
        } else {
            (None, None, None, vec![], None)
        };

    let (full_compilation, files_to_compile) = if let Some(chached_info) = &cached_info_opt {
        if chached_info.program.is_some() {
            // we already have cached user program, consider incremental compilation
            match modified_files {
                Some(files) => (false, BTreeSet::from_iter(files)),
                None => (true, BTreeSet::new()),
            }
        } else {
            (true, BTreeSet::new())
        }
    } else {
        (true, BTreeSet::new())
    };

    // diagnostics converted from the compiler format
    let mut lsp_diags = BTreeMap::new();
    // for diagnostics information that we actually send to the IDE, we need to
    // start with empty diagnostics for all files and replace them with actual diagnostics
    // only for files that have failures/warnings so that diagnostics for all other files
    // (that no longer have failures/warnings) are reset
    let mut ide_diags = lsp_empty_diagnostics(mapped_files.file_name_mapping());
    if full_compilation || !files_to_compile.is_empty() {
        build_plan.compile_with_driver_and_deps(
            dependencies,
            &mut std::io::sink(),
            |compiler| {
                let compiler = compiler.set_ide_mode();
                // extract expansion AST
                let (files, compilation_result) = compiler
                    .set_pre_compiled_program_opt(compiled_libs)
                    .set_files_to_compile(if full_compilation {
                        None
                    } else {
                        Some(files_to_compile.clone())
                    })
                    .run::<PASS_PARSER>()?;
                let compiler = match compilation_result {
                    Ok(v) => v,
                    Err((_pass, diags)) => {
                        let failure = true;
                        diagnostics = Some((diags, failure));
                        eprintln!("parsed AST compilation failed");
                        return Ok((files, vec![]));
                    }
                };
                eprintln!("compiled to parsed AST");
                let (compiler, parsed_program) = compiler.into_ast();
                parsed_ast = Some(parsed_program.clone());

                // extract typed AST
                let compilation_result = compiler.at_parser(parsed_program).run::<PASS_TYPING>();
                let compiler = match compilation_result {
                    Ok(v) => v,
                    Err((_pass, diags)) => {
                        let failure = true;
                        diagnostics = Some((diags, failure));
                        eprintln!("typed AST compilation failed");
                        eprintln!("diagnostics: {:#?}", diagnostics);
                        return Ok((files, vec![]));
                    }
                };
                eprintln!("compiled to typed AST");
                let (compiler, typed_program) = compiler.into_ast();
                typed_ast = Some(typed_program.clone());
                compiler_info = Some(CompilerInfo::from(
                    compiler.compilation_env().ide_information().clone(),
                ));
                edition = Some(compiler.compilation_env().edition(Some(root_pkg_name)));

                // compile to CFGIR for accurate diags
                eprintln!("compiling to CFGIR");
                let compilation_result = compiler.at_typing(typed_program).run::<PASS_CFGIR>();
                let compiler = match compilation_result {
                    Ok(v) => v,
                    Err((_pass, diags)) => {
                        let failure = false;
                        diagnostics = Some((diags, failure));
                        eprintln!("compilation to CFGIR failed");
                        return Ok((files, vec![]));
                    }
                };
                let failure = false;
                diagnostics = Some((compiler.compilation_env().take_final_diags(), failure));
                eprintln!("compiled to CFGIR");
                Ok((files, vec![]))
            },
        )?;

        if let Some((compiler_diagnostics, failure)) = diagnostics {
            lsp_diags =
                lsp_diagnostics(&compiler_diagnostics.into_codespan_format(), &mapped_files);
            if failure {
                // just return diagnostics as we don't have typed AST that we can use to compute
                // symbolication information
                debug_assert!(typed_ast.is_none());
                ide_diags.extend(lsp_diags);
                return Ok((None, ide_diags));
            }
        }
    }
    // uwrap's are safe - this function returns earlier (during diagnostics processing)
    // when failing to produce the ASTs
    let (parsed_definitions, typed_modules) = if full_compilation {
        let parsed_program = parsed_ast.unwrap();
        let parsed_definitions = ParsedDefinitions {
            source_definitions: parsed_program.source_definitions,
            lib_definitions: parsed_program.lib_definitions,
        };
        let typed_modules = typed_ast.unwrap().modules;
        (parsed_definitions, typed_modules)
    } else if files_to_compile.is_empty() {
        // no compilation happened, so we get everything from the cache, and
        // the unwraps are safe because the cache is guaranteed to exist (otherwise
        // compilation would have happened)
        let cached_info = cached_info_opt.clone().unwrap();
        let compiled_program = cached_info.program.unwrap();
        (
            compiled_program.parsed_definitions.clone(),
            compiled_program.typed_modules.clone(),
        )
    } else {
        merge_user_programs(
            cached_info_opt.clone(),
            parsed_ast.unwrap(),
            typed_ast.unwrap().modules,
            file_paths,
            files_to_compile,
        )
    };

    // There may be diagnostics from other packages that still need to be displayed
    // for files that otherwise compile without errors/warnings. An example is an
    // error in a macro that is manifested in the file where macro is defined
    // rather than in the file where macro is used. We need to layer these warnings
    // on top of (potentially) empty ones for the current package.
    for diags in other_diags {
        for (f, dvec) in diags.iter() {
            merge_diagnostics_for_file(&mut ide_diags, f, dvec);
        }
    }
    for (f, dvec) in lsp_diags.iter() {
        merge_diagnostics_for_file(&mut ide_diags, f, dvec);
    }

    let compiled_pkg_info = CompiledPkgInfo {
        path: pkg_path.into(),
        manifest_hash,
        deps_hash,
        cached_deps: cached_info_opt,
        program: CompiledProgram {
            parsed_definitions,
            typed_modules,
        },
        mapped_files,
        edition,
        compiler_info,
        lsp_diags: Arc::new(lsp_diags),
    };
    Ok((Some(compiled_pkg_info), ide_diags))
}

/// Helper function to merge diagnostics for a file into the IDE diagnostics map.
/// If diagnostics for the file don't exist yet, they are inserted.
/// If they do exist, new diagnostics are appended if they aren't already present.
fn merge_diagnostics_for_file(
    ide_diags: &mut BTreeMap<PathBuf, Vec<Diagnostic>>,
    file_path: &PathBuf,
    diagnostics: &Vec<Diagnostic>,
) {
    // sadly, `Diagnostic` does not implement `Hash`, only `Eq`, so the check is rather costly...
    let ide_diags_for_file_opt = ide_diags.get_mut(file_path);
    if ide_diags_for_file_opt.is_none() {
        ide_diags.insert(file_path.clone(), diagnostics.clone());
    } else {
        let ide_diags_for_file = ide_diags_for_file_opt.unwrap();
        for d in diagnostics {
            if !ide_diags_for_file.contains(d) {
                ide_diags_for_file.push(d.clone());
            }
        }
    }
}

fn has_precompiled_deps(
    pkg_path: &Path,
    pkg_dependencies: Arc<Mutex<BTreeMap<PathBuf, PrecomputedPkgInfo>>>,
) -> bool {
    let pkg_deps = pkg_dependencies.lock().unwrap();
    pkg_deps.contains_key(pkg_path)
}

fn compute_mapped_files(
    resolved_graph: &ResolvedGraph,
    overlay_fs: VfsPath,
) -> (MappedFiles, String, BTreeSet<FileHash>) {
    let mut mapped_files: MappedFiles = MappedFiles::empty();
    let mut hasher = Sha256::new();
    let mut dep_hashes = BTreeSet::new();
    for rpkg in resolved_graph.package_table.values() {
        for f in rpkg.get_sources(&resolved_graph.build_options).unwrap() {
            let is_dep = rpkg.package_path != resolved_graph.graph.root_path;
            // dunce does a better job of canonicalization on Windows
            let fname = dunce::canonicalize(f.as_str())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| f.to_string());
            let mut contents = String::new();
            // there is a fair number of unwraps here but if we can't read the files
            // that by all accounts should be in the file system, then there is not much
            // we can do so it's better to fail so that we can investigate
            let vfs_file_path = overlay_fs.join(fname.as_str()).unwrap();
            let mut vfs_file = vfs_file_path.open_file().unwrap();
            let _ = vfs_file.read_to_string(&mut contents);
            let fhash = FileHash::new(&contents);
            if is_dep {
                hasher.update(fhash.0);
                dep_hashes.insert(fhash);
            }
            // write to top layer of the overlay file system so that the content
            // is immutable for the duration of compilation and symbolication
            let _ = vfs_file_path.parent().create_dir_all();
            let mut vfs_file = vfs_file_path.create_file().unwrap();
            let _ = vfs_file.write_all(contents.as_bytes());
            mapped_files.add(fhash, fname.into(), Arc::from(contents.into_boxed_str()));
        }
    }
    (mapped_files, format!("{:X}", hasher.finalize()), dep_hashes)
}

/// Merges a cached compiled program with newly computed compiled program
/// In the newly computed program, only modified files are fully compiled
/// and these files are merged with the cached compiled program.
fn merge_user_programs(
    cached_info_opt: Option<AnalyzedPkgInfo>,
    parsed_program_new: P::Program,
    typed_program_modules_new: UniqueMap<ModuleIdent, ModuleDefinition>,
    file_paths_new: Arc<BTreeMap<FileHash, PathBuf>>,
    modified_files: BTreeSet<PathBuf>,
) -> (ParsedDefinitions, UniqueMap<ModuleIdent, ModuleDefinition>) {
    fn process_new_parsed_pkg(
        pkg_def: P::PackageDefinition,
        file_paths: Arc<BTreeMap<FileHash, PathBuf>>,
        modified_files: &BTreeSet<PathBuf>,
        unmodified_definitions: &mut Vec<P::PackageDefinition>,
    ) {
        // add new modules to `unmodified_definitions` (which become the result) if nothing's changed,
        // but even if nothing's changed we still need to update the named address map index
        let pkg_modified = is_parsed_pkg_modified(&pkg_def, modified_files, file_paths);

        if pkg_modified {
            unmodified_definitions.push(pkg_def);
        } else {
            // find cached package definition with the same hash
            // and update its named address map index
            let pkg_hash = match &pkg_def.def {
                P::Definition::Module(mdef) => mdef.loc.file_hash(),
                P::Definition::Address(adef) => adef.loc.file_hash(),
            };
            let cached_pkg_def =
                unmodified_definitions
                    .iter_mut()
                    .find(|pkg_def| match &pkg_def.def {
                        P::Definition::Module(mdef) => mdef.loc.file_hash() == pkg_hash,
                        P::Definition::Address(adef) => adef.loc.file_hash() == pkg_hash,
                    });
            if let Some(cached_pkg_def) = cached_pkg_def {
                cached_pkg_def.named_address_map = pkg_def.named_address_map;
            }
        }
    }

    // unraps are safe as this function only called when cached compiled program exists
    let cached_info = cached_info_opt.unwrap();
    let compiled_program_cached = cached_info.program.unwrap();
    let file_paths_cached = cached_info.file_paths;
    let mut result_parsed_definitions = compiled_program_cached.parsed_definitions.clone();
    let mut result_typed_modules = compiled_program_cached.typed_modules.clone();
    // remove modules from user code that belong to modified files
    result_parsed_definitions
        .source_definitions
        .retain(|pkg_def| {
            !is_parsed_pkg_modified(pkg_def, &modified_files, file_paths_cached.clone())
        });
    result_parsed_definitions.lib_definitions.retain(|pkg_def| {
        !is_parsed_pkg_modified(pkg_def, &modified_files, file_paths_cached.clone())
    });
    let mut typed_modules_cached_filtered = UniqueMap::new();
    for (mident, mdef) in result_typed_modules.into_iter() {
        if !is_typed_mod_modified(&mdef, &mident, &modified_files, file_paths_cached.clone()) {
            _ = typed_modules_cached_filtered.add(mident, mdef);
        }
    }
    result_typed_modules = typed_modules_cached_filtered;
    // add new modules from user code, but even if nothing's changed we still
    // need to update the named address map index)
    for pkg_def in parsed_program_new.source_definitions {
        process_new_parsed_pkg(
            pkg_def,
            file_paths_new.clone(),
            &modified_files,
            &mut result_parsed_definitions.source_definitions,
        );
    }
    for pkg_def in parsed_program_new.lib_definitions {
        process_new_parsed_pkg(
            pkg_def,
            file_paths_new.clone(),
            &modified_files,
            &mut result_parsed_definitions.lib_definitions,
        );
    }
    for (mident, mdef) in typed_program_modules_new.into_iter() {
        if is_typed_mod_modified(&mdef, &mident, &modified_files, file_paths_new.clone()) {
            result_typed_modules.remove(&mident); // in case new file has new definition of the module
            _ = result_typed_modules.add(mident, mdef);
        }
    }

    (result_parsed_definitions, result_typed_modules)
}

/// Checks if a parsed module is modified by getting
/// the module's file path and checking if it's included
/// in the set of modified file paths.
fn is_parsed_mod_modified(
    mdef: &P::ModuleDefinition,
    modified_files: &BTreeSet<PathBuf>,
    file_paths: Arc<BTreeMap<FileHash, PathBuf>>,
) -> bool {
    let Some(mod_file_path) = file_paths.get(&mdef.loc.file_hash()) else {
        eprintln!("no file path for parsed module {}", mdef.name);
        debug_assert!(false);
        return false;
    };
    modified_files.contains(mod_file_path)
}

/// Checks if a typed module is modified by getting
/// the module's file path and checking if it's included
/// in the set of modified file paths.
fn is_typed_mod_modified(
    mdef: &ModuleDefinition,
    mident: &ModuleIdent,
    modified_files: &BTreeSet<PathBuf>,
    file_paths: Arc<BTreeMap<FileHash, PathBuf>>,
) -> bool {
    let Some(mod_file_path) = file_paths.get(&mdef.loc.file_hash()) else {
        eprintln!("no file path for typed module {}", mident.value.module);
        debug_assert!(false);
        return false;
    };
    modified_files.contains(mod_file_path)
}

/// Checks if any of the package modules's were modified.
fn is_parsed_pkg_modified(
    pkg_def: &P::PackageDefinition,
    modified_files: &BTreeSet<PathBuf>,
    file_paths: Arc<BTreeMap<FileHash, PathBuf>>,
) -> bool {
    match &pkg_def.def {
        P::Definition::Module(mdef) => is_parsed_mod_modified(mdef, modified_files, file_paths),
        P::Definition::Address(adef) => adef
            .modules
            .iter()
            .any(|mdef| is_parsed_mod_modified(mdef, modified_files, file_paths.clone())),
    }
}
