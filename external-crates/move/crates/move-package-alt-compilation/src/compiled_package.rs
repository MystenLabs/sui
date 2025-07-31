// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_package_alt::{
    flavor::MoveFlavor,
    graph::NamedAddress,
    package::{RootPackage, paths::PackagePath},
    schema::OriginalID,
};

use colored::Colorize;

use crate::build_config::BuildConfig;
use crate::on_disk_package::{OnDiskCompiledPackage, OnDiskPackage};
use move_package_alt::package::layout::SourcePackageLayout;

use crate::layout::CompiledPackageLayout;

use anyhow::Result;
use move_binary_format::CompiledModule;
use move_bytecode_utils::Modules;
use move_command_line_common::files::{extension_equals, find_filenames, find_move_filenames};
use move_compiler::{
    Compiler, Flags,
    compiled_unit::{AnnotatedCompiledUnit, CompiledUnit},
    diagnostics::{Diagnostics, report_warnings, warning_filters::WarningFiltersBuilder},
    editions::{Edition, Flavor},
    linters::{self, LINT_WARNING_PREFIX},
    shared::{
        PackageConfig, PackagePaths, SaveFlag, SaveHook,
        files::{FileName, MappedFiles},
    },
    sui_mode,
};
use move_core_types::{account_address::AccountAddress, parsing::address::NumericalAddress};
use move_docgen::{Docgen, DocgenFlags, DocgenOptions};
use move_model_2::source_model;
use move_symbol_pool::Symbol;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    io::Write,
    path::{Path, PathBuf},
    str::FromStr,
};
use tracing::debug;
use vfs::VfsPath;

/// References file for documentation generation
pub const REFERENCE_TEMPLATE_FILENAME: &str = "references.md";

/// Represents a compiled package in memory.
#[derive(Clone, Debug)]
pub struct CompiledPackage {
    /// Meta information about the compilation of this `CompiledPackage`
    pub compiled_package_info: CompiledPackageInfo,
    /// The output compiled bytecode in the root package (both module, and scripts) along with its
    /// source file
    pub root_compiled_units: Vec<CompiledUnitWithSource>,
    /// The output compiled bytecode for dependencies
    pub deps_compiled_units: Vec<(Symbol, CompiledUnitWithSource)>,

    // Optional artifacts from compilation
    /// filename -> doctext
    pub compiled_docs: Option<Vec<(String, String)>>,
    /// The list of published ids for the dependencies of this package
    pub deps_published_ids: Vec<OriginalID>,
    /// The mapping of file hashes to file names and contents
    pub file_map: MappedFiles,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledPackageInfo {
    /// The name of the compiled package
    pub package_name: Symbol,
    /// The instantiations for all named addresses that were used for compilation
    // pub address_alias_instantiation: BTreeMap<String, String>,
    /// The hash of the source directory at the time of compilation. `None` if the source for this
    /// package is not available/this package was not compiled.
    // pub source_digest: Option<String>,
    /// The build flags that were used when compiling this package.
    pub build_flags: BuildConfig,
}

#[derive(Debug, Clone)]
pub struct CompiledUnitWithSource {
    pub unit: CompiledUnit,
    pub source_path: PathBuf,
}

impl CompiledPackage {
    /// Return an iterator over all compiled units in this package, including dependencies
    pub fn get_all_compiled_units_with_source(
        &self,
    ) -> impl Iterator<Item = &CompiledUnitWithSource> {
        self.root_compiled_units
            .iter()
            .chain(self.deps_compiled_units.iter().map(|(_, unit)| unit))
    }

    /// `root_compiled_units` filtered over `CompiledUnit::Module`
    pub fn root_modules(&self) -> impl Iterator<Item = &CompiledUnitWithSource> {
        self.root_compiled_units.iter()
    }

    /// Return an iterator over all bytecode modules in this package, including dependencies
    pub fn get_modules_and_deps(&self) -> impl Iterator<Item = &CompiledModule> {
        self.get_all_compiled_units_with_source()
            .map(|m| &m.unit.module)
    }

    /// Return an iterator over the root bytecode modules in this package, excluding dependencies
    pub fn root_modules_map(&self) -> Modules {
        Modules::new(
            self.root_compiled_units
                .iter()
                .map(|unit| &unit.unit.module),
        )
    }

    pub fn get_topological_srted_deps() {}

