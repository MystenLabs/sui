// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This set of modules are responsible for building symbolication information on top of compiler's parsed
//! and typed ASTs, in particular identifier definitions to be used for implementing go-to-def,
//! go-to-references, and on-hover language server commands.
//!
//! The analysis starts with top-level module definitions being processed and then proceeds to
//! process parsed AST (parsing analysis) and typed AST (typing analysis) to gather all the required
//! information which is then summarized in the Symbols struct subsequently used by the language
//! server to find definitions, references, auto-completions, etc.  Parsing analysis is largely
//! responsible for processing import statements (no longer available at the level of typed AST) and
//! typing analysis gathers remaining information. In particular, for local definitions, typing
//! analysis builds a scope stack, entering encountered definitions and matching uses to a
//! definition in the innermost scope.
//!
//! Here is a brief description of how the symbolication information is encoded. Each identifier in
//! the source code of a given module is represented by its location (UseLoc struct): line number,
//! starting and ending column, and hash of the source file where this identifier is located). A
//! definition for each identifier (if any - e.g., built-in type definitions are excluded as there
//! is no place in source code where they are defined) is also represented by its location in the
//! source code (DefLoc struct): line, starting column and a hash of the source file where it's
//! located. The symbolication process maps each identifier with its definition, and also computes
//! other relevant information for each identifier, such as location of its type and information
//! that should be displayed on hover. All this information for an identifier is stored in the
//! UseDef struct.

//! All UseDefs for a given module are stored in a per module map keyed on the line number where the
//! identifier represented by a given UseDef is located - the map entry contains a set of UseDef-s
//! ordered by the column where the identifier starts.
//!
//! For example consider the following code fragment (0-based line numbers on the left and 0-based
//! column numbers at the bottom):
//!
//! 7: const SOME_CONST: u64 = 42;
//! 8:
//! 9: SOME_CONST + SOME_CONST
//!    |     |  |   | |      |
//!    0     6  9  13 15    22
//!
//! Symbolication information for this code fragment would look as follows assuming that this code
//! is stored in a file with hash FHASH (we omit on-hover, type def and doc string info here; also
//! note that identifier in the definition of the constant maps to itself):
//!
//! [7] -> [UseDef(col_start:6,  col_end:13, DefLoc(7:6, FHASH))]
//! [9] -> [UseDef(col_start:0,  col_end: 9, DefLoc(7:6, FHASH))],
//!        [UseDef(col_start:13, col_end:22, DefLoc(7:6, FHASH))]
//!
//! We also associate all uses of an identifier with its definition to support
//! go-to-references. This is done in a global map from an identifier location (DefLoc) to a set of
//! use locations (UseLoc).
#![allow(clippy::non_canonical_partial_ord_impl)]

use crate::{
    analysis::{
        DefMap, find_datatype, parsing_analysis::parsing_mod_def_to_map_key, run_parsing_analysis,
        run_typing_analysis,
    },
    compiler_info::CompilerInfo,
    symbols::{
        compilation::{
            CachedPackages, CachedPkgInfo, CompiledPkgInfo, CompiledProgram, ParsedDefinitions,
            SymbolsComputationData, get_compiled_pkg,
        },
        cursor::CursorContext,
        def_info::{DefInfo, FunType, VariantInfo},
        ide_strings::{const_val_to_ide_string, mod_ident_to_ide_string},
        mod_defs::{FieldDef, MemberDef, MemberDefInfo, ModuleDefs},
        use_def::{References, UseDef, UseDefMap},
    },
    utils::{expansion_mod_ident_to_map_key, loc_start_to_lsp_position_opt, lsp_position_to_loc},
};

use anyhow::Result;
use lsp_types::{Diagnostic, Position};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
    vec,
};
use vfs::VfsPath;

use move_command_line_common::files::FileHash;
use move_compiler::{
    editions::{Edition, FeatureGate},
    expansion::ast::{self as E, ModuleIdent, ModuleIdent_, Visibility},
    linters::LintLevel,
    naming::ast::{DatatypeTypeParameter, StructFields, Type, Type_, TypeName_, VariantFields},
    parser::ast::{self as P, DocComment},
    shared::{
        Identifier, NamedAddressMap, files::MappedFiles,
        stdlib_definitions::UNIT_TEST_POISON_INJECTION_NAME, unique_map::UniqueMap,
    },
    typing::ast::ModuleDefinition,
};
use move_ir_types::location::*;
use move_package::source_package::parsed_manifest::Dependencies;
use move_symbol_pool::Symbol;

pub mod compilation;
pub mod cursor;
pub mod def_info;
pub mod ide_strings;
pub mod mod_defs;
pub mod requests;
pub mod runner;
pub mod use_def;

