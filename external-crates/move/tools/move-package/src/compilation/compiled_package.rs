// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    compilation::package_layout::CompiledPackageLayout,
    resolution::resolution_graph::{Package, Renaming, ResolvedGraph, ResolvedTable},
    source_package::{
        layout::{SourcePackageLayout, REFERENCE_TEMPLATE_FILENAME},
        parsed_manifest::{FileName, PackageDigest, PackageName},
    },
    BuildConfig,
};
use anyhow::{ensure, Result};
use colored::Colorize;
use move_abigen::{Abigen, AbigenOptions};
use move_binary_format::file_format::{CompiledModule, CompiledScript};
use move_bytecode_source_map::utils::source_map_from_file;
use move_bytecode_utils::Modules;
use move_command_line_common::{
    env::get_bytecode_version_from_env,
    files::{
        extension_equals, find_filenames, try_exists, MOVE_COMPILED_EXTENSION, MOVE_EXTENSION,
        SOURCE_MAP_EXTENSION,
    },
};
use move_compiler::{
    compiled_unit::{
        self, AnnotatedCompiledUnit, CompiledUnit, NamedCompiledModule, NamedCompiledScript,
    },
    diagnostics::FilesSourceText,
    shared::{Flags, NamedAddressMap, NumericalAddress, PackagePaths},
    Compiler,
};
use move_docgen::{Docgen, DocgenOptions};
use move_model::{model::GlobalEnv, options::ModelBuilderOptions, run_model_builder_with_options};
use move_symbol_pool::Symbol;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub enum CompilationCachingStatus {
    /// The package and all if its dependencies were cached
    Cached,
    /// At least this package and/or one of its dependencies needed to be rebuilt
    Recompiled,
}

#[derive(Debug, Clone)]
pub struct CompiledUnitWithSource {
    pub unit: CompiledUnit,
    pub source_path: PathBuf,
}

/// Represents meta information about a package and the information it was compiled with. Shared
/// across both the `CompiledPackage` and `OnDiskCompiledPackage` structs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledPackageInfo {
    /// The name of the compiled package
    pub package_name: PackageName,
    /// The instantiations for all named addresses that were used for compilation
    pub address_alias_instantiation: ResolvedTable,
    /// The hash of the source directory at the time of compilation. `None` if the source for this
    /// package is not available/this package was not compiled.
    pub source_digest: Option<PackageDigest>,
    /// The build flags that were used when compiling this package.
    pub build_flags: BuildConfig,
}

/// Represents a compiled package in memory.
#[derive(Debug, Clone)]
pub struct CompiledPackage {
    /// Meta information about the compilation of this `CompiledPackage`
    pub compiled_package_info: CompiledPackageInfo,
    /// The output compiled bytecode in the root package (both module, and scripts) along with its
    /// source file
    pub root_compiled_units: Vec<CompiledUnitWithSource>,
    /// The output compiled bytecode for dependencies
    pub deps_compiled_units: Vec<(PackageName, CompiledUnitWithSource)>,

    // Optional artifacts from compilation
    //
    /// filename -> doctext
    pub compiled_docs: Option<Vec<(String, String)>>,
    /// filename -> json bytes for ScriptABI. Can then be used to generate transaction builders in
    /// various languages.
    pub compiled_abis: Option<Vec<(String, Vec<u8>)>>,
}

/// Represents a compiled package that has been saved to disk. This holds only the minimal metadata
/// needed to reconstruct a `CompiledPackage` package from it and to determine whether or not a
/// recompilation of the package needs to be performed or not.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnDiskPackage {
    /// Information about the package and the specific compilation that was done.
    pub compiled_package_info: CompiledPackageInfo,
    /// Dependency names for this package.
    pub dependencies: Vec<PackageName>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnDiskCompiledPackage {
    /// Path to the root of the package and its data on disk. Relative to/rooted at the directory
    /// containing the `Move.toml` file for this package.
    pub root_path: PathBuf,
    pub package: OnDiskPackage,
}

