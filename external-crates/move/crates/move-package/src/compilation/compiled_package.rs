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
use itertools::{Either, Itertools};
use move_binary_format::file_format::CompiledModule;
use move_bytecode_source_map::utils::{
    serialize_to_json, serialize_to_json_string, source_map_from_file,
};
use move_bytecode_utils::Modules;
use move_command_line_common::files::{
    extension_equals, find_filenames, try_exists, FileHash, MOVE_BYTECODE_EXTENSION,
    MOVE_COMPILED_EXTENSION, MOVE_EXTENSION, SOURCE_MAP_EXTENSION,
};
use move_compiler::{
    compiled_unit::{AnnotatedCompiledUnit, CompiledUnit, NamedCompiledModule},
    editions::Flavor,
    linters,
    shared::{
        files::MappedFiles, NamedAddressMap, NumericalAddress, PackageConfig, PackagePaths,
        SaveFlag, SaveHook,
    },
    sui_mode::{self},
    Compiler,
};
use move_disassembler::disassembler::Disassembler;
use move_docgen::{Docgen, DocgenFlags, DocgenOptions};
use move_model_2::source_model;
use move_symbol_pool::Symbol;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::{Path, PathBuf},
};
use vfs::VfsPath;

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
    /// The mapping of file hashes to file names and contents
    pub file_map: MappedFiles,
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

#[derive(Debug, Clone)]
pub enum ModuleFormat {
    Source,
    Bytecode,
}

#[derive(Debug, Clone)]
pub struct DependencyInfo<'a> {
    pub name: Symbol,
    pub is_immediate: bool,
    pub source_paths: Vec<Symbol>,
    pub address_mapping: &'a ResolvedTable,
    pub compiler_config: PackageConfig,
    pub module_format: ModuleFormat,
}

pub(crate) struct BuildResult<T> {
    pub(crate) root_package_name: Symbol,
    pub(crate) immediate_dependencies: Vec<Symbol>,
    pub(crate) result: T,
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
        let mut file_map = MappedFiles::empty();

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