/// Result of the symbolication process
#[derive(Debug, Clone)]
pub struct Symbols {
    /// A map from def locations to all the references (uses)
    pub references: References,
    /// A mapping from uses to definitions in a file
    pub file_use_defs: FileUseDefs,
    /// A mapping from filePath to ModuleDefs
    pub file_mods: FileModules,
    /// Mapped file information for translating locations into positions
    pub files: MappedFiles,
    /// Additional information about definitions
    pub def_info: DefMap,
    /// IDE Annotation Information from the Compiler
    pub compiler_info: CompilerInfo,
    /// Cursor information gathered up during analysis
    pub cursor_context: Option<CursorContext>,
}

/// Information about field order in structs and enums needed for auto-completion
/// to be consistent with field order in the source code
#[derive(Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
struct FieldOrderInfo {
    pub structs: BTreeMap<String, StructFieldOrderInfo>,
    pub variants: BTreeMap<String, VariantFieldOrderInfo>,
}

/// Map from struct name to field order information
pub type StructFieldOrderInfo = BTreeMap<Symbol, BTreeMap<Symbol, usize>>;

/// Map from enum name to variant name to field order information
pub type VariantFieldOrderInfo = BTreeMap<Symbol, BTreeMap<Symbol, BTreeMap<Symbol, usize>>>;

type FileUseDefs = BTreeMap<PathBuf, UseDefMap>;

pub type FileModules = BTreeMap<PathBuf, BTreeSet<ModuleDefs>>;

/// Main driver to get symbols for the whole package. Returned symbols is an option as only the
/// correctly computed symbols should be a replacement for the old set - if symbols are not
/// actually (re)computed and the diagnostics are returned, the old symbolic information should
/// be retained even if it's getting out-of-date.
pub fn get_symbols(
    packages_info: Arc<Mutex<CachedPackages>>,
    ide_files_root: VfsPath,
    pkg_path: &Path,
    lint: LintLevel,
    cursor_info: Option<(&PathBuf, Position)>,
    implicit_deps: Dependencies,
) -> Result<(Option<Symbols>, BTreeMap<PathBuf, Vec<Diagnostic>>)> {
    // helper function to avoid holding the lock for too long
    let has_pkg_entry = || {
        packages_info
            .lock()
            .unwrap()
            .pkg_info
            .contains_key(pkg_path)
    };

    // If no attempt was yet made to cache symbols for this package,
    // it means that we are symboliciating it for the first time.
    // In this case, we should symbolicate twice - once to do full
    // compilation and symbolication to get all the code and symbols
    // that can be cached, and second time to actually use the cached
    // values and drop the artifacts of full compilation/symbolication
    // (or at least try to). If we don't do that, then after the first
    // symbolication we will hold on to all the full compilation/symbolication
    // artifacts which will keep memory footpring pretty high.
    let mut should_retry = !has_pkg_entry();

    loop {
        let compilation_start = Instant::now();
        let (compiled_pkg_info_opt, ide_diagnostics) = get_compiled_pkg(
            packages_info.clone(),
            ide_files_root.clone(),
            pkg_path,
            lint,
            implicit_deps.clone(),
        )?;
        eprintln!("compilation complete in: {:?}", compilation_start.elapsed());
        let Some(compiled_pkg_info) = compiled_pkg_info_opt else {
            return Ok((None, ide_diagnostics));
        };
        let analysis_start = Instant::now();
        let symbols = compute_symbols(packages_info.clone(), compiled_pkg_info, cursor_info);
        eprintln!("analysis complete in {:?}", analysis_start.elapsed());
        eprintln!("get_symbols load complete");

        if !should_retry {
            return Ok((Some(symbols), ide_diagnostics));
        }
        should_retry = false;
        eprintln!("Retrying compilation for {:?}", pkg_path);
    }
}