impl CompilationCachingStatus {
    /// Returns `true` if this package and all dependencies are cached
    pub fn is_cached(&self) -> bool {
        matches!(self, Self::Cached)
    }

    /// Returns `true` if this package or one of its dependencies was rebuilt
    pub fn is_rebuilt(&self) -> bool {
        !self.is_cached()
    }
}

impl OnDiskCompiledPackage {
    pub fn from_path(p: &Path) -> Result<Self> {
        let (buf, build_path) = if try_exists(p)? && extension_equals(p, "yaml") {
            (std::fs::read(p)?, p.parent().unwrap().parent().unwrap())
        } else {
            (
                std::fs::read(p.join(CompiledPackageLayout::BuildInfo.path()))?,
                p.parent().unwrap(),
            )
        };
        let package = serde_yaml::from_slice::<OnDiskPackage>(&buf)?;
        assert!(build_path.ends_with(CompiledPackageLayout::Root.path()));
        let root_path = build_path.join(package.compiled_package_info.package_name.as_str());
        Ok(Self { root_path, package })
    }

    pub fn into_compiled_package(&self) -> Result<CompiledPackage> {
        let root_name = self.package.compiled_package_info.package_name;
        assert!(self.root_path.ends_with(root_name.as_str()));
        let root_compiled_units = self.get_compiled_units_paths(root_name)?;
        let root_compiled_units = root_compiled_units
            .into_iter()
            .map(|bytecode_path| self.decode_unit(root_name, &bytecode_path))
            .collect::<Result<Vec<_>>>()?;
        let mut deps_compiled_units = vec![];
        for dep_name in self.package.dependencies.iter().copied() {
            let compiled_units = self.get_compiled_units_paths(dep_name)?;
            for bytecode_path in compiled_units {
                deps_compiled_units.push((dep_name, self.decode_unit(dep_name, &bytecode_path)?))
            }
        }

        let docs_path = self
            .root_path
            .join(self.package.compiled_package_info.package_name.as_str())
            .join(CompiledPackageLayout::CompiledDocs.path());
        let compiled_docs = if docs_path.is_dir() {
            Some(
                find_filenames(&[docs_path.to_string_lossy().to_string()], |path| {
                    extension_equals(path, "md")
                })?
                .into_iter()
                .map(|path| {
                    let contents = std::fs::read_to_string(&path).unwrap();
                    (path, contents)
                })
                .collect(),
            )
        } else {
            None
        };

        let abi_path = self
            .root_path
            .join(self.package.compiled_package_info.package_name.as_str())
            .join(CompiledPackageLayout::CompiledABIs.path());
        let compiled_abis = if abi_path.is_dir() {
            Some(
                find_filenames(&[abi_path.to_string_lossy().to_string()], |path| {
                    extension_equals(path, "abi")
                })?
                .into_iter()
                .map(|path| {
                    let contents = std::fs::read(&path).unwrap();
                    (path, contents)
                })
                .collect(),
            )
        } else {
            None
        };

        Ok(CompiledPackage {
            compiled_package_info: self.package.compiled_package_info.clone(),
            root_compiled_units,
            deps_compiled_units,
            compiled_docs,
            compiled_abis,
        })
    }

