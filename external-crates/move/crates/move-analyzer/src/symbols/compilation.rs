// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module contains code responsible for compiling Move sources
//! to a represenatation that can be used for computing symbols.

use crate::{
    compiler_info::{CompilerAnalysisInfo, CompilerAutocompleteInfo, process_ide_annotations},
    diagnostics::{lsp_diagnostics, lsp_empty_diagnostics},
    symbols::{
        def_info::DefInfo,
        mod_defs::{ModuleDefs, ModuleParsingInfo},
        mod_extensions::collect_extensions_info,
        use_def::{UseDefMap, UseLoc},
    },
};

use anyhow::Result;
use lsp_types::{Diagnostic, Position};
use move_symbol_pool::Symbol;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    vec,
};
use vfs::{
    VfsPath,
    impls::{memory::MemoryFS, overlay::OverlayFS, physical::PhysicalFS},
};

use move_command_line_common::files::FileHash;
use move_compiler::{
    Flags, PASS_CFGIR, PASS_PARSER, PASS_TYPING, PreCompiledProgramInfo,
    construct_pre_compiled_lib,
    diagnostics::codes::Severity,
    editions::{Edition, Flavor},
    expansion::ast::ModuleIdent,
    linters::LintLevel,
    parser::ast as P,
    shared::{
        NamedAddressMap, NamedAddressMaps, PackagePaths, files::MappedFiles, unique_map::UniqueMap,
    },
    typing::ast::ModuleDefinition,
};
use move_ir_types::location::Loc;

use move_package_alt::{MoveFlavor, RootPackage};
use move_package_alt_compilation::{
    build_config::BuildConfig,
    build_plan::BuildPlan,
    compilation::{compiler_flags, make_deps_for_compiler},
    find_env,
    source_discovery::get_sources,
};

pub const MANIFEST_FILE_NAME: &str = "Move.toml";

/// Top-level cache to contain info about compiled/analyzer packages
#[derive(Clone)]
pub struct CachedPackages {
    /// Cached info about user packages, keyed on the package path.
    /// The `None` value indicates that the package is not cached,
    /// but that caching was at least attempted.
    pub pkg_info: BTreeMap<PathBuf, Option<CachedPkgInfo>>,
    /// Pre-compiled binaries for individual dependency packages, keyed on the
    /// package path to accomodate different versions of the same package
    /// within the same workspace. The intent is to share them between
    /// different user packages
    pub compiled_dep_pkgs: BTreeMap<PathBuf, Arc<PreCompiledProgramInfo>>,
}

/// Information about parsed definitions
#[derive(Clone)]
pub struct ParsedDefinitions {
    pub named_address_maps: NamedAddressMaps,
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
    pub dep_hashes: Vec<FileHash>,
    /// Information about cached dependencies
    pub cached_deps: Option<AnalyzedPkgInfo>,
    /// Compiled user program
    pub program: CompiledProgram,
    /// Maped files
    pub mapped_files: MappedFiles,
    /// Edition of the compiler
    pub edition: Edition,
    /// Compiler analysis info
    pub compiler_analysis_info: CompilerAnalysisInfo,
    /// Compiler autocomplete info
    pub compiler_autocomplete_info: Option<CompilerAutocompleteInfo>,
    /// IDE diagnostics related to the package
    pub lsp_diags: Arc<BTreeMap<PathBuf, Vec<Diagnostic>>>,
}

/// Precomputed information about the package and its dependencies
/// cached with the purpose of being re-used during the analysis.
#[derive(Clone)]
pub struct CachedPkgInfo {
    /// Hash of the manifest file for a given package
    pub manifest_hash: Option<FileHash>,
    /// Hashes of dependency source files
    pub dep_hashes: Vec<FileHash>,
    /// Precompiled deps
    pub deps: Arc<PreCompiledProgramInfo>,
    /// Dependency names
    pub dep_names: BTreeSet<Symbol>,
    /// Symbols computation data
    pub deps_symbols_data: Arc<SymbolsComputationData>,
    /// Compiled user program
    pub program: Arc<CompiledProgram>,
    /// Mapping from file hashes to file paths
    pub file_paths: Arc<BTreeMap<FileHash, PathBuf>>,
    /// A mapping from file paths to file hashes for user code
    pub user_file_hashes: Arc<BTreeMap<PathBuf, FileHash>>,
    /// Compiler analysis info (cached)
    pub compiler_analysis_info: CompilerAnalysisInfo,
    /// IDE diagnostics related to the package
    pub lsp_diags: Arc<BTreeMap<PathBuf, Vec<Diagnostic>>>,
}