/// Compute symbols for a given package from the parsed and typed ASTs,
/// as well as other auxiliary data provided in `compiled_pkg_info`.
pub fn compute_symbols(
    packages_info: Arc<Mutex<CachedPackages>>,
    mut compiled_pkg_info: CompiledPkgInfo,
    cursor_info: Option<(&PathBuf, Position)>,
) -> Symbols {
    let pkg_path = compiled_pkg_info.path.clone();
    let manifest_hash = compiled_pkg_info.manifest_hash;
    let cached_dep_opt = compiled_pkg_info.cached_deps.clone();
    let dep_hashes = compiled_pkg_info.dep_hashes.clone();
    let edition = compiled_pkg_info.edition;
    let compiler_info = compiled_pkg_info.compiler_info.clone();
    let lsp_diags = compiled_pkg_info.lsp_diags.clone();
    let file_paths = compiled_pkg_info
        .mapped_files
        .file_name_mapping()
        .iter()
        .map(|(fhash, fpath)| (*fhash, fpath.clone()))
        .collect::<BTreeMap<_, _>>();
    let user_file_hashes = compiled_pkg_info
        .mapped_files
        .file_name_mapping()
        .iter()
        .map(|(fhash, fpath)| (fpath.clone(), *fhash))
        .collect::<BTreeMap<_, _>>();
    let mut symbols_computation_data = SymbolsComputationData::new();
    let typed_mod_named_address_maps = compiled_pkg_info
        .program
        .typed_modules
        .iter()
        .map(|(_, _, mdef)| (mdef.loc, mdef.named_address_map.clone()))
        .collect::<BTreeMap<_, _>>();
    let cursor_context = compute_symbols_pre_process(
        &mut symbols_computation_data,
        &mut compiled_pkg_info,
        cursor_info,
        &typed_mod_named_address_maps,
    );
    let cursor_context = compute_symbols_parsed_program(
        &mut symbols_computation_data,
        &compiled_pkg_info,
        cursor_context,
        &typed_mod_named_address_maps,
    );

    let (symbols, deps_symbols_data_opt, program) =
        compute_symbols_typed_program(symbols_computation_data, compiled_pkg_info, cursor_context);

    let mut pkg_deps = packages_info.lock().unwrap();

    if let Some(cached_deps) = cached_dep_opt {
        // we have at least compiled program available, either already cached
        // or created for the purpose of this analysis
        if let Some(deps_symbols_data) = deps_symbols_data_opt {
            // dependencies may have changed or not, but we still need to update the cache
            // with new file hashes and user program info
            pkg_deps.pkg_info.insert(
                pkg_path.clone(),
                Some(CachedPkgInfo {
                    manifest_hash,
                    dep_hashes,
                    deps: cached_deps.program_deps.clone(),
                    dep_names: cached_deps.dep_names.clone(),
                    deps_symbols_data,
                    program: Arc::new(program),
                    file_paths: Arc::new(file_paths),
                    user_file_hashes: Arc::new(user_file_hashes),
                    edition,
                    compiler_info,
                    lsp_diags,
                }),
            );
        }
    }
    // record attempt at caching as some actions are taken
    // on the first attempt to symbolicate and we need to
    // know once this attempt is complete
    pkg_deps.pkg_info.entry(pkg_path).or_insert(None);
    symbols
}

/// Preprocess parsed and typed programs prior to actual symbols computation.
pub fn compute_symbols_pre_process(
    computation_data: &mut SymbolsComputationData,
    compiled_pkg_info: &mut CompiledPkgInfo,
    cursor_info: Option<(&PathBuf, Position)>,
    typed_mod_named_address_maps: &BTreeMap<Loc, Arc<NamedAddressMap>>,
) -> Option<CursorContext> {
    let mut fields_order_info = FieldOrderInfo::new();
    let parsed_program = &compiled_pkg_info.program.parsed_definitions;
    let typed_program_modules = &compiled_pkg_info.program.typed_modules;
    pre_process_parsed_program(
        parsed_program,
        &mut fields_order_info,
        typed_mod_named_address_maps,
    );

    let mut cursor_context = compute_cursor_context(&compiled_pkg_info.mapped_files, cursor_info);
    pre_process_typed_modules(
        typed_program_modules,
        &fields_order_info,
        &compiled_pkg_info.mapped_files,
        &mut computation_data.mod_outer_defs,
        &mut computation_data.mod_use_defs,
        &mut computation_data.references,
        &mut computation_data.def_info,
        &compiled_pkg_info.edition,
        cursor_context.as_mut(),
    );

    if let Some(cached_deps) = compiled_pkg_info.cached_deps.clone() {
        if let Some(cached_symbols_data) = cached_deps.symbols_data {
            // We need to update definitions for the code being currently processed
            // so that these definitions are available when ASTs for this code are visited
            computation_data
                .mod_outer_defs
                .extend(cached_symbols_data.mod_outer_defs.clone());
            computation_data
                .def_info
                .extend(cached_symbols_data.def_info.clone());
        }
    }

    cursor_context
}