    /// Return the bytecode modules in this package, topologically sorted in dependency order.
    /// This is the function to call if you would like to publish or statically analyze the modules.
    pub fn get_dependency_sorted_modules(&self) -> Vec<CompiledModule> {
        let all_modules = Modules::new(self.get_modules_and_deps());

        // SAFETY: package built successfully
        let modules = all_modules.compute_topological_order().unwrap();

        // Collect all module IDs from the current package to be published (module names are not
        // sufficient as we may have modules with the same names in user code and in Sui
        // framework which would result in the latter being pulled into a set of modules to be
        // published).
        let self_modules: HashSet<_> = self
            .root_modules_map()
            .iter_modules()
            .iter()
            .map(|m| m.self_id())
            .collect();

        modules
            .filter(|module| self_modules.contains(&module.self_id()))
            .cloned()
            .collect()
    }

    /// Return a serialized representation of the bytecode modules in this package, topologically
    /// sorted in dependency order.
    pub fn get_package_bytes(&self) -> Vec<Vec<u8>> {
        self.get_dependency_sorted_modules()
            .iter()
            .map(|m| {
                let mut bytes = Vec::new();
                m.serialize_with_version(m.version, &mut bytes).unwrap(); // safe because package built successfully
                bytes
            })
            .collect()
    }
    pub fn get_module_by_name(
        &self,
        package_name: &str,
        module_name: &str,
    ) -> Result<&CompiledUnitWithSource> {
        if self.compiled_package_info.package_name.as_str() == package_name {
            return self.get_module_by_name_from_root(module_name);
        }

        self.deps_compiled_units
            .iter()
            .filter(|(dep_package, _)| dep_package.as_str() == package_name)
            .map(|(_, unit)| unit)
            .find(|unit| unit.unit.name().as_str() == module_name)
            .ok_or_else(|| {
                anyhow::format_err!(
                    "Unable to find module with name '{}' in package {}",
                    module_name,
                    self.compiled_package_info.package_name
                )
            })
    }

    pub fn get_module_by_name_from_root(
        &self,
        module_name: &str,
    ) -> Result<&CompiledUnitWithSource> {
        self.root_modules()
            .find(|unit| unit.unit.name().as_str() == module_name)
            .ok_or_else(|| {
                anyhow::format_err!(
                    "Unable to find module with name '{}' in package {}",
                    module_name,
                    self.compiled_package_info.package_name
                )
            })
    }

