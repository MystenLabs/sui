// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    compilation::on_disk_package::OnDiskPackage,
    flavor::MoveFlavor,
    package::{EnvironmentName, RootPackage, paths::PackagePath},
};

use crate::schema::PublishedID;

use move_core_types::account_address::AccountAddress;

use super::{
    build_config::BuildConfig, on_disk_package::OnDiskCompiledPackage,
    package_layout::CompiledPackageLayout, source_layout::SourcePackageLayout,
};
use anyhow::{Result, bail};
use move_binary_format::CompiledModule;
use move_bytecode_utils::Modules;
use move_command_line_common::files::{extension_equals, find_filenames, find_move_filenames};
use move_compiler::{
    compiled_unit::CompiledUnit,
    diagnostics::{
        Diagnostics, report_diagnostics_to_buffer, report_warnings,
        warning_filters::WarningFiltersBuilder,
    },
    linters::LINT_WARNING_PREFIX,
    shared::{
        PackageConfig, PackagePaths, SaveFlag, SaveHook,
        files::{FileName, MappedFiles},
    },
};
use move_core_types::parsing::address::NumericalAddress;
use move_docgen::{Docgen, DocgenFlags, DocgenOptions};
use move_model_2::source_model;
use move_symbol_pool::Symbol;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    io::Write,
    path::{Path, PathBuf},
};
use tracing::debug;

/// References file for documentation generation
pub const REFERENCE_TEMPLATE_FILENAME: &str = "references.md";

/// Represents a compiled package in memory.
pub struct CompiledPackage {
    /// Meta information about the compilation of this `CompiledPackage`
    compiled_package_info: CompiledPackageInfo,
    /// The output compiled bytecode in the root package (both module, and scripts) along with its
    /// source file
    root_compiled_units: Vec<CompiledUnitWithSource>,
    /// The output compiled bytecode for dependencies
    deps_compiled_units: Vec<(Symbol, CompiledUnitWithSource)>,

    // Optional artifacts from compilation
    //
    /// filename -> doctext
    compiled_docs: Option<Vec<(String, String)>>,
    /// The list of published ids for the dependencies of this package
    deps_published_ids: Vec<PublishedID>,
    /// The mapping of file hashes to file names and contents
    file_map: MappedFiles,
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