/// Process parsed program for symbols computation.
pub fn compute_symbols_parsed_program(
    computation_data: &mut SymbolsComputationData,
    compiled_pkg_info: &CompiledPkgInfo,
    mut cursor_context: Option<CursorContext>,
    typed_mod_named_address_maps: &BTreeMap<Loc, Arc<NamedAddressMap>>,
) -> Option<CursorContext> {
    run_parsing_analysis(
        computation_data,
        compiled_pkg_info,
        cursor_context.as_mut(),
        &compiled_pkg_info.program.parsed_definitions,
        typed_mod_named_address_maps,
    );
    cursor_context
}

/// Process typed program for symbols computation. Returns:
/// - computed symbols
/// - optional cacheable symbols data (obtained either from cache or recomputed)
/// - compiled user program
pub fn compute_symbols_typed_program(
    computation_data: SymbolsComputationData,
    mut compiled_pkg_info: CompiledPkgInfo,
    cursor_context: Option<CursorContext>,
) -> (
    Symbols,
    Option<Arc<SymbolsComputationData>>,
    CompiledProgram,
) {
    // run typing analysis for the main user program
    let compiler_info = &mut compiled_pkg_info.compiler_info.as_mut().unwrap();
    let mapped_files = &compiled_pkg_info.mapped_files;
    let mut computation_data = run_typing_analysis(
        computation_data,
        mapped_files,
        compiler_info,
        &compiled_pkg_info.program.typed_modules,
    );
    let mut file_use_defs = BTreeMap::new();
    update_file_use_defs(&computation_data, mapped_files, &mut file_use_defs);

    let deps_symbols_data_opt = if let Some(cached_deps) = compiled_pkg_info.cached_deps.clone() {
        let deps_symbols_data = if let Some(cached_symbols_data) = cached_deps.symbols_data {
            // We have cached results of the dependency symbols computation from the previous run.
            // Create `file_use_defs` map and merge references to produce complete symbols data
            // (mod_outer_defs and def_info have already been merged to facilitate user program
            // analysis).
            update_file_use_defs(&cached_symbols_data, mapped_files, &mut file_use_defs);
            for (def_loc, uses) in &cached_symbols_data.references {
                computation_data
                    .references
                    .entry(*def_loc)
                    .or_default()
                    .extend(uses);
            }
            cached_symbols_data
        } else {
            // No cached dependency symbols which means that dependency symbools
            // and user-level symbols are all in `computation_data`
            let dep_mod_ident_strs = computation_data
                .mod_outer_defs
                .iter()
                .filter_map(|(mod_ident_str, mod_defs)| {
                    if cached_deps.dep_hashes.contains(&mod_defs.fhash) {
                        Some(mod_ident_str)
                    } else {
                        None
                    }
                })
                .collect::<BTreeSet<_>>();

            /// macro to filter computation data fields based on a predicate
            /// determining if computation data belongs to a dependency
            macro_rules! filter_computation_data {
                ($data:expr, $field:ident, $predicate:expr) => {
                    $data
                        .$field
                        .clone()
                        .into_iter()
                        .filter($predicate)
                        .collect()
                };
            }

            let deps_computation_data = SymbolsComputationData {
                mod_outer_defs: filter_computation_data!(
                    computation_data,
                    mod_outer_defs,
                    |(mod_ident_str, _)| dep_mod_ident_strs.contains(mod_ident_str)
                ),
                mod_use_defs: filter_computation_data!(
                    computation_data,
                    mod_use_defs,
                    |(mod_ident_str, _)| dep_mod_ident_strs.contains(mod_ident_str)
                ),
                references: filter_computation_data!(computation_data, references, |(loc, _)| {
                    cached_deps.dep_hashes.contains(&loc.file_hash())
                }),
                def_info: filter_computation_data!(computation_data, def_info, |(loc, _)| {
                    cached_deps.dep_hashes.contains(&loc.file_hash())
                }),
                mod_to_alias_lengths: filter_computation_data!(
                    computation_data,
                    mod_to_alias_lengths,
                    |(mod_ident_str, _)| dep_mod_ident_strs.contains(mod_ident_str)
                ),
            };
            Arc::new(deps_computation_data)
        };
        Some(deps_symbols_data)
    } else {
        None
    };

    let mut file_mods: FileModules = BTreeMap::new();
    for d in computation_data.mod_outer_defs.into_values() {
        let path = compiled_pkg_info.mapped_files.file_path(&d.fhash.clone());
        file_mods.entry(path.to_path_buf()).or_default().insert(d);
    }

    (
        Symbols {
            references: computation_data.references,
            file_use_defs,
            file_mods,
            def_info: computation_data.def_info,
            files: compiled_pkg_info.mapped_files,
            compiler_info: compiled_pkg_info.compiler_info.unwrap(),
            cursor_context,
        },
        deps_symbols_data_opt,
        compiled_pkg_info.program,
    )
}