    fn decode_unit(
        &self,
        package_name: Symbol,
        bytecode_path_str: &str,
    ) -> Result<CompiledUnitWithSource> {
        let package_name_opt = Some(package_name);
        let bytecode_path = Path::new(bytecode_path_str);
        let path_to_file = CompiledPackageLayout::path_to_file_after_category(bytecode_path);
        let bytecode_bytes = std::fs::read(bytecode_path)?;
        let source_map = source_map_from_file(
            &self
                .root_path
                .join(CompiledPackageLayout::SourceMaps.path())
                .join(&path_to_file)
                .with_extension(SOURCE_MAP_EXTENSION),
        )?;
        let source_path = self
            .root_path
            .join(CompiledPackageLayout::Sources.path())
            .join(path_to_file)
            .with_extension(MOVE_EXTENSION);
        ensure!(
            source_path.is_file(),
            "Error decoding package: {}. \
            Unable to find corresponding source file for '{}' in package {}",
            self.package.compiled_package_info.package_name,
            bytecode_path_str,
            package_name
        );
        match CompiledScript::deserialize(&bytecode_bytes) {
            Ok(script) => {
                let name = FileName::from(
                    bytecode_path
                        .file_stem()
                        .unwrap()
                        .to_string_lossy()
                        .to_string(),
                );
                let unit = CompiledUnit::Script(NamedCompiledScript {
                    package_name: package_name_opt,
                    name,
                    script,
                    source_map,
                });
                Ok(CompiledUnitWithSource { unit, source_path })
            }
            Err(_) => {
                let module = CompiledModule::deserialize_with_defaults(&bytecode_bytes)?;
                let (address_bytes, module_name) = {
                    let id = module.self_id();
                    let parsed_addr = NumericalAddress::new(
                        id.address().into_bytes(),
                        move_compiler::shared::NumberFormat::Hex,
                    );
                    let module_name = FileName::from(id.name().as_str());
                    (parsed_addr, module_name)
                };
                let unit = CompiledUnit::Module(NamedCompiledModule {
                    package_name: package_name_opt,
                    address: address_bytes,
                    name: module_name,
                    module,
                    source_map,
                });
                Ok(CompiledUnitWithSource { unit, source_path })
            }
        }
    }

    /// Save `bytes` under `path_under` relative to the package on disk
    pub(crate) fn save_under(&self, file: impl AsRef<Path>, bytes: &[u8]) -> Result<()> {
        let path_to_save = self.root_path.join(file);
        let parent = path_to_save.parent().unwrap();
        std::fs::create_dir_all(parent)?;
        std::fs::write(path_to_save, bytes).map_err(|err| err.into())
    }

    #[allow(unused)]
    pub(crate) fn has_source_changed_since_last_compile(&self, resolved_package: &Package) -> bool {
        match &self.package.compiled_package_info.source_digest {
            // Don't have source available to us
            None => false,
            Some(digest) => digest != &resolved_package.source_digest,
        }
    }

    #[allow(unused)]
    pub(crate) fn are_build_flags_different(&self, build_config: &BuildConfig) -> bool {
        build_config != &self.package.compiled_package_info.build_flags
    }

    fn get_compiled_units_paths(&self, package_name: Symbol) -> Result<Vec<String>> {
        let package_dir = if self.package.compiled_package_info.package_name == package_name {
            self.root_path.clone()
        } else {
            self.root_path
                .join(CompiledPackageLayout::Dependencies.path())
                .join(package_name.as_str())
        };
        let mut compiled_unit_paths = vec![];
        let module_path = package_dir.join(CompiledPackageLayout::CompiledModules.path());
        if try_exists(&module_path)? {
            compiled_unit_paths.push(module_path);
        }
        let script_path = package_dir.join(CompiledPackageLayout::CompiledScripts.path());
        if try_exists(&script_path)? {
            compiled_unit_paths.push(script_path);
        }
        find_filenames(&compiled_unit_paths, |path| {
            extension_equals(path, MOVE_COMPILED_EXTENSION)
        })
    }

    fn save_compiled_unit(
        &self,
        package_name: Symbol,
        compiled_unit: &CompiledUnitWithSource,
    ) -> Result<()> {
        let root_package = self.package.compiled_package_info.package_name;
        assert!(self.root_path.ends_with(root_package.as_str()));
        let category_dir = match &compiled_unit.unit {
            CompiledUnit::Script(_) => CompiledPackageLayout::CompiledScripts.path(),
            CompiledUnit::Module(_) => CompiledPackageLayout::CompiledModules.path(),
        };
        let file_path = if root_package == package_name {
            PathBuf::new()
        } else {
            CompiledPackageLayout::Dependencies
                .path()
                .join(package_name.as_str())
        }
        .join(match &compiled_unit.unit {
            CompiledUnit::Script(named) => named.name.as_str(),
            CompiledUnit::Module(named) => named.name.as_str(),
        });

        self.save_under(
            category_dir
                .join(&file_path)
                .with_extension(MOVE_COMPILED_EXTENSION),
            compiled_unit
                .unit
                .serialize(get_bytecode_version_from_env())
                .as_slice(),
        )?;
        self.save_under(
            CompiledPackageLayout::SourceMaps
                .path()
                .join(&file_path)
                .with_extension(SOURCE_MAP_EXTENSION),
            compiled_unit.unit.serialize_source_map().as_slice(),
        )?;
        self.save_under(
            CompiledPackageLayout::Sources
                .path()
                .join(&file_path)
                .with_extension(MOVE_EXTENSION),
            std::fs::read_to_string(&compiled_unit.source_path)?.as_bytes(),
        )
    }
}