/// Package data used during compilation and analysis
#[derive(Clone)]
pub struct AnalyzedPkgInfo {
    /// Cached  pre-compiled program representing dependencies
    pub program_deps: Arc<PreCompiledProgramInfo>,
    /// Dependency names
    pub dep_names: BTreeSet<Symbol>,
    /// Cached symbols computation data for dependencies
    pub symbols_data: Option<Arc<SymbolsComputationData>>,
    /// Compiled user program
    pub program: Option<Arc<CompiledProgram>>,
    /// Mapping from file hashes to file paths
    pub file_paths: Arc<BTreeMap<FileHash, PathBuf>>,
    /// A mapping from file paths to file hashes for user code
    pub user_file_hashes: Arc<BTreeMap<PathBuf, FileHash>>,
    /// Hashes of dependencies
    pub dep_hashes: Vec<FileHash>,
}

/// Data used during symbols computation
#[derive(Clone)]
pub struct SymbolsComputationData {
    /// Outermost definitions in a module (structs, consts, functions), keyed on a ModuleIdent
    /// string
    pub mod_outer_defs: BTreeMap<String, ModuleDefs>,
    /// Per-module parsing data, keyed by file hash
    /// and then by module location within that file
    pub mod_parsing_info: BTreeMap<FileHash, BTreeMap<Loc, ModuleParsingInfo>>,
    /// A UseDefMap for a given file
    pub use_defs: BTreeMap<FileHash, UseDefMap>,
    /// Uses (references) for a definition at a given location
    pub references: BTreeMap<Loc, BTreeSet<UseLoc>>,
    /// Additional information about a definitions at a given location
    pub def_info: BTreeMap<Loc, DefInfo>,
    /// Module name lengths in access paths for a given module (needs to be appropriately
    /// set before the module processing starts) keyed on a ModuleIdent string
    pub mod_to_alias_lengths: BTreeMap<String, BTreeMap<Position, usize>>,
}

/// Mapped files and associated (meta) data
#[derive(Clone)]
struct MappedFilesData {
    /// Mapped files
    files: MappedFiles,
    /// Hash of all dependency files
    deps_hash: String,
    /// Hashes of individual dependency files
    dep_hashes: Vec<FileHash>,
    /// Paths of individual dependency packages
    dep_pkg_paths: BTreeMap<Symbol, PathBuf>,
    /// Root package source files (for extension detection)
    root_source_files: Vec<Symbol>,
    /// Root package named addresses (for extension detection)
    root_named_addresses: Arc<NamedAddressMap>,
    /// Root package edition (for extension detection)
    root_edition: Edition,
}

/// Result of caching dependencies (used internally).
/// This struct passes data from the caching block to the compiler driver closure.
#[derive(Clone)]
struct CachingResult {
    /// Cached package info needed for analysis
    pkg_deps: Option<AnalyzedPkgInfo>,
    /// Compiler analysis info
    compiler_analysis_info: CompilerAnalysisInfo,
    /// Source dependencies (package name -> PackagePaths)
    src_deps: BTreeMap<Symbol, PackagePaths>,
    /// Dependency files that should be compiled fully instead of using pre-compiled libs
    dep_files_to_compile_fully: BTreeSet<Symbol>,
    /// Packages containing files to compile fully (kept in dependencies)
    packages_to_keep: BTreeSet<Symbol>,
    /// User files containing extended modules that need full compilation
    user_files_to_compile_fully: BTreeSet<PathBuf>,
}

impl CachedPackages {
    pub fn new() -> Self {
        Self {
            pkg_info: BTreeMap::new(),
            compiled_dep_pkgs: BTreeMap::new(),
        }
    }
}

impl AnalyzedPkgInfo {
    pub fn new(
        program_deps: Arc<PreCompiledProgramInfo>,
        dep_names: BTreeSet<Symbol>,
        symbols_data: Option<Arc<SymbolsComputationData>>,
        program: Option<Arc<CompiledProgram>>,
        file_paths: Arc<BTreeMap<FileHash, PathBuf>>,
        user_file_hashes: Arc<BTreeMap<PathBuf, FileHash>>,
        dep_hashes: Vec<FileHash>,
    ) -> Self {
        Self {
            program_deps,
            dep_names,
            symbols_data,
            program,
            file_paths,
            user_file_hashes,
            dep_hashes,
        }
    }

    /// Constructs `AnalyzedPkgInfo` with only information about
    /// precompiled dependencies.
    pub fn new_precompiled_only(
        program_deps: Arc<PreCompiledProgramInfo>,
        dep_names: BTreeSet<Symbol>,
        dep_hashes: Vec<FileHash>,
    ) -> Self {
        Self {
            program_deps,
            dep_names,
            symbols_data: None,
            program: None,
            file_paths: Arc::new(BTreeMap::new()),
            user_file_hashes: Arc::new(BTreeMap::new()),
            dep_hashes,
        }
    }
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
            mod_parsing_info: BTreeMap::new(),
            use_defs: BTreeMap::new(),
            references: BTreeMap::new(),
            def_info: BTreeMap::new(),
            mod_to_alias_lengths: BTreeMap::new(),
        }
    }
}