// Given use-defs for a the main program or dependencies, update the per-file
// use-def map
fn update_file_use_defs(
    computation_data: &SymbolsComputationData,
    mapped_files: &MappedFiles,
    file_use_defs: &mut FileUseDefs,
) {
    for (module_ident_str, use_defs) in &computation_data.mod_use_defs {
        // unwrap here is safe as all modules in a given program have the module_defs entry
        // in the map
        let module_defs = computation_data
            .mod_outer_defs
            .get(module_ident_str)
            .unwrap();
        let fpath = match mapped_files.file_name_mapping().get(&module_defs.fhash) {
            Some(p) => p.as_path().to_string_lossy().to_string(),
            None => return,
        };

        let fpath_buffer =
            dunce::canonicalize(fpath.clone()).unwrap_or_else(|_| PathBuf::from(fpath.as_str()));

        file_use_defs
            .entry(fpath_buffer)
            .or_default()
            .extend(use_defs.clone().elements());
    }
}

fn compute_cursor_context(
    mapped_files: &MappedFiles,
    cursor_info: Option<(&PathBuf, Position)>,
) -> Option<CursorContext> {
    let (path, pos) = cursor_info?;
    let file_hash = mapped_files.file_hash(path)?;
    let loc = lsp_position_to_loc(mapped_files, file_hash, &pos)?;
    eprintln!("computed cursor loc");
    Some(CursorContext::new(loc))
}

/// Pre-process parsed program to get initial info before AST traversals
fn pre_process_parsed_program(
    prog: &ParsedDefinitions,
    fields_order_info: &mut FieldOrderInfo,
    typed_mod_named_address_maps: &BTreeMap<Loc, Arc<NamedAddressMap>>,
) {
    prog.source_definitions.iter().for_each(|pkg_def| {
        pre_process_parsed_pkg(pkg_def, fields_order_info, typed_mod_named_address_maps);
    });
    prog.lib_definitions.iter().for_each(|pkg_def| {
        pre_process_parsed_pkg(pkg_def, fields_order_info, typed_mod_named_address_maps);
    });
}

/// Pre-process parsed package to get initial info before AST traversals
fn pre_process_parsed_pkg(
    pkg_def: &P::PackageDefinition,
    fields_order_info: &mut FieldOrderInfo,
    typed_mod_named_address_maps: &BTreeMap<Loc, Arc<NamedAddressMap>>,
) {
    if let P::Definition::Module(mod_def) = &pkg_def.def {
        // when doing full standalone compilation (vs. pre-compiling dependencies)
        // we may have a module at parsing but no longer at typing
        // in case there is a name conflict with a dependency (and
        // mod_named_address_maps comes from typing modules)
        let Some(pkg_addresses) = typed_mod_named_address_maps.get(&mod_def.loc) else {
            eprintln!(
                "no typing-level named address maps for module {}",
                mod_def.name.value(),
            );
            return;
        };
        let Some(mod_ident_str) = parsing_mod_def_to_map_key(pkg_addresses.clone(), mod_def) else {
            return;
        };
        for member in &mod_def.members {
            if let P::ModuleMember::Struct(sdef) = member {
                if let P::StructFields::Named(fields) = &sdef.fields {
                    let indexed_fields = fields
                        .iter()
                        .enumerate()
                        .map(|(i, (_, f, _))| (f.value(), i))
                        .collect::<BTreeMap<_, _>>();
                    fields_order_info
                        .structs
                        .entry(mod_ident_str.clone())
                        .or_default()
                        .entry(sdef.name.value())
                        .or_default()
                        .extend(indexed_fields);
                }
            }
            if let P::ModuleMember::Enum(edef) = member {
                for vdef in &edef.variants {
                    if let P::VariantFields::Named(fields) = &vdef.fields {
                        let indexed_fields = fields
                            .iter()
                            .enumerate()
                            .map(|(i, (_, f, _))| (f.value(), i))
                            .collect::<BTreeMap<_, _>>();
                        fields_order_info
                            .variants
                            .entry(mod_ident_str.clone())
                            .or_default()
                            .entry(edef.name.value())
                            .or_default()
                            .entry(vdef.name.value())
                            .or_default()
                            .extend(indexed_fields);
                    }
                }
            }
        }
    }
}