impl CompiledPackage {
    /// Returns all compiled units with sources for this package in transitive dependencies. Order
    /// is not guaranteed.
    pub fn all_compiled_units_with_source(&self) -> impl Iterator<Item = &CompiledUnitWithSource> {
        self.root_compiled_units
            .iter()
            .chain(self.deps_compiled_units.iter().map(|(_, unit)| unit))
    }

    /// Returns all compiled units for this package in transitive dependencies. Order is not
    /// guaranteed.
    pub fn all_compiled_units(&self) -> impl Iterator<Item = &CompiledUnit> {
        self.all_compiled_units_with_source().map(|unit| &unit.unit)
    }

    /// Returns compiled modules for this package and its transitive dependencies
    pub fn all_modules_map(&self) -> Modules {
        Modules::new(
            self.all_compiled_units()
                .filter_map(|unit| match unit {
                    CompiledUnit::Module(NamedCompiledModule { module, .. }) => Some(module),
                    CompiledUnit::Script(_) => None,
                })
                .collect::<Vec<_>>(),
        )
    }

    pub fn root_modules_map(&self) -> Modules {
        Modules::new(
            self.root_compiled_units
                .iter()
                .filter_map(|unit| match &unit.unit {
                    CompiledUnit::Module(NamedCompiledModule { module, .. }) => Some(module),
                    CompiledUnit::Script(_) => None,
                }),
        )
    }

    /// `all_compiled_units_with_source` filtered over `CompiledUnit::Module`
    pub fn all_modules(&self) -> impl Iterator<Item = &CompiledUnitWithSource> {
        self.all_compiled_units_with_source()
            .filter(|unit| matches!(unit.unit, CompiledUnit::Module(_)))
    }