impl MappedFilesData {
    pub fn new(
        files: MappedFiles,
        deps_hash: String,
        dep_hashes: Vec<FileHash>,
        dep_pkg_paths: BTreeMap<Symbol, PathBuf>,
        root_source_files: Vec<Symbol>,
        root_named_addresses: Arc<NamedAddressMap>,
        root_edition: Edition,
    ) -> Self {
        Self {
            files,
            deps_hash,
            dep_hashes,
            dep_pkg_paths,
            root_source_files,
            root_named_addresses,
            root_edition,
        }
    }
}

impl CachingResult {
    pub fn new(
        pkg_deps: Option<AnalyzedPkgInfo>,
        compiler_analysis_info: CompilerAnalysisInfo,
        src_deps: BTreeMap<Symbol, PackagePaths>,
        dep_files_to_compile_fully: BTreeSet<Symbol>,
        packages_to_keep: BTreeSet<Symbol>,
        user_files_to_compile_fully: BTreeSet<PathBuf>,
    ) -> Self {
        Self {
            pkg_deps,
            compiler_analysis_info,
            src_deps,
            dep_files_to_compile_fully,
            packages_to_keep,
            user_files_to_compile_fully,
        }
    }

    pub fn empty() -> Self {
        Self {
            pkg_deps: None,
            compiler_analysis_info: CompilerAnalysisInfo::new(),
            src_deps: BTreeMap::new(),
            dep_files_to_compile_fully: BTreeSet::new(),
            packages_to_keep: BTreeSet::new(),
            user_files_to_compile_fully: BTreeSet::new(),
        }
    }

    /// Returns pre-compiled program info with modules filtered out for files
    /// that need full compilation. Returns the original pre-compiled info
    /// unchanged when no filtering is needed.
    fn get_filtered_precompiled(&self) -> Option<Arc<PreCompiledProgramInfo>> {
        self.pkg_deps.as_ref().map(|d| {
            if self.dep_files_to_compile_fully.is_empty() {
                d.program_deps.clone()
            } else {
                Arc::new(
                    d.program_deps
                        .filter_modules_on_paths(&self.dep_files_to_compile_fully),
                )
            }
        })
    }

    /// Returns file paths to exclude from compiler targets. These are files
    /// from kept packages that don't need full compilation. Returns an empty
    /// set when no filtering is needed.
    fn get_files_to_exclude_from_targets(&self) -> BTreeSet<Symbol> {
        self.packages_to_keep
            .iter()
            .flat_map(|package| {
                let all_package_files: BTreeSet<Symbol> = self
                    .src_deps
                    .get(package)
                    .map(|pp| pp.paths.iter().map(|p| Symbol::from(p.as_str())).collect())
                    .unwrap_or_default();
                all_package_files
                    .difference(&self.dep_files_to_compile_fully)
                    .copied()
                    .collect::<BTreeSet<_>>()
            })
            .collect()
    }

    /// Returns all files (dependency + user) that need full compilation as PathBuf.
    fn get_all_files_to_compile_fully(&self) -> BTreeSet<PathBuf> {
        let mut all_files: BTreeSet<PathBuf> = self
            .dep_files_to_compile_fully
            .iter()
            .map(|s| PathBuf::from(s.as_str()))
            .collect();
        all_files.extend(self.user_files_to_compile_fully.clone());
        all_files
    }
}