fn pre_process_typed_modules(
    typed_modules: &UniqueMap<ModuleIdent, ModuleDefinition>,
    fields_order_info: &FieldOrderInfo,
    files: &MappedFiles,
    mod_outer_defs: &mut BTreeMap<String, ModuleDefs>,
    mod_use_defs: &mut BTreeMap<String, UseDefMap>,
    references: &mut References,
    def_info: &mut DefMap,
    edition: &Option<Edition>,
    mut cursor_context: Option<&mut CursorContext>,
) {
    for (pos, module_ident, module_def) in typed_modules {
        // If the cursor is in this module, mark that down.
        if let Some(cursor) = &mut cursor_context {
            if module_def.loc.contains(&cursor.loc) {
                cursor.module = Some(sp(pos, *module_ident));
            }
        };

        let mod_ident_str = expansion_mod_ident_to_map_key(module_ident);
        let (defs, symbols) = get_mod_outer_defs(
            &pos,
            &sp(pos, *module_ident),
            mod_ident_str.clone(),
            module_def,
            fields_order_info,
            files,
            references,
            def_info,
            edition,
        );
        mod_outer_defs.insert(mod_ident_str.clone(), defs);
        mod_use_defs.insert(mod_ident_str, symbols);
    }
}

/// Converts parsing AST's `LeadingNameAccess` to expansion AST's `Address` (similarly to
/// expansion::translate::top_level_address but disregarding the name portion of `Address` as we
/// only care about actual address here if it's available). We need this to be able to reliably
/// compare parsing AST's module identifier with expansion/typing AST's module identifier, even in
/// presence of module renaming (i.e., we cannot rely on module names if addresses are available).
pub fn parsed_address(ln: P::LeadingNameAccess, pkg_addresses: Arc<NamedAddressMap>) -> E::Address {
    let sp!(loc, ln_) = ln;
    match ln_ {
        P::LeadingNameAccess_::AnonymousAddress(bytes) => E::Address::anonymous(loc, bytes),
        P::LeadingNameAccess_::GlobalAddress(name) => E::Address::NamedUnassigned(name),
        P::LeadingNameAccess_::Name(name) => match pkg_addresses.get(&name.value).copied() {
            // set `name_conflict` to `true` to force displaying (addr==pkg_name) so that the string
            // representing map key is consistent with what's generated for expansion ModuleIdent in
            // `expansion_mod_ident_to_map_key`
            Some(addr) => E::Address::Numerical {
                name: Some(name),
                value: sp(loc, addr),
                name_conflict: true,
            },
            None => E::Address::NamedUnassigned(name),
        },
    }
}

/// Get empty symbols
pub fn empty_symbols() -> Symbols {
    Symbols {
        file_use_defs: BTreeMap::new(),
        references: BTreeMap::new(),
        file_mods: BTreeMap::new(),
        def_info: BTreeMap::new(),
        files: MappedFiles::empty(),
        compiler_info: CompilerInfo::new(),
        cursor_context: None,
    }
}

fn field_defs_and_types(
    datatype_name: Symbol,
    fields: &E::Fields<(DocComment, Type)>,
    fields_order_opt: Option<&BTreeMap<Symbol, usize>>,
    mod_ident: &ModuleIdent,
    def_info: &mut DefMap,
) -> (Vec<FieldDef>, Vec<Type>) {
    let mut field_defs = vec![];
    let mut field_types = vec![];
    let mut ordered_fields = fields
        .iter()
        .map(|(floc, fname, (_, (fdoc, ftype)))| (floc, fdoc, fname, ftype))
        .collect::<Vec<_>>();
    // sort fields by order if available for correct auto-completion
    if let Some(fields_order) = fields_order_opt {
        ordered_fields.sort_by_key(|(_, _, fname, _)| fields_order.get(fname).copied());
    }
    for (floc, fdoc, fname, ftype) in ordered_fields {
        field_defs.push(FieldDef {
            name: *fname,
            loc: floc,
        });
        let doc_string = fdoc.comment().map(|d| d.value.to_owned());
        def_info.insert(
            floc,
            DefInfo::Field(
                mod_ident.value,
                datatype_name,
                *fname,
                ftype.clone(),
                doc_string,
            ),
        );
        field_types.push(ftype.clone());
    }
    (field_defs, field_types)
}

fn datatype_type_params(data_tparams: &[DatatypeTypeParameter]) -> Vec<(Type, /* phantom */ bool)> {
    data_tparams
        .iter()
        .map(|t| {
            (
                sp(
                    t.param.user_specified_name.loc,
                    Type_::Param(t.param.clone()),
                ),
                t.is_phantom,
            )
        })
        .collect()
}

/// Some functions defined in a module need to be ignored.
pub fn ignored_function(name: Symbol) -> bool {
    // In test mode (that's how IDE compiles Move source files), the compiler inserts an dummy
    // function preventing publishing of modules compiled in test mode. We need to ignore its
    // definition to avoid spurious on-hover display of this function's info whe hovering close to
    // `module` keyword.
    name == UNIT_TEST_POISON_INJECTION_NAME
}