    /// Return the published ids of the dependencies of this package
    pub fn dependency_ids(&self) -> Vec<OriginalID> {
        self.deps_published_ids.clone()
    }
}

fn build_docs(
    docgen_flags: DocgenFlags,
    package_name: Symbol,
    model: &source_model::Model,
    package_root: &Path,
    deps: &[Symbol],
    install_dir: &Option<PathBuf>,
) -> Result<Vec<(String, String)>> {
    let root_doc_templates = find_filenames(
        &[package_root
            .join(SourcePackageLayout::DocTemplates.path())
            .to_string_lossy()
            .to_string()],
        |path| extension_equals(path, "md"),
    )
    .unwrap_or_else(|_| vec![]);
    let root_for_docs = if let Some(install_dir) = install_dir {
        install_dir.join(CompiledPackageLayout::Root.path())
    } else {
        CompiledPackageLayout::Root.path().to_path_buf()
    };
    let dep_paths = deps
        .iter()
        .map(|dep_name| {
            root_for_docs
                .join(CompiledPackageLayout::CompiledDocs.path())
                .join(dep_name.as_str())
                .to_string_lossy()
                .to_string()
        })
        .collect();
    let in_pkg_doc_path = root_for_docs
        .join(CompiledPackageLayout::CompiledDocs.path())
        .join(package_name.as_str());
    let references_path = package_root
        .join(SourcePackageLayout::DocTemplates.path())
        .join(REFERENCE_TEMPLATE_FILENAME);
    let references_file = if references_path.exists() {
        Some(references_path.to_string_lossy().to_string())
    } else {
        None
    };
    let doc_options = DocgenOptions {
        doc_path: dep_paths,
        output_directory: in_pkg_doc_path.to_string_lossy().to_string(),
        root_doc_templates,
        compile_relative_to_output_dir: true,
        references_file,
        flags: docgen_flags,
    };
    let docgen = Docgen::new(model, &doc_options);
    docgen.generate(model)
}

/// Save the compiled package to disk
fn save_to_disk(
    root_compiled_units: Vec<CompiledUnitWithSource>,
    compiled_package_info: CompiledPackageInfo,
    deps_compiled_units: Vec<(Symbol, CompiledUnitWithSource)>,
    compiled_docs: Option<Vec<(String, String)>>,
    root_package: Symbol,
    under_path: PathBuf,
) -> anyhow::Result<OnDiskCompiledPackage> {
    check_filepaths_ok(&root_compiled_units, compiled_package_info.package_name)?;
    assert!(under_path.ends_with(CompiledPackageLayout::Root.path()));
    let on_disk_package = OnDiskCompiledPackage {
        root_path: under_path.join(root_package.to_string()),
        package: OnDiskPackage {
            compiled_package_info: compiled_package_info.clone(),
            dependencies: deps_compiled_units
                .iter()
                .map(|(package_name, _)| *package_name)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        },
    };

    // Clear out the build dir for this package so we don't keep artifacts from previous
    // compilations
    if on_disk_package.root_path.is_dir() {
        std::fs::remove_dir_all(&on_disk_package.root_path)?;
    }

    std::fs::create_dir_all(&on_disk_package.root_path)?;

    for compiled_unit in root_compiled_units {
        on_disk_package.save_compiled_unit(root_package, &compiled_unit)?;
        if compiled_package_info.build_flags.save_disassembly {
            on_disk_package.save_disassembly_to_disk(root_package, &compiled_unit)?;
        }
    }
    for (dep_name, compiled_unit) in deps_compiled_units {
        let dep_name: Symbol = dep_name.as_str().into();
        on_disk_package.save_compiled_unit(dep_name, &compiled_unit)?;
        if compiled_package_info.build_flags.save_disassembly {
            on_disk_package.save_disassembly_to_disk(dep_name, &compiled_unit)?;
        }
    }

    if let Some(docs) = compiled_docs {
        for (doc_filename, doc_contents) in docs {
            on_disk_package.save_under(
                CompiledPackageLayout::CompiledDocs
                    .path()
                    .join(doc_filename)
                    .with_extension("md"),
                doc_contents.clone().as_bytes(),
            )?;
        }
    }

    on_disk_package.save_under(
        CompiledPackageLayout::BuildInfo.path(),
        serde_yaml::to_string(&on_disk_package.package)?.as_bytes(),
    )?;

    Ok(on_disk_package)
}

/// There may be additional information that needs to be displayed after diagnostics are reported
/// (optionally report diagnostics themselves if files argument is provided).
fn _decorate_warnings(warning_diags: Diagnostics, files: Option<&MappedFiles>) {
    let any_linter_warnings = warning_diags.any_with_prefix(LINT_WARNING_PREFIX);
    let (filtered_diags_num, unique) =
        warning_diags.filtered_source_diags_with_prefix(LINT_WARNING_PREFIX);
    if let Some(f) = files {
        report_warnings(f, warning_diags);
    }
    if any_linter_warnings {
        eprintln!("Please report feedback on the linter warnings at https://forums.sui.io\n");
    }
    if filtered_diags_num > 0 {
        eprintln!(
            "Total number of linter warnings suppressed: {filtered_diags_num} (unique lints: {unique})"
        );
    }
}

fn source_paths_for_config(package_path: &Path, config: &BuildConfig) -> Vec<PathBuf> {
    let mut places_to_look = Vec::new();
    let mut add_path = |layout_path: SourcePackageLayout| {
        let path = package_path.join(layout_path.path());
        if layout_path.is_optional() && !path.exists() {
            return;
        }
        places_to_look.push(path)
    };

    add_path(SourcePackageLayout::Sources);
    add_path(SourcePackageLayout::Scripts);

    if config.test_mode {
        add_path(SourcePackageLayout::Tests);
    }

    places_to_look
        .into_iter()
        .filter(|path| path.exists())
        .collect()
}

// Find all the source files for a package at the given path
fn get_sources(path: &PackagePath, config: &BuildConfig) -> Result<Vec<FileName>> {
    let places_to_look = source_paths_for_config(path.path(), config);
    Ok(find_move_filenames(&places_to_look, false)?
        .into_iter()
        .map(FileName::from)
        .collect())
}

// We take the (restrictive) view that all filesystems are case insensitive to maximize
// portability of packages.
fn check_filepaths_ok(
    root_compiled_units: &Vec<CompiledUnitWithSource>,
    package_name: Symbol,
) -> Result<()> {
    // A mapping of (lowercase_name => [info_for_each_occurence]
    let mut insensitive_mapping = BTreeMap::new();
    for compiled_unit in root_compiled_units {
        let name = compiled_unit.unit.name.as_str();
        let entry = insensitive_mapping
            .entry(name.to_lowercase())
            .or_insert_with(Vec::new);
        entry.push((
            name,
            compiled_unit.source_path.to_string_lossy().to_string(),
        ));
    }
    let errs = insensitive_mapping
            .into_iter()
            .filter_map(|(insensitive_name, occurence_infos)| {
                if occurence_infos.len() > 1 {
                    let name_conflict_error_msg = occurence_infos
                        .into_iter()
                        .map(|(name,  fpath)| {
                                format!(
                                    "\tModule '{}' at path '{}'",
                                    name,
                                    fpath
                                )
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    Some(format!(
                        "The following modules and/or scripts would collide as '{}' on the file system:\n{}",
                        insensitive_name, name_conflict_error_msg
                    ))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
    if !errs.is_empty() {
        anyhow::bail!(
            "Module and/or script names found that would cause failures on case insensitive \
                file systems when compiling package '{}':\n{}\nPlease rename these scripts and/or modules to resolve these conflicts.",
            package_name,
            errs.join("\n"),
        )
    }
    Ok(())
}

fn compiler_flags(build_config: &BuildConfig) -> Flags {
    let flags = if build_config.test_mode {
        Flags::testing()
    } else {
        Flags::empty()
    };
    flags
        .set_warnings_are_errors(build_config.warnings_are_errors)
        .set_json_errors(build_config.json_errors)
        .set_silence_warnings(build_config.silence_warnings)
}

pub fn build_all<W: Write, F: MoveFlavor>(
    w: &mut W,
    vfs_root: Option<VfsPath>,
    root_pkg: RootPackage<F>,
    build_config: &BuildConfig,
    compiler_driver: impl FnOnce(Compiler) -> Result<(MappedFiles, Vec<AnnotatedCompiledUnit>)>,
) -> Result<CompiledPackage> {
    let deps_published_ids = root_pkg.deps_published_ids().clone();
    let project_root = root_pkg.path().as_ref().to_path_buf();
    let program_info_hook = SaveHook::new([SaveFlag::TypingInfo]);
    let package_name = Symbol::from(root_pkg.name().as_str());
    let (file_map, all_compiled_units) =
        build_for_driver(w, vfs_root, build_config, root_pkg, |compiler| {
            let compiler = compiler.add_save_hook(&program_info_hook);
            compiler_driver(compiler)
        })?;

    let mut all_compiled_units_vec = vec![];
    let mut root_compiled_units = vec![];
    let mut deps_compiled_units = vec![];

    // TODO: improve/rework this? Renaming the root pkg to have a unique name for the compiler
    // this has to match whatever we're doing in build_for_driver function
    let root_package_name = Symbol::from(format!("{}_root", package_name.as_str()));

    for mut annot_unit in all_compiled_units {
        let source_path = PathBuf::from(
            file_map
                .get(&annot_unit.loc().file_hash())
                .unwrap()
                .0
                .as_str(),
        );
        let package_name = annot_unit.named_module.package_name.unwrap();
        // unwraps below are safe as the source path exists (or must have existed at some point)
        // so it would be syntactically correct
        let file_name = PathBuf::from(source_path.file_name().unwrap());
        if let Ok(p) = dunce::canonicalize(source_path.parent().unwrap()) {
            annot_unit
                .named_module
                .source_map
                .set_from_file_path(p.join(file_name));
        }
        let unit = CompiledUnitWithSource {
            unit: annot_unit.named_module,
            source_path,
        };
        if package_name == root_package_name {
            root_compiled_units.push(unit.clone())
        } else {
            deps_compiled_units.push((package_name, unit.clone()))
        }
        all_compiled_units_vec.push((unit.source_path, unit.unit));
    }

    let mut compiled_docs = None;

    // TODO: probably we want a separate command for this rather than doing it as part of
    // compilation
    if build_config.generate_docs {
        // TODO: fix this root_name_address_map
        let root_named_address_map = BTreeMap::new();
        let program_info = program_info_hook.take_typing_info();
        let model = source_model::Model::from_source(
            file_map.clone(),
            Some(root_package_name),
            root_named_address_map,
            program_info,
            all_compiled_units_vec,
        )?;

        compiled_docs = Some(build_docs(
            DocgenFlags::default(), // TODO this should be configurable
            root_package_name,
            &model,
            &project_root,
            //TODO Fix this, it needs immediate dependencies for this pkg
            &[],
            // &immediate_dependencies,
            &build_config.install_dir,
        )?);
    };

    let compiled_package_info = CompiledPackageInfo {
        package_name: root_package_name,
        // // TODO: correct address alias instantiation
        // address_alias_instantiation: BTreeMap::new(),
        // TODO: compute source digest
        // source_digest: None,
        build_flags: build_config.clone(),
    };

    let under_path = project_root.join("build");

    save_to_disk(
        root_compiled_units.clone(),
        compiled_package_info.clone(),
        deps_compiled_units.clone(),
        compiled_docs,
        root_package_name,
        under_path,
    )?;

    let compiled_package = CompiledPackage {
        compiled_package_info,
        root_compiled_units,
        deps_compiled_units,
        compiled_docs: None,
        deps_published_ids,
        file_map,
        // compiled_docs,
    };

    Ok(compiled_package)
}

#[allow(unreachable_code)] // TODO
pub(crate) fn build_for_driver<W: Write, T, F: MoveFlavor>(
    w: &mut W,
    vfs_root: Option<VfsPath>,
    build_config: &BuildConfig,
    root_pkg: RootPackage<F>,
    compiler_driver: impl FnOnce(Compiler) -> Result<T>,
) -> Result<T> {
    let packages = root_pkg.packages()?;

    let mut package_paths: Vec<PackagePaths> = vec![];

    for (counter, pkg) in packages.into_iter().enumerate() {
        let name: Symbol = pkg.name().as_str().into();

        if !pkg.is_root() {
            writeln!(w, "{} {name}", "INCLUDING DEPENDENCY".bold().green())?;
        }

        let mut addresses: BTreeMap<Symbol, NumericalAddress> = BTreeMap::new();
        for (dep_name, dep) in pkg.named_addresses()? {
            let name = dep_name.as_str().into();

            let addr = match dep {
                NamedAddress::RootPackage(_) => AccountAddress::ZERO,
                NamedAddress::Unpublished { dummy_addr } => {
                    writeln!(
                        w,
                        "{} Using address 0x{} for unpublished dependency `{name}` in package `{}`",
                        "NOTE".bold().yellow(),
                        dummy_addr.0.short_str_lossless(),
                        pkg.name()
                    )?;
                    dummy_addr.0
                }
                NamedAddress::Defined(original_id) => original_id.0,
            };

            let addr: NumericalAddress =
                NumericalAddress::new(addr.into_bytes(), move_compiler::shared::NumberFormat::Hex);
            addresses.insert(name, addr);
        }

        // TODO: better default handling for edition and flavor
        let config = PackageConfig {
            is_dependency: !pkg.is_root(),
            edition: Edition::from_str(pkg.edition())?,
            flavor: Flavor::from_str(pkg.flavor().unwrap_or("sui"))?,
            warning_filter: WarningFiltersBuilder::new_for_source(),
        };

        // TODO: improve/rework this? Renaming the root pkg to have a unique name for the compiler
        let safe_name = if pkg.is_root() {
            Symbol::from(format!("{}_root", name))
        } else {
            Symbol::from(format!("{}_{}", name, counter))
        };

        debug!("Package name {:?} -- Safe name {:?}", name, safe_name);
        debug!("Named address map {:#?}", addresses);
        let paths = PackagePaths {
            name: Some((safe_name, config)),
            paths: get_sources(pkg.path(), build_config)?,
            named_address_map: addresses,
        };

        package_paths.push(paths);
    }

    debug!("Package paths {:#?}", package_paths);

    writeln!(w, "{} {}", "BUILDING".bold().green(), root_pkg.name())?;

    let lint_level = build_config.lint_flag.get();
    let sui_mode = build_config.default_flavor == Some(Flavor::Sui);
    let flags = compiler_flags(build_config);

    let mut compiler = Compiler::from_package_paths(vfs_root, package_paths, vec![])
        .unwrap()
        .set_flags(flags);
    if sui_mode {
        let (filter_attr_name, filters) = sui_mode::linters::known_filters();
        compiler = compiler
            .add_custom_known_filters(filter_attr_name, filters)
            .add_visitors(sui_mode::linters::linter_visitors(lint_level))
    }
    let (filter_attr_name, filters) = linters::known_filters();
    compiler = compiler
        .add_custom_known_filters(filter_attr_name, filters)
        .add_visitors(linters::linter_visitors(lint_level));

    compiler_driver(compiler)
}