/// Builds a package at a given path and, if successful, returns parsed AST
/// and typed AST as well as (regardless of success) diagnostics.
/// See `get_symbols` for explanation of what `modified_files` parameter is.
pub fn get_compiled_pkg<F: MoveFlavor>(
    packages_info: Arc<Mutex<CachedPackages>>,
    ide_files_root: VfsPath,
    pkg_path: &Path,
    lint: LintLevel,
    flavor: Option<Flavor>,
    cursor_file_opt: Option<&PathBuf>,
) -> Result<(Option<CompiledPkgInfo>, BTreeMap<PathBuf, Vec<Diagnostic>>)> {
    let build_config = move_package_alt_compilation::build_config::BuildConfig {
        test_mode: true,
        default_flavor: flavor,
        lint_flag: lint.into(),
        allow_dirty: true,
        ..Default::default()
    };

    eprintln!("symbolicating {:?}", pkg_path);

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

    let root_pkg = load_root_pkg::<F>(&build_config, pkg_path)?;
    let root_pkg_name = Symbol::from(root_pkg.name().to_string());
    // the package's transitive dependencies
    let mut dependencies: Vec<_> = root_pkg
        .packages()
        .into_iter()
        .filter(|x| !x.is_root())
        .collect();
    let build_plan =
        BuildPlan::create(&root_pkg, &build_config)?.set_compiler_vfs_root(overlay_fs_root.clone());

    // Hash dependencies so we can check if something has changed.
    // TODO: do we still need this?
    let mapped_files_data =
        compute_mapped_files(&root_pkg, &build_config, overlay_fs_root.clone())?;
    let file_paths: Arc<BTreeMap<FileHash, PathBuf>> = Arc::new(
        mapped_files_data
            .files
            .file_name_mapping()
            .iter()
            .map(|(fhash, fpath)| (*fhash, fpath.clone()))
            .collect(),
    );

    let mut parsed_ast = None;
    let mut typed_ast = None;
    let mut diagnostics = None;
    let mut compiler_analysis_info_opt = None;
    let mut compiler_autocomplete_info_opt = None;

    let compiler_flags = compiler_flags(&build_config);
    let (caching_result, other_diags) = if let Ok(deps_package_paths) =
        make_deps_for_compiler(&mut Vec::new(), dependencies.clone(), &build_config)
    {
        let src_deps: BTreeMap<Symbol, PackagePaths> = deps_package_paths
            .into_iter()
            .filter_map(|p| p.name.as_ref().map(|(n, _)| (*n, p.clone())))
            .collect();

        // Map from file paths to package names (used for incremental dep compilation)
        let file_to_package: BTreeMap<Symbol, Symbol> = src_deps
            .iter()
            .flat_map(|(pkg_name, pkg_paths)| {
                pkg_paths
                    .paths
                    .iter()
                    .map(move |path| (Symbol::from(path.as_str()), *pkg_name))
            })
            .collect();

        let mut cached_packages = packages_info.lock().unwrap();
        // need to extract all data from pkg_info first so that we can
        // borrow it mutably later
        let cached_pkg_info_opt = match cached_packages.pkg_info.get(pkg_path) {
            Some(Some(d)) => {
                let mut hasher = Sha256::new();
                d.dep_hashes.iter().for_each(|h| {
                    hasher.update(h.to_bytes());
                });
                let deps_hash = hasher_to_hash_string(hasher);
                if manifest_hash.is_some()
                    && manifest_hash == d.manifest_hash
                    && mapped_files_data.deps_hash == deps_hash
                {
                    eprintln!("found cached deps for {:?}", pkg_path);
                    Some(d)
                } else {
                    eprintln!("found invalidated cached deps for {:?}", pkg_path);
                    None
                }
            }
            _ => {
                eprintln!("no cached deps for {:?}", pkg_path);
                None
            }
        };

        let other_diags = cached_packages
            .pkg_info
            .iter()
            .filter_map(|(p, cached_pkg_info_opt)| {
                if p != pkg_path {
                    cached_pkg_info_opt.as_ref().map(|c| c.lsp_diags.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let caching_result = match cached_pkg_info_opt {
            Some(cached_pkg_info) => {
                // Detect all extended modules (both dependency and user-space)
                let extended = collect_extensions_info(
                    &mapped_files_data.root_source_files,
                    &overlay_fs_root,
                    mapped_files_data.root_edition,
                    mapped_files_data.root_named_addresses.clone(),
                    &cached_pkg_info.deps,
                );

                // Get file paths for extended dependency modules
                let dep_files_to_compile_fully = cached_pkg_info
                    .deps
                    .get_file_paths_for_modules(&extended.extended_dep_modules);

                // Compute packages containing files to compile fully
                let packages_to_keep: BTreeSet<Symbol> = dep_files_to_compile_fully
                    .iter()
                    .filter_map(|f| file_to_package.get(f).copied())
                    .collect();

                // Remove dependencies that are already included in the cached package info,
                // EXCEPT those in packages_to_keep (which need full compilation).
                dependencies.retain(|d| {
                    let pkg_symbol = Symbol::from(d.id().to_string());
                    !cached_pkg_info.dep_names.contains(&pkg_symbol)
                        || packages_to_keep.contains(&pkg_symbol)
                });

                let deps = cached_pkg_info.deps.clone();
                let analyzed_pkg_info = AnalyzedPkgInfo::new(
                    deps,
                    cached_pkg_info.dep_names.clone(),
                    Some(cached_pkg_info.deps_symbols_data.clone()),
                    Some(cached_pkg_info.program.clone()),
                    cached_pkg_info.file_paths.clone(),
                    cached_pkg_info.user_file_hashes.clone(),
                    cached_pkg_info.dep_hashes.clone(),
                );

                // Combine extended module files and extension files for full compilation
                let mut user_files_to_compile = extended.extended_user_files;
                user_files_to_compile.extend(extended.extension_files);

                CachingResult::new(
                    Some(analyzed_pkg_info),
                    cached_pkg_info.compiler_analysis_info.clone(),
                    src_deps.clone(),
                    dep_files_to_compile_fully,
                    packages_to_keep,
                    user_files_to_compile,
                )
            }
            None => {
                // get the topologically sorted dependencies, but use the package ids instead of
                // package names. In the new pkg system, multiple packages with the same name can
                // exist as the package system will assign unique package ids to them, before
                // passing them to the compiler.
                let sorted_deps: Vec<Symbol> = root_pkg
                    .sorted_deps_ids()
                    .into_iter()
                    .map(|x| Symbol::from(x.to_string()))
                    .collect();
                if let Some((program_deps, dep_names)) = compute_pre_compiled_dep_data(
                    &mut cached_packages.compiled_dep_pkgs,
                    mapped_files_data.dep_pkg_paths.clone(),
                    src_deps.clone(),
                    root_pkg_name,
                    &sorted_deps,
                    compiler_flags,
                    overlay_fs_root.clone(),
                ) {
                    // Detect all extended modules (both dependency and user-space)
                    let extended = collect_extensions_info(
                        &mapped_files_data.root_source_files,
                        &overlay_fs_root,
                        mapped_files_data.root_edition,
                        mapped_files_data.root_named_addresses.clone(),
                        &program_deps,
                    );

                    let dep_files_to_compile_fully =
                        program_deps.get_file_paths_for_modules(&extended.extended_dep_modules);
                    let packages_to_keep: BTreeSet<Symbol> = dep_files_to_compile_fully
                        .iter()
                        .filter_map(|f| file_to_package.get(f).copied())
                        .collect();

                    let analyzed_pkg_info = AnalyzedPkgInfo::new_precompiled_only(
                        program_deps,
                        dep_names,
                        mapped_files_data.dep_hashes.clone(),
                    );
                    // On first compilation, user_files is empty since full compilation happens anyway
                    CachingResult::new(
                        Some(analyzed_pkg_info),
                        CompilerAnalysisInfo::new(),
                        src_deps,
                        dep_files_to_compile_fully,
                        packages_to_keep,
                        BTreeSet::new(),
                    )
                } else {
                    CachingResult::empty()
                }
            }
        };

        (caching_result, other_diags)
    } else {
        (CachingResult::empty(), vec![])
    };

    let (full_compilation, files_to_compile) = if let Some(cached_info) = &caching_result.pkg_deps {
        if cached_info.program.is_some() {
            // we already have cached user program, consider incremental compilation
            let cached_user_file_hashes = cached_info.user_file_hashes.clone();

            // Compute modified files: either new files or files with different hashes
            let mut modified_files = BTreeSet::new();

            // Check all files directly without materializing intermediate collection
            for (fhash, fpath) in mapped_files_data.files.file_name_mapping().iter() {
                match cached_user_file_hashes.get(fpath) {
                    // File exists in cache but has different hash (modified)
                    Some(cached_hash) if cached_hash != fhash => {
                        modified_files.insert(fpath.clone());
                    }
                    // File doesn't exist in cache (new file)
                    None => {
                        modified_files.insert(fpath.clone());
                    }
                    // File exists and has same hash (unchanged) - do nothing
                    Some(_) => {}
                }
            }

            // Add cursor file to force incremental compilation for autocomplete
            if let Some(cursor_file) = cursor_file_opt {
                modified_files.insert(cursor_file.clone());
            }

            // Add user files that contain extended modules to ensure their function bodies
            // are preserved during compilation
            modified_files.extend(caching_result.user_files_to_compile_fully.clone());

            (false, modified_files)
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
    let mut ide_diags = lsp_empty_diagnostics(mapped_files_data.files.file_name_mapping());
    if full_compilation || !files_to_compile.is_empty() {
        build_plan.compile_with_driver_and_deps(
            dependencies.into_iter().map(|x| x.id()).cloned().collect(),
            &mut std::io::sink(),
            |compiler| {
                // Set up compiler with optional filtering for incremental dependency compilation
                let (files, compilation_result) = compiler
                    .set_ide_mode()
                    .filter_dep_package_targets(&caching_result.get_files_to_exclude_from_targets())
                    .set_pre_compiled_program_opt(caching_result.get_filtered_precompiled())
                    .set_files_to_compile(if full_compilation {
                        None
                    } else {
                        // Include both modified user files and files containing extended modules
                        // (both dependency and user-space) to ensure function bodies are preserved
                        let mut all_files = files_to_compile.clone();
                        all_files.extend(caching_result.get_all_files_to_compile_fully());
                        Some(all_files)
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
                        return Ok((files, vec![]));
                    }
                };
                eprintln!("compiled to typed AST");
                let (compiler, typed_program) = compiler.into_ast();
                typed_ast = Some(typed_program.clone());
                let (analysis_info, autocomplete_info) =
                    process_ide_annotations(compiler.compilation_env().ide_information().clone());
                // Don't update caching_result here - will be merged in conditional below
                compiler_analysis_info_opt = Some(analysis_info);

                // Filter autocomplete info based on cursor file
                // - If cursor_file_opt is None: no autocomplete needed, use empty info
                // - If cursor_file_opt is Some: only keep autocomplete info for that file
                compiler_autocomplete_info_opt = Some(if let Some(cursor_file) = cursor_file_opt {
                    filter_autocomplete_for_file(
                        autocomplete_info,
                        cursor_file,
                        mapped_files_data.files.file_name_mapping(),
                    )
                } else {
                    CompilerAutocompleteInfo::new()
                });
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
            lsp_diags = lsp_diagnostics(
                &compiler_diagnostics.into_codespan_format(),
                &mapped_files_data.files,
            );
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
    let (parsed_definitions, typed_modules, compiler_analysis_info) = if full_compilation {
        let parsed_program = parsed_ast.unwrap();
        let parsed_definitions = ParsedDefinitions {
            named_address_maps: parsed_program.named_address_maps,
            source_definitions: parsed_program.source_definitions,
            lib_definitions: parsed_program.lib_definitions,
        };
        let typed_modules = typed_ast.unwrap().modules;
        (
            parsed_definitions,
            typed_modules,
            // unwrap is safe as this is created at the same time
            // as parsed_ast and typed_ast
            compiler_analysis_info_opt.unwrap(),
        )
    } else if files_to_compile.is_empty() {
        // no compilation happened, so we get everything from the cache, and
        // the unwraps are safe because the cache is guaranteed to exist (otherwise
        // compilation would have happened)
        if let Some(Some(cached)) = packages_info.lock().unwrap().pkg_info.get(pkg_path) {
            // Restore diagnostics from cache so that they can be propeerly
            // displayed even if no compilation happened (e.g., upon first
            // opening a package).
            lsp_diags = (*cached.lsp_diags).clone();
        }
        let cached_info = caching_result.pkg_deps.clone().unwrap();
        let compiled_program = cached_info.program.unwrap();
        (
            compiled_program.parsed_definitions.clone(),
            compiled_program.typed_modules.clone(),
            caching_result.compiler_analysis_info.clone(),
        )
    } else {
        let (parsed_defs, typed_mods) = merge_user_programs(
            caching_result.pkg_deps.clone(),
            parsed_ast.unwrap(),
            typed_ast.unwrap().modules,
            file_paths.clone(),
            files_to_compile.clone(),
        );

        let merged_analysis_info = merge_compiler_analysis_info(
            caching_result.compiler_analysis_info.clone(),
            compiler_analysis_info_opt.unwrap(),
            &file_paths,
            &files_to_compile,
        );

        (parsed_defs, typed_mods, merged_analysis_info)
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

    let root_edition = mapped_files_data.root_edition;
    let compiled_pkg_info = CompiledPkgInfo {
        path: pkg_path.into(),
        manifest_hash,
        dep_hashes: mapped_files_data.dep_hashes,
        cached_deps: caching_result.pkg_deps,
        program: CompiledProgram {
            parsed_definitions,
            typed_modules,
        },
        mapped_files: mapped_files_data.files,
        edition: root_edition,
        compiler_analysis_info,
        compiler_autocomplete_info: compiler_autocomplete_info_opt,
        lsp_diags: Arc::new(lsp_diags),
    };
    Ok((Some(compiled_pkg_info), ide_diags))
}

/// Get pre-compiled dependencies from cache or compile and cache them
/// if they are not in the cache.
/// It may or may not succeed in pre-compiling all dependencies. If it
/// none of them can be pre-compiled, it returns None.
/// If some of them can be pre-compiled, it returns a tuple of the
/// pre-compiled dependencies and the names of the dependencies whose
/// pre-compilation was succcesful.
fn compute_pre_compiled_dep_data(
    compiled_dep_pkgs: &mut BTreeMap<PathBuf, Arc<PreCompiledProgramInfo>>,
    mut dep_paths: BTreeMap<Symbol, PathBuf>,
    mut src_deps: BTreeMap<Symbol, PackagePaths>,
    root_package_name: Symbol,
    topological_order: &[Symbol],
    compiler_flags: Flags,
    vfs_root: VfsPath,
) -> Option<(Arc<PreCompiledProgramInfo>, BTreeSet<Symbol>)> {
    let mut pre_compiled_modules = BTreeMap::new();
    let mut pre_compiled_names = BTreeSet::new();
    for pkg_name in topological_order.iter().rev() {
        // both pkg_name and root_package_name are actually PackageIDs and generated by the pkg
        // system
        if pkg_name == &root_package_name {
            continue;
        }
        let Some(dep_path) = dep_paths.remove(pkg_name) else {
            eprintln!("no dep path for {pkg_name}, no caching");
            // do non-cached path
            return None;
        };
        let Some(dep_info) = src_deps.remove(pkg_name) else {
            eprintln!("no dep info for {pkg_name}, no caching");
            // do non-cached path
            return None;
        };
        let Some((name, _)) = dep_info.name else {
            eprintln!("no pkg name, no caching");
            // do non-cached path
            return None;
        };
        if let Some(dep_pkg) = compiled_dep_pkgs.get(&dep_path) {
            eprintln!("found cached dep for {:?}", dep_path);
            pre_compiled_modules.extend(dep_pkg.iter().map(|(k, v)| (*k, v.clone())));
            pre_compiled_names.insert(name);
            continue;
        }
        eprintln!("pre-compiling dep {name}");
        let new_pre_compiled_modules_opt = construct_pre_compiled_lib(
            vec![dep_info],
            None,
            Some(Arc::new(PreCompiledProgramInfo::new(
                pre_compiled_modules.clone(),
            ))),
            true,
            compiler_flags.clone(),
            Some(vfs_root.clone()),
        )
        .inspect_err(|e| {
            eprintln!("failed to pre-compile dep {name} for {root_package_name}: {e}");
        })
        .ok()
        .and_then(|pprog_and_comments| {
            pprog_and_comments.inspect_err(|(_, diags)| {
                let diags_vec = diags.clone().into_vec();
                let first_diag = diags_vec.iter().find(|d| d.info().severity() == Severity::BlockingError || d.info().severity() == Severity::Bug).unwrap_or_else(|| diags_vec.first().unwrap());
                eprintln!(
                    "failed to construct pre-compiled dep {name} for {root_package_name}: {first_diag:?}"
                );
            })
            .ok()
        });
        if let Some(new_pre_compiled_modules) = new_pre_compiled_modules_opt {
            pre_compiled_modules.extend(
                new_pre_compiled_modules
                    .iter()
                    .map(|(k, v)| (*k, v.clone())),
            );
            pre_compiled_names.insert(name);
            eprintln!("inserting new dep into cache for {:?}", dep_path);
            compiled_dep_pkgs.insert(dep_path, Arc::new(new_pre_compiled_modules.clone()));
        } else {
            // bail with whatever deps we managed to pre-compile
            break;
        }
    }
    Some((
        Arc::new(PreCompiledProgramInfo::new(pre_compiled_modules)),
        pre_compiled_names,
    ))
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
    if let Some(ide_diags_for_file) = ide_diags_for_file_opt {
        for d in diagnostics {
            if !ide_diags_for_file.contains(d) {
                ide_diags_for_file.push(d.clone());
            }
        }
    } else {
        ide_diags.insert(file_path.clone(), diagnostics.clone());
    }
}

fn compute_mapped_files<F: MoveFlavor>(
    root_pkg: &RootPackage<F>,
    build_config: &BuildConfig,
    overlay_fs: VfsPath,
) -> anyhow::Result<MappedFilesData> {
    let mut mapped_files: MappedFiles = MappedFiles::empty();
    let mut hasher = Sha256::new();
    let mut dep_hashes = vec![];
    let mut dep_pkg_paths = BTreeMap::new();
    let mut root_source_files = Vec::new();

    // Compute root package info once (for extension detection)
    let root_named_addresses = Arc::new(
        root_pkg
            .package_info()
            .named_addresses()
            .map(|addrs| build_config.addresses_for_config(addrs).inner)
            .unwrap_or_default(),
    );
    let root_edition = root_pkg
        .package_info()
        .edition()
        .or(build_config.default_edition)
        .unwrap_or(Edition::LEGACY);

    for rpkg in root_pkg.packages() {
        for f in get_sources(rpkg.path(), build_config).unwrap() {
            let is_dep = !rpkg.is_root();
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
                hasher.update(fhash.to_bytes());
                dep_hashes.push(fhash);
                dep_pkg_paths.insert(rpkg.id().clone().into(), rpkg.path().path().to_path_buf());
            } else {
                // Collect root source files for extension detection
                root_source_files.push(Symbol::from(fname.as_str()));
            }
            // write to top layer of the overlay file system so that the content
            // is immutable for the duration of compilation and symbolication
            let _ = vfs_file_path.parent().create_dir_all();
            let mut vfs_file = vfs_file_path.create_file().unwrap();
            let _ = vfs_file.write_all(contents.as_bytes());
            mapped_files.add(fhash, fname.into(), Arc::from(contents.into_boxed_str()));
        }
    }

    Ok(MappedFilesData::new(
        mapped_files,
        hasher_to_hash_string(hasher),
        dep_hashes,
        dep_pkg_paths,
        root_source_files,
        root_named_addresses,
        root_edition,
    ))
}

/// Helper function to convert a hasher to a hash string
/// consistently across different functions.
fn hasher_to_hash_string(hasher: Sha256) -> String {
    format!("{:X}", hasher.finalize())
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
            // Update ALL cached package definitions from the same file. All modules in
            // a file share the same NamedAddressMapIndex, so we update all of them.
            let pkg_file_hash = match &pkg_def.def {
                P::Definition::Module(mdef) => mdef.loc.file_hash(),
                P::Definition::Address(adef) => adef.loc.file_hash(),
            };
            for cached_pkg_def in
                unmodified_definitions
                    .iter_mut()
                    .filter(|cached_def| match &cached_def.def {
                        P::Definition::Module(mdef) => mdef.loc.file_hash() == pkg_file_hash,
                        P::Definition::Address(adef) => adef.loc.file_hash() == pkg_file_hash,
                    })
            {
                cached_pkg_def.named_address_map = pkg_def.named_address_map;
            }
        }
    }

    // unwraps are safe as this function only called when cached compiled program exists
    let cached_info = cached_info_opt.unwrap();
    let compiled_program_cached = cached_info.program.unwrap();
    let file_paths_cached = cached_info.file_paths;

    // Use new named_address_maps directly. Cached packages get their indices updated
    // via process_new_parsed_pkg to point to maps in the new NamedAddressMaps.
    let mut result_parsed_definitions = ParsedDefinitions {
        named_address_maps: parsed_program_new.named_address_maps.clone(),
        source_definitions: compiled_program_cached
            .parsed_definitions
            .source_definitions
            .clone(),
        lib_definitions: compiled_program_cached
            .parsed_definitions
            .lib_definitions
            .clone(),
    };
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

/// Merges cached CompilerAnalysisInfo with newly compiled info during incremental compilation.
/// Filters out entries from modified files from the cache, then adds new entries.
fn merge_compiler_analysis_info(
    cached_info: CompilerAnalysisInfo,
    new_info: CompilerAnalysisInfo,
    file_paths: &BTreeMap<FileHash, PathBuf>,
    modified_files: &BTreeSet<PathBuf>,
) -> CompilerAnalysisInfo {
    let mut result = cached_info;

    // Helper to check if a location is in a modified file
    let is_modified = |loc: &Loc| -> bool {
        file_paths
            .get(&loc.file_hash())
            .map(|path| modified_files.contains(path))
            .unwrap_or(false)
    };

    // Remove entries from modified files
    result.macro_info.retain(|loc, _| !is_modified(loc));
    result.expanded_lambdas.retain(|loc| !is_modified(loc));
    result.ellipsis_binders.retain(|loc| !is_modified(loc));
    result.string_values.retain(|loc, _| !is_modified(loc));

    // Add new entries - no additional filtering needed
    // as incremental compilation produced these
    // only for modified files
    result.macro_info.extend(new_info.macro_info);
    result.expanded_lambdas.extend(new_info.expanded_lambdas);
    result.ellipsis_binders.extend(new_info.ellipsis_binders);
    result.string_values.extend(new_info.string_values);

    result
}

/// Filters CompilerAutocompleteInfo to only include entries for the specified file.
/// Used when cursor is in a specific file - we only need autocomplete info for that file.
fn filter_autocomplete_for_file(
    autocomplete_info: CompilerAutocompleteInfo,
    cursor_file: &PathBuf,
    file_paths: &BTreeMap<FileHash, PathBuf>,
) -> CompilerAutocompleteInfo {
    // Find the FileHash for the cursor file
    let cursor_fhash = file_paths
        .iter()
        .find(|(_, path)| *path == cursor_file)
        .map(|(fhash, _)| *fhash);

    let Some(cursor_fhash) = cursor_fhash else {
        // Cursor file not in mapped files - return empty
        return CompilerAutocompleteInfo::new();
    };

    // Filter dot_autocomplete_info: keep only the cursor file's entry
    let filtered_dot = autocomplete_info
        .dot_autocomplete_info
        .into_iter()
        .filter(|(fhash, _)| *fhash == cursor_fhash)
        .collect();

    // Filter path_autocomplete_info: keep only entries whose Loc is in cursor file
    let filtered_path = autocomplete_info
        .path_autocomplete_info
        .into_iter()
        .filter(|(loc, _)| loc.file_hash() == cursor_fhash)
        .collect();

    CompilerAutocompleteInfo {
        dot_autocomplete_info: filtered_dot,
        path_autocomplete_info: filtered_path,
    }
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
    if modified_files.contains(mod_file_path) {
        return true;
    }

    // When both the extended module and extension are in user space, extension members get
    // inlined into the extended module during expansion. If only the extension file is modified,
    // the extended module's definition location (checked above) appears unchanged. Without
    // checking member locations, we'd use stale cached data with incorrect file hashes.
    let is_member_modified = |loc: &Loc| -> bool {
        let Some(member_file_path) = file_paths.get(&loc.file_hash()) else {
            eprintln!(
                "no file path for member in typed module {}",
                mident.value.module
            );
            debug_assert!(false);
            return false;
        };
        modified_files.contains(member_file_path)
    };

    for (name_loc, _, _) in &mdef.functions {
        if is_member_modified(&name_loc) {
            return true;
        }
    }
    for (name_loc, _, _) in &mdef.structs {
        if is_member_modified(&name_loc) {
            return true;
        }
    }
    for (name_loc, _, _) in &mdef.enums {
        if is_member_modified(&name_loc) {
            return true;
        }
    }
    for (name_loc, _, _) in &mdef.constants {
        if is_member_modified(&name_loc) {
            return true;
        }
    }

    false
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

fn load_root_pkg<F: MoveFlavor>(
    build_config: &BuildConfig,
    path: &Path,
) -> anyhow::Result<RootPackage<F>> {
    let env = find_env::<F>(path, build_config)?;
    let mut root_pkg = build_config.package_loader(path, &env).load_sync()?;

    root_pkg.save_lockfile_to_disk()?;

    Ok(root_pkg)
}