/// Get symbols for outer definitions in the module (functions, structs, and consts)
fn get_mod_outer_defs(
    loc: &Loc,
    mod_ident: &ModuleIdent,
    mod_ident_str: String,
    mod_def: &ModuleDefinition,
    fields_order_info: &FieldOrderInfo,
    files: &MappedFiles,
    references: &mut References,
    def_info: &mut DefMap,
    edition: &Option<Edition>,
) -> (ModuleDefs, UseDefMap) {
    let mut structs = BTreeMap::new();
    let mut enums = BTreeMap::new();
    let mut constants = BTreeMap::new();
    let mut functions = BTreeMap::new();

    let fhash = loc.file_hash();
    let mut positional = false;
    for (name_loc, name, def) in &mod_def.structs {
        // process struct fields first
        let mut field_defs = vec![];
        let mut field_types = vec![];
        if let StructFields::Defined(pos_fields, fields) = &def.fields {
            positional = *pos_fields;
            let fields_order_opt = fields_order_info
                .structs
                .get(&mod_ident_str)
                .and_then(|s| s.get(name));
            (field_defs, field_types) =
                field_defs_and_types(*name, fields, fields_order_opt, mod_ident, def_info);
        };

        // process the struct itself
        let field_names = field_defs.iter().map(|f| sp(f.loc, f.name)).collect();
        structs.insert(
            *name,
            MemberDef {
                name_loc,
                info: MemberDefInfo::Struct {
                    field_defs,
                    positional,
                },
            },
        );
        let pub_struct = edition
            .map(|e| e.supports(FeatureGate::PositionalFields))
            .unwrap_or(false);
        let visibility = if pub_struct {
            // fake location OK as this is for display purposes only
            Visibility::Public(Loc::invalid())
        } else {
            Visibility::Internal
        };
        let doc_string = def.doc.comment().map(|d| d.value.to_owned());
        def_info.insert(
            name_loc,
            DefInfo::Struct(
                mod_ident.value,
                *name,
                visibility,
                datatype_type_params(&def.type_parameters),
                def.abilities.clone(),
                field_names,
                field_types,
                doc_string,
            ),
        );
    }

    for (name_loc, name, def) in &mod_def.enums {
        // process variants
        let mut variants_info = BTreeMap::new();
        let mut def_info_variants = vec![];
        for (vname_loc, vname, vdef) in &def.variants {
            let (field_defs, field_types, positional) = match &vdef.fields {
                VariantFields::Defined(pos_fields, fields) => {
                    let fields_order_opt = fields_order_info
                        .variants
                        .get(&mod_ident_str)
                        .and_then(|v| v.get(name))
                        .and_then(|v| v.get(vname));
                    let (defs, types) =
                        field_defs_and_types(*name, fields, fields_order_opt, mod_ident, def_info);
                    (defs, types, *pos_fields)
                }
                VariantFields::Empty => (vec![], vec![], false),
            };
            let field_names = field_defs.iter().map(|f| sp(f.loc, f.name)).collect();
            def_info_variants.push(VariantInfo {
                name: sp(vname_loc, *vname),
                empty: field_defs.is_empty(),
                positional,
            });
            variants_info.insert(*vname, (vname_loc, field_defs, positional));

            let vdoc_string = def.doc.comment().map(|d| d.value.to_owned());
            def_info.insert(
                vname_loc,
                DefInfo::Variant(
                    mod_ident.value,
                    *name,
                    *vname,
                    positional,
                    field_names,
                    field_types,
                    vdoc_string,
                ),
            );
        }
        // process the enum itself
        enums.insert(
            *name,
            MemberDef {
                name_loc,
                info: MemberDefInfo::Enum { variants_info },
            },
        );
        let enum_doc_string = def.doc.comment().map(|d| d.value.to_owned());
        def_info.insert(
            name_loc,
            DefInfo::Enum(
                mod_ident.value,
                *name,
                Visibility::Public(Loc::invalid()),
                datatype_type_params(&def.type_parameters),
                def.abilities.clone(),
                def_info_variants,
                enum_doc_string,
            ),
        );
    }

    for (name_loc, name, c) in &mod_def.constants {
        constants.insert(
            *name,
            MemberDef {
                name_loc,
                info: MemberDefInfo::Const,
            },
        );
        let doc_string = c.doc.comment().map(|d| d.value.to_owned());
        def_info.insert(
            name_loc,
            DefInfo::Const(
                mod_ident.value,
                *name,
                c.signature.clone(),
                const_val_to_ide_string(&c.value),
                doc_string,
            ),
        );
    }

    for (name_loc, name, fun) in &mod_def.functions {
        if ignored_function(*name) {
            continue;
        }
        let fun_type = if fun.entry.is_some() {
            FunType::Entry
        } else if fun.macro_.is_some() {
            FunType::Macro
        } else {
            FunType::Regular
        };
        let doc_string = fun.doc.comment().map(|d| d.value.to_owned());
        let fun_info = DefInfo::Function(
            mod_ident.value,
            fun.visibility,
            fun_type,
            *name,
            fun.signature
                .type_parameters
                .iter()
                .map(|t| (sp(t.user_specified_name.loc, Type_::Param(t.clone()))))
                .collect(),
            fun.signature
                .parameters
                .iter()
                .map(|(_, n, _)| sp(n.loc, n.value.name))
                .collect(),
            fun.signature
                .parameters
                .iter()
                .map(|(_, _, t)| t.clone())
                .collect(),
            fun.signature.return_type.clone(),
            doc_string,
        );
        functions.insert(
            *name,
            MemberDef {
                name_loc,
                info: MemberDefInfo::Fun {
                    attrs: fun
                        .attributes
                        .clone()
                        .iter()
                        .map(|(_loc, name, _attr)| name.to_string())
                        .collect(),
                },
            },
        );
        def_info.insert(name_loc, fun_info);
    }

    let mut use_def_map = UseDefMap::new();

    let ident = mod_ident.value;
    let doc_string = mod_def.doc.comment().map(|d| d.value.to_owned());

    let mod_defs = ModuleDefs {
        fhash,
        ident,
        name_loc: *loc,
        structs,
        enums,
        constants,
        functions,
        untyped_defs: BTreeSet::new(),
        call_infos: BTreeMap::new(),
        import_insert_info: None,
        neighbors: mod_def.immediate_neighbors.clone(),
    };

    // insert use of the module name in the definition itself
    let mod_name = ident.module;
    if let Some(mod_name_start) = loc_start_to_lsp_position_opt(files, &mod_name.loc()) {
        use_def_map.insert(
            mod_name_start.line,
            UseDef::new(
                references,
                &BTreeMap::new(),
                mod_name.loc().file_hash(),
                mod_name_start,
                mod_defs.name_loc,
                &mod_name.value(),
                None,
            ),
        );
        def_info.insert(
            mod_defs.name_loc,
            DefInfo::Module(mod_ident_to_ide_string(&ident, None, false), doc_string),
        );
    }

    (mod_defs, use_def_map)
}