        for unit in root_compiled_units
            .iter()
            .chain(deps_compiled_units.iter().map(|(_, unit)| unit))
        {
            let contents = Arc::from(std::fs::read_to_string(&unit.source_path)?);
            file_map.add(
                FileHash::new(&contents),
                FileName::from(unit.source_path.to_string_lossy().to_string()),
                contents,
            );
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

        Ok(CompiledPackage {
            compiled_package_info: self.package.compiled_package_info.clone(),
            root_compiled_units,
            deps_compiled_units,
            compiled_docs,
            file_map,
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
        let unit = NamedCompiledModule {
            package_name: package_name_opt,
            address: address_bytes,
            name: module_name,
            module,
            source_map,
            address_name: None,
        };
        Ok(CompiledUnitWithSource { unit, source_path })
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
        let category_dir = CompiledPackageLayout::CompiledModules.path();
        let file_path = if root_package == package_name {
            PathBuf::new()
        } else {
            CompiledPackageLayout::Dependencies
                .path()
                .join(package_name.as_str())
        }
        .join(compiled_unit.unit.name.as_str());

        self.save_under(
            category_dir
                .join(&file_path)
                .with_extension(MOVE_COMPILED_EXTENSION),
            compiled_unit.unit.serialize().as_slice(),
        )?;
        self.save_under(
            CompiledPackageLayout::SourceMaps
                .path()
                .join(&file_path)
                .with_extension(SOURCE_MAP_EXTENSION),
            compiled_unit.unit.serialize_source_map().as_slice(),
        )?;
        self.save_under(
            CompiledPackageLayout::SourceMaps
                .path()
                .join(&file_path)
                .with_extension("json"),
            &serialize_to_json(&compiled_unit.unit.source_map)?,
        )?;
        self.save_under(
            CompiledPackageLayout::Sources
                .path()
                .join(&file_path)
                .with_extension(MOVE_EXTENSION),
            std::fs::read_to_string(&compiled_unit.source_path)?.as_bytes(),
        )
    }

    fn save_disassembly_to_disk(
        &self,
        package_name: Symbol,
        unit: &CompiledUnitWithSource,
    ) -> Result<()> {
        let root_package = self.package.compiled_package_info.package_name;
        assert!(self.root_path.ends_with(root_package.as_str()));
        let disassembly_dir = CompiledPackageLayout::Disassembly.path();
        let file_path = if root_package == package_name {
            PathBuf::new()
        } else {
            CompiledPackageLayout::Dependencies
                .path()
                .join(package_name.as_str())
        }
        .join(unit.unit.name.as_str());
        let d = Disassembler::from_unit(&unit.unit);
        let (disassembled_string, bytecode_map) = d.disassemble_with_source_map()?;
        self.save_under(
            disassembly_dir
                .join(&file_path)
                .with_extension(MOVE_BYTECODE_EXTENSION),
            disassembled_string.as_bytes(),
        )?;
        self.save_under(
            disassembly_dir.join(&file_path).with_extension("json"),
            serialize_to_json_string(&bytecode_map)?.as_bytes(),
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
        Modules::new(self.all_compiled_units().map(|unit| &unit.module))
    }

    pub fn root_modules_map(&self) -> Modules {
        Modules::new(
            self.root_compiled_units
                .iter()
                .map(|unit| &unit.unit.module),
        )
    }

    /// `all_compiled_units_with_source` filtered over `CompiledUnit::Module`
    pub fn all_modules(&self) -> impl Iterator<Item = &CompiledUnitWithSource> {
        self.all_compiled_units_with_source()
    }

    /// `root_compiled_units` filtered over `CompiledUnit::Module`
    pub fn root_modules(&self) -> impl Iterator<Item = &CompiledUnitWithSource> {
        self.root_compiled_units.iter()
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

    pub(crate) fn build_for_driver<W: Write, T>(
        w: &mut W,
        vfs_root: Option<VfsPath>,
        resolved_package: Package,
        transitive_dependencies: Vec<DependencyInfo>,
        resolution_graph: &ResolvedGraph,
        compiler_driver: impl FnOnce(Compiler) -> Result<T>,
    ) -> Result<BuildResult<T>> {
        let immediate_dependencies = transitive_dependencies
            .iter()
            .filter(|&dep| dep.is_immediate)
            .map(|dep| dep.name)
            .collect::<Vec<_>>();
        for dep in &transitive_dependencies {
            writeln!(w, "{} {}", "INCLUDING DEPENDENCY".bold().green(), dep.name)?;
        }
        let root_package_name = resolved_package.source_package.package.name;
        writeln!(w, "{} {}", "BUILDING".bold().green(), root_package_name)?;

        // gather source/dep files with their address mappings
        let (sources_package_paths, deps_package_paths) = make_source_and_deps_for_compiler(
            resolution_graph,
            &resolved_package,
            transitive_dependencies,
        )?;
        let flags = resolution_graph.build_options.compiler_flags();
        // Partition deps_package according whether src is available
        let (src_deps, bytecode_deps): (Vec<_>, Vec<_>) = deps_package_paths
            .clone()
            .into_iter()
            .partition_map(|(p, b)| match b {
                ModuleFormat::Source => Either::Left(p),
                ModuleFormat::Bytecode => Either::Right(p),
            });
        // If bytecode dependency is not empty, do not allow renaming
        if !bytecode_deps.is_empty() {
            if let Some(pkg_name) = resolution_graph.contains_renaming() {
                anyhow::bail!(
                    "Found address renaming in package '{}' when \
                    building with bytecode dependencies -- this is currently not supported",
                    pkg_name
                )
            }
        }

        // invoke the compiler
        let mut paths = src_deps;
        paths.push(sources_package_paths.clone());

        let lint_level = resolution_graph.build_options.lint_flag.get();
        let sui_mode = resolution_graph
            .build_options
            .default_flavor
            .map_or(false, |f| f == Flavor::Sui);

        let mut compiler = Compiler::from_package_paths(vfs_root, paths, bytecode_deps)
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
        Ok(BuildResult {
            root_package_name,
            immediate_dependencies,
            result: compiler_driver(compiler)?,
        })
    }

    pub(crate) fn build_for_result<W: Write, T>(
        w: &mut W,
        vfs_root: Option<VfsPath>,
        resolved_package: Package,
        transitive_dependencies: Vec<DependencyInfo>,
        resolution_graph: &ResolvedGraph,
        compiler_driver: impl FnMut(Compiler) -> Result<T>,
    ) -> Result<T> {
        let build_result = Self::build_for_driver(
            w,
            vfs_root,
            resolved_package,
            transitive_dependencies,
            resolution_graph,
            compiler_driver,
        )?;
        Ok(build_result.result)
    }

    pub(crate) fn build_all<W: Write>(
        w: &mut W,
        vfs_root: Option<VfsPath>,
        project_root: &Path,
        resolved_package: Package,
        transitive_dependencies: Vec<DependencyInfo>,
        resolution_graph: &ResolvedGraph,
        compiler_driver: impl FnOnce(Compiler) -> Result<(MappedFiles, Vec<AnnotatedCompiledUnit>)>,
    ) -> Result<CompiledPackage> {
        let program_info_hook = SaveHook::new([SaveFlag::TypingInfo]);
        let BuildResult {
            root_package_name,
            immediate_dependencies,
            result,
        } = Self::build_for_driver(
            w,
            vfs_root,
            resolved_package.clone(),
            transitive_dependencies,
            resolution_graph,
            |compiler| {
                let compiler = compiler.add_save_hook(&program_info_hook);
                compiler_driver(compiler)
            },
        )?;
        let program_info = program_info_hook.take_typing_info();
        let (file_map, all_compiled_units) = result;
        let mut all_compiled_units_vec = vec![];
        let mut root_compiled_units = vec![];
        let mut deps_compiled_units = vec![];
        for annot_unit in all_compiled_units {
            let source_path = PathBuf::from(
                file_map
                    .get(&annot_unit.loc().file_hash())
                    .unwrap()
                    .0
                    .as_str(),
            );
            let package_name = annot_unit.named_module.package_name.unwrap();
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
        if resolution_graph.build_options.generate_docs {
            let root_named_address_map = resolved_package.resolved_table.clone();
            let model = source_model::Model::new(
                file_map.clone(),
                Some(root_package_name),
                root_named_address_map,
                program_info,
                all_compiled_units_vec,
            )?;

            compiled_docs = Some(Self::build_docs(
                DocgenFlags::default(), // TODO this should be configurable
                resolved_package.source_package.package.name,
                &model,
                &resolved_package.package_path,
                &immediate_dependencies,
                &resolution_graph.build_options.install_dir,
            )?);
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
            file_map,
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
            if self.compiled_package_info.build_flags.save_disassembly {
                on_disk_package.save_disassembly_to_disk(root_package, compiled_unit)?;
            }
        }
        for (dep_name, compiled_unit) in &self.deps_compiled_units {
            on_disk_package.save_compiled_unit(*dep_name, compiled_unit)?;
            if self.compiled_package_info.build_flags.save_disassembly {
                on_disk_package.save_disassembly_to_disk(*dep_name, compiled_unit)?;
            }
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

        on_disk_package.save_under(
            CompiledPackageLayout::BuildInfo.path(),
            serde_yaml::to_string(&on_disk_package.package)?.as_bytes(),
        )?;

        Ok(on_disk_package)
    }

    fn build_docs(
        docgen_flags: DocgenFlags,
        package_name: PackageName,
        model: &source_model::Model,
        package_root: &Path,
        deps: &[PackageName],
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
        docgen.gen(model)
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
    deps: Vec<DependencyInfo>,
) -> Result<(
    /* sources */ PackagePaths,
    /* deps */ Vec<(PackagePaths, ModuleFormat)>,
)> {
    let deps_package_paths = make_deps_for_compiler_internal(deps)?;
    let root_named_addrs = apply_named_address_renaming(
        root.source_package.package.name,
        named_address_mapping_for_compiler(&root.resolved_table),
        &root.renaming,
    );
    let sources = root.get_sources(&resolution_graph.build_options)?;
    let source_package_paths = PackagePaths {
        name: Some((
            root.source_package.package.name,
            root.compiler_config(
                /* is_dependency */ false,
                &resolution_graph.build_options,
            ),
        )),
        paths: sources,
        named_address_map: root_named_addrs,
    };
    Ok((source_package_paths, deps_package_paths))
}

pub(crate) fn make_deps_for_compiler_internal(
    deps: Vec<DependencyInfo>,
) -> Result<Vec<(PackagePaths, ModuleFormat)>> {
    deps.into_iter()
        .map(|dep| {
            let paths = dep
                .source_paths
                .into_iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let named_address_map = named_address_mapping_for_compiler(dep.address_mapping);
            Ok((
                PackagePaths {
                    name: Some((dep.name, dep.compiler_config)),
                    paths,
                    named_address_map,
                },
                dep.module_format,
            ))
        })
        .collect::<Result<Vec<_>>>()
}