    /// Return the published ids of the dependencies of this package
    pub fn dependency_ids(&self) -> Vec<PublishedID> {
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

pub async fn compile<F: MoveFlavor>(
    // TODO: how does this work?
    // vfs_root: Option<&Path>,
    root_pkg: &RootPackage<F>,
    build_config: &BuildConfig,
    env: &EnvironmentName,
) -> Result<CompiledPackage> {
    // TODO: refactor this
    let pkgs = BTreeSet::from(["Sui", "SuiSystem", "MoveStdlib"]);
    let names = BTreeMap::from([
        ("Sui", "sui"),
        ("SuiSystem", "sui_system"),
        ("MoveStdlib", "std"),
    ]);

    let mut named_address_map: BTreeMap<Symbol, NumericalAddress> = BTreeMap::new();
    let root_pkg_paths = find_move_filenames(&[root_pkg.package_path().path()], false)
        .unwrap()
        .into_iter()
        .map(FileName::from)
        .collect::<Vec<_>>();

    let program_info_hook = SaveHook::new([SaveFlag::TypingInfo]);

    let mut published_ids = vec![];

    if let Some(dependency_graph) = &root_pkg.dependencies().get(env) {
        for node in dependency_graph.nodes() {
            if node.name() == root_pkg.package_name() {
                continue;
            }
            let addr = &node.publish_data(env)?.publication.published_at;
            published_ids.push(addr.clone());
            let addr = NumericalAddress::new(
                addr.0.into_bytes(),
                move_compiler::shared::NumberFormat::Hex,
            );
            let pkg_name: Symbol = if names.contains_key(&node.name().as_str()) {
                // return one of the standard aliases
                (*names.get(node.name().as_str()).unwrap()).into()
            } else {
                node.name().as_str().into()
            };

            named_address_map.insert(pkg_name, addr);
        }
    }

    named_address_map.insert(
        root_pkg.package_name().as_str().into(),
        NumericalAddress::new(
            AccountAddress::from_hex_literal("0x0")
                .unwrap()
                .into_bytes(),
            move_compiler::shared::NumberFormat::Hex,
        ),
    );

    debug!("Named address map: {:#?}", named_address_map);

    if let Some(dependency_graph) = &root_pkg.dependencies().get(env) {
        let mut dependencies_paths = vec![];
        let nodes = dependency_graph.nodes();

        // Find the source paths for each dependency and build the PackagePaths
        for node in nodes {
            println!("Building dependency: {}", node.name());
            let sources = get_sources(node.path())?;
            let is_dependency = node.name() != root_pkg.package_name();

            debug!("Node: {:?}, is dependency: {is_dependency}", node.name());

            // TODO: probably here we need to use a different type than Symbol
            let source_package_paths: PackagePaths<Symbol, Symbol> = PackagePaths {
                name: Some((
                    node.name().as_str().into(),
                    PackageConfig {
                        is_dependency,
                        warning_filter: WarningFiltersBuilder::new_for_source(),
                        // TODO: we need to use this probably in the manifest for deserialization
                        flavor: move_compiler::editions::Flavor::Sui,
                        // TODO: we should add this to the type in the manifest.
                        edition: move_compiler::editions::Edition {
                            edition: root_pkg.edition().into(),
                            // TODO: should we have this as a field?
                            release: None,
                        },
                    },
                )),
                named_address_map: named_address_map.clone(),
                paths: sources,
            };

            dependencies_paths.push(source_package_paths);
        }

        debug!("Source package paths: {:#?}", dependencies_paths);

        // Compile the root package and its dependencies
        let compiler =
            move_compiler::Compiler::from_package_paths(None, dependencies_paths, vec![])?;
        let compiler = compiler.add_save_hook(&program_info_hook);

        let (files, units_res) = compiler.build()?;
        let data: (MappedFiles, Vec<_>) = match units_res {
            Ok((units, warning_diags)) => {
                decorate_warnings(warning_diags, Some(&files));
                (files, units)
            }
            Err(error_diags) => {
                // with errors present don't even try decorating warnings output to avoid
                // clutter
                assert!(!error_diags.is_empty());
                let diags_buf =
                    report_diagnostics_to_buffer(&files, error_diags, /* color */ true);
                if let Err(err) = std::io::stderr().write_all(&diags_buf) {
                    anyhow::bail!("Cannot output compiler diagnostics: {}", err);
                }
                anyhow::bail!("Compilation error");
            }
        };

        let root_package_name = root_pkg.package_name().as_str().into();

        let all_compiled_units = data.1;
        let file_map = data.0;
        let mut all_compiled_units_vec = vec![];
        let mut root_compiled_units = vec![];
        let mut deps_compiled_units = vec![];

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
                root_pkg.package_path().path(),
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

        let under_path = root_pkg.package_path().path().join("build");
        let root_package_name: Symbol = root_pkg.package_name().as_str().into();

        save_to_disk(
            root_compiled_units.clone(),
            compiled_package_info.clone(),
            deps_compiled_units.clone(),
            compiled_docs,
            root_package_name,
            under_path,
        );

        let compiled_package = CompiledPackage {
            compiled_package_info,
            root_compiled_units,
            deps_compiled_units,
            compiled_docs: None,
            deps_published_ids: published_ids,
            file_map,
            // compiled_docs,
        };

        Ok(compiled_package)
    } else {
        bail!("Could not compiled package for {env} env")
    }
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
    // self.check_filepaths_ok()?;
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
fn decorate_warnings(warning_diags: Diagnostics, files: Option<&MappedFiles>) {
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

fn source_paths_for_config(package_path: &Path) -> Vec<PathBuf> {
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

    places_to_look
        .into_iter()
        .filter(|path| path.exists())
        .collect()
}

// Find all the source files for a package at the given path
fn get_sources(path: &PackagePath) -> Result<Vec<FileName>> {
    let places_to_look = source_paths_for_config(path.path());
    Ok(find_move_filenames(&places_to_look, false)?
        .into_iter()
        .map(FileName::from)
        .collect())
}