pub fn type_def_loc(
    mod_outer_defs: &BTreeMap<String, ModuleDefs>,
    sp!(_, t): &Type,
) -> Option<Loc> {
    match t {
        Type_::Ref(_, r) => type_def_loc(mod_outer_defs, r),
        Type_::Apply(_, sp!(_, TypeName_::ModuleType(sp!(_, mod_ident), struct_name)), _) => {
            let mod_ident_str = expansion_mod_ident_to_map_key(mod_ident);
            mod_outer_defs
                .get(&mod_ident_str)
                .and_then(|mod_defs| find_datatype(mod_defs, &struct_name.value()))
        }
        _ => None,
    }
}

//**************************************************************************************************
// Impls
//**************************************************************************************************

impl Symbols {
    pub fn line_uses(&self, use_fpath: &Path, use_line: u32) -> BTreeSet<UseDef> {
        let Some(file_symbols) = self.file_use_defs.get(use_fpath) else {
            return BTreeSet::new();
        };
        file_symbols.get(use_line).unwrap_or_else(BTreeSet::new)
    }

    pub fn def_info(&self, def_loc: &Loc) -> Option<&DefInfo> {
        self.def_info.get(def_loc)
    }

    pub fn mod_defs(&self, fhash: &FileHash, mod_ident: ModuleIdent_) -> Option<&ModuleDefs> {
        let Some(fpath) = self.files.file_name_mapping().get(fhash) else {
            return None;
        };
        let Some(mod_defs) = self.file_mods.get(fpath) else {
            return None;
        };
        mod_defs.iter().find(|d| d.ident == mod_ident)
    }

    pub fn file_hash(&self, path: &Path) -> Option<FileHash> {
        let Some(mod_defs) = self.file_mods.get(path) else {
            return None;
        };
        Some(mod_defs.first().unwrap().fhash)
    }
}

impl Default for FieldOrderInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl FieldOrderInfo {
    pub fn new() -> Self {
        Self {
            structs: BTreeMap::new(),
            variants: BTreeMap::new(),
        }
    }
}