    /// `root_compiled_units` filtered over `CompiledUnit::Module`
    pub fn root_modules(&self) -> impl Iterator<Item = &CompiledUnitWithSource> {
        self.root_compiled_units
            .iter()
            .filter(|unit| matches!(unit.unit, CompiledUnit::Module(_)))
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
            .filter(|(dep_package, unit)| {
                dep_package.as_str() == package_name && matches!(unit.unit, CompiledUnit::Module(_))
            })
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

    pub fn get_script_by_name(
        &self,
        package_name: &str,
        script_name: &str,
    ) -> Result<&CompiledUnitWithSource> {
        if self.compiled_package_info.package_name.as_str() == package_name {
            return self.get_script_by_name_from_root(script_name);
        }

        self.deps_compiled_units
            .iter()
            .filter(|(dep_package, unit)| {
                dep_package.as_str() == package_name && matches!(unit.unit, CompiledUnit::Script(_))
            })
            .map(|(_, unit)| unit)
            .find(|unit| unit.unit.name().as_str() == script_name)
            .ok_or_else(|| {
                anyhow::format_err!(
                    "Unable to find script with name '{}' in package {}",
                    script_name,
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

    pub fn get_script_by_name_from_root(
        &self,
        script_name: &str,
    ) -> Result<&CompiledUnitWithSource> {
        self.scripts()
            .find(|unit| unit.unit.name().as_str() == script_name)
            .ok_or_else(|| {
                anyhow::format_err!(
                    "Unable to find script with name '{}' in package {}",
                    script_name,
                    self.compiled_package_info.package_name
                )
            })
    }

    pub fn scripts(&self) -> impl Iterator<Item = &CompiledUnitWithSource> {
        self.root_compiled_units
            .iter()
            .filter(|unit| matches!(unit.unit, CompiledUnit::Script(_)))
    }

    #[allow(unused)]
    fn can_load_cached(
        package: &OnDiskCompiledPackage,
        resolution_graph: &ResolvedGraph,
        resolved_package: &Package,
        is_root_package: bool,
    ) -> bool {
        // TODO: add more tests for the different caching cases
        !(package.has_source_changed_since_last_compile(resolved_package) // recompile if source has changed
            // Recompile if the flags are different
                || package.are_build_flags_different(&resolution_graph.build_options)
                // Force root package recompilation in test mode
                || resolution_graph.build_options.test_mode && is_root_package
                // Recompile if force recompilation is set
                || resolution_graph.build_options.force_recompilation) &&
                // Dive deeper to make sure that instantiations haven't changed since that
                // can be changed by other packages above us in the dependency graph possibly
                package.package.compiled_package_info.address_alias_instantiation
                    == resolved_package.resolved_table
    }

    pub(crate) fn build_all<W: Write>(
        w: &mut W,
        project_root: &Path,
        resolved_package: Package,
        transitive_dependencies: Vec<(
            /* name */ Symbol,
            /* is immediate */ bool,
            /* source paths */ Vec<Symbol>,
            /* address mapping */ &ResolvedTable,
        )>,
        resolution_graph: &ResolvedGraph,
        mut compiler_driver: impl FnMut(
            Compiler,
        )
            -> Result<(FilesSourceText, Vec<AnnotatedCompiledUnit>)>,
    ) -> Result<CompiledPackage> {
        let immediate_dependencies = transitive_dependencies
            .iter()
            .filter(|(_, is_immediate, _, _)| *is_immediate)
            .map(|(name, _, _, _)| *name)
            .collect::<Vec<_>>();
        let transitive_dependencies = transitive_dependencies
            .into_iter()
            .map(|(name, _is_immediate, source_paths, address_mapping)| {
                (name, source_paths, address_mapping)
            })
            .collect::<Vec<_>>();
        for (dep_package_name, _, _) in &transitive_dependencies {
            writeln!(
                w,
                "{} {}",
                "INCLUDING DEPENDENCY".bold().green(),
                dep_package_name
            )?;
        }
        let root_package_name = resolved_package.source_package.package.name;
        writeln!(w, "{} {}", "BUILDING".bold().green(), root_package_name)?;

        // gather source/dep files with their address mappings
        let (sources_package_paths, deps_package_paths) = make_source_and_deps_for_compiler(
            resolution_graph,
            &resolved_package,
            transitive_dependencies,
        )?;
        let flags = if resolution_graph.build_options.test_mode {
            Flags::testing()
        } else {
            Flags::empty()
        };
        // invoke the compiler
        let mut paths = deps_package_paths.clone();
        paths.push(sources_package_paths.clone());

        let compiler = Compiler::from_package_paths(paths, vec![]).set_flags(flags);
        let (file_map, all_compiled_units) = compiler_driver(compiler)?;
        let mut root_compiled_units = vec![];
        let mut deps_compiled_units = vec![];
        for annot_unit in all_compiled_units {
            let source_path = PathBuf::from(file_map[&annot_unit.loc().file_hash()].0.as_str());
            let package_name = match &annot_unit {
                compiled_unit::CompiledUnitEnum::Module(m) => m.named_module.package_name.unwrap(),
                compiled_unit::CompiledUnitEnum::Script(s) => s.named_script.package_name.unwrap(),
            };
            let unit = CompiledUnitWithSource {
                unit: annot_unit.into_compiled_unit(),
                source_path,
            };
            if package_name == root_package_name {
                root_compiled_units.push(unit)
            } else {
                deps_compiled_units.push((package_name, unit))
            }
        }

        let mut compiled_docs = None;
        let mut compiled_abis = None;
        if resolution_graph.build_options.generate_docs
            || resolution_graph.build_options.generate_abis
        {
            let model = run_model_builder_with_options(
                vec![sources_package_paths],
                deps_package_paths,
                ModelBuilderOptions::default(),
            )?;

            if resolution_graph.build_options.generate_docs {
                compiled_docs = Some(Self::build_docs(
                    resolved_package.source_package.package.name,
                    &model,
                    &resolved_package.package_path,
                    &immediate_dependencies,
                    &resolution_graph.build_options.install_dir,
                ));
            }

            if resolution_graph.build_options.generate_abis {
                compiled_abis = Some(Self::build_abis(
                    get_bytecode_version_from_env(),
                    &model,
                    &root_compiled_units,
                ));
            }
        };

        let compiled_package = CompiledPackage {
            compiled_package_info: CompiledPackageInfo {
                package_name: resolved_package.source_package.package.name,
                address_alias_instantiation: resolved_package.resolved_table,
                source_digest: Some(resolved_package.source_digest),
                build_flags: resolution_graph.build_options.clone(),
            },
            root_compiled_units,
            deps_compiled_units,
            compiled_docs,
            compiled_abis,
        };

        compiled_package.save_to_disk(project_root.join(CompiledPackageLayout::Root.path()))?;

        Ok(compiled_package)
    }

    // We take the (restrictive) view that all filesystems are case insensitive to maximize
    // portability of packages.
    fn check_filepaths_ok(&self) -> Result<()> {
        // A mapping of (lowercase_name => [info_for_each_occurence]
        let mut insensitive_mapping = BTreeMap::new();
        for compiled_unit in &self.root_compiled_units {
            let is_module = matches!(&compiled_unit.unit, CompiledUnit::Module(_));
            let name = match &compiled_unit.unit {
                CompiledUnit::Script(named) => named.name.as_str(),
                CompiledUnit::Module(named) => named.name.as_str(),
            };
            let entry = insensitive_mapping
                .entry(name.to_lowercase())
                .or_insert_with(Vec::new);
            entry.push((
                name,
                is_module,
                compiled_unit.source_path.to_string_lossy().to_string(),
            ));
        }
        let errs = insensitive_mapping
            .into_iter()
            .filter_map(|(insensitive_name, occurence_infos)| {
                if occurence_infos.len() > 1 {
                    let name_conflict_error_msg = occurence_infos
                        .into_iter()
                        .map(|(name, is_module, fpath)| {
                                format!(
                                    "\t{} '{}' at path '{}'",
                                    if is_module { "Module" } else { "Script" },
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
            anyhow::bail!("Module and/or script names found that would cause failures on case insensitive \
                file systems when compiling package '{}':\n{}\nPlease rename these scripts and/or modules to resolve these conflicts.",
                self.compiled_package_info.package_name,
                errs.join("\n"),
            )
        }
        Ok(())
    }

    pub(crate) fn save_to_disk(&self, under_path: PathBuf) -> Result<OnDiskCompiledPackage> {
        self.check_filepaths_ok()?;
        assert!(under_path.ends_with(CompiledPackageLayout::Root.path()));
        let root_package = self.compiled_package_info.package_name;
        let on_disk_package = OnDiskCompiledPackage {
            root_path: under_path.join(root_package.as_str()),
            package: OnDiskPackage {
                compiled_package_info: self.compiled_package_info.clone(),
                dependencies: self
                    .deps_compiled_units
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

        for compiled_unit in &self.root_compiled_units {
            on_disk_package.save_compiled_unit(root_package, compiled_unit)?;
        }
        for (dep_name, compiled_unit) in &self.deps_compiled_units {
            on_disk_package.save_compiled_unit(*dep_name, compiled_unit)?;
        }

        if let Some(docs) = &self.compiled_docs {
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

        if let Some(abis) = &self.compiled_abis {
            for (filename, abi_bytes) in abis {
                on_disk_package.save_under(
                    CompiledPackageLayout::CompiledABIs
                        .path()
                        .join(filename)
                        .with_extension("abi"),
                    abi_bytes,
                )?;
            }
        }

        on_disk_package.save_under(
            CompiledPackageLayout::BuildInfo.path(),
            serde_yaml::to_string(&on_disk_package.package)?.as_bytes(),
        )?;

        Ok(on_disk_package)
    }

    fn build_abis(
        bytecode_version: Option<u32>,
        model: &GlobalEnv,
        compiled_units: &[CompiledUnitWithSource],
    ) -> Vec<(String, Vec<u8>)> {
        let bytecode_map: BTreeMap<_, _> = compiled_units
            .iter()
            .map(|unit| match &unit.unit {
                CompiledUnit::Script(script) => (
                    script.name.to_string(),
                    unit.unit.serialize(bytecode_version),
                ),
                CompiledUnit::Module(module) => (
                    module.name.to_string(),
                    unit.unit.serialize(bytecode_version),
                ),
            })
            .collect();
        let abi_options = AbigenOptions {
            in_memory_bytes: Some(bytecode_map),
            output_directory: "".to_string(),
            ..AbigenOptions::default()
        };
        let mut abigen = Abigen::new(model, &abi_options);
        abigen.gen();
        abigen.into_result()
    }

    fn build_docs(
        package_name: PackageName,
        model: &GlobalEnv,
        package_root: &Path,
        deps: &[PackageName],
        install_dir: &Option<PathBuf>,
    ) -> Vec<(String, String)> {
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
                    .join(dep_name.as_str())
                    .join(CompiledPackageLayout::CompiledDocs.path())
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        let in_pkg_doc_path = root_for_docs
            .join(package_name.as_str())
            .join(CompiledPackageLayout::CompiledDocs.path());
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
            ..DocgenOptions::default()
        };
        let docgen = Docgen::new(model, &doc_options);
        docgen.gen()
    }
}

pub(crate) fn named_address_mapping_for_compiler(
    resolution_table: &ResolvedTable,
) -> BTreeMap<Symbol, NumericalAddress> {
    resolution_table
        .iter()
        .map(|(ident, addr)| {
            let parsed_addr =
                NumericalAddress::new(addr.into_bytes(), move_compiler::shared::NumberFormat::Hex);
            (*ident, parsed_addr)
        })
        .collect::<BTreeMap<_, _>>()
}

pub(crate) fn apply_named_address_renaming(
    current_package_name: Symbol,
    address_resolution: BTreeMap<Symbol, NumericalAddress>,
    renaming: &Renaming,
) -> NamedAddressMap {
    let package_renamings = renaming
        .iter()
        .filter_map(|(rename_to, (package_name, from_name))| {
            if package_name == &current_package_name {
                Some((from_name, *rename_to))
            } else {
                None
            }
        })
        .collect::<BTreeMap<_, _>>();

    address_resolution
        .into_iter()
        .map(|(name, value)| {
            let new_name = package_renamings.get(&name).copied();
            (new_name.unwrap_or(name), value)
        })
        .collect()
}

pub(crate) fn make_source_and_deps_for_compiler(
    resolution_graph: &ResolvedGraph,
    root: &Package,
    deps: Vec<(
        /* name */ Symbol,
        /* source paths */ Vec<Symbol>,
        /* address mapping */ &ResolvedTable,
    )>,
) -> Result<(
    /* sources */ PackagePaths,
    /* deps */ Vec<PackagePaths>,
)> {
    let deps_package_paths = deps
        .into_iter()
        .map(|(name, source_paths, resolved_table)| {
            let paths = source_paths
                .into_iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let named_address_map = named_address_mapping_for_compiler(resolved_table);
            Ok(PackagePaths {
                name: Some(name),
                paths,
                named_address_map,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let root_named_addrs = apply_named_address_renaming(
        root.source_package.package.name,
        named_address_mapping_for_compiler(&root.resolved_table),
        &root.renaming,
    );
    let sources = root.get_sources(&resolution_graph.build_options)?;
    let source_package_paths = PackagePaths {
        name: Some(root.source_package.package.name),
        paths: sources,
        named_address_map: root_named_addrs,
    };
    Ok((source_package_paths, deps_package_paths))
}
