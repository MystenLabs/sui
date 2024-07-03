// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod package_lock;

pub mod compilation;
pub mod lock_file;
pub mod migration;
pub mod package_hooks;
pub mod resolution;
pub mod source_package;

use anyhow::{anyhow, Result};
use clap::*;
use lock_file::LockFile;
use move_compiler::{
    editions::{Edition, Flavor},
    Flags,
};
use move_core_types::account_address::AccountAddress;
use move_model::model::GlobalEnv;
use resolution::{dependency_graph::DependencyGraphBuilder, resolution_graph::ResolvedGraph};
use serde::{Deserialize, Serialize};
use source_package::{
    layout::SourcePackageLayout,
    manifest_parser::{parse_move_manifest_string, parse_source_manifest},
    parsed_manifest::DependencyKind,
};
use std::{
    collections::BTreeMap,
    io::{BufRead, Write},
    path::{Path, PathBuf},
};

use crate::{
    compilation::{
        build_plan::BuildPlan, compiled_package::CompiledPackage, model_builder::ModelBuilder,
    },
    lock_file::schema::update_compiler_toolchain,
    package_lock::PackageLock,
};
use move_compiler::linters::LintLevel;

#[derive(Debug, Parser, Clone, Serialize, Deserialize, Eq, PartialEq, PartialOrd, Default)]
#[clap(about)]
pub struct BuildConfig {
    /// Compile in 'dev' mode. The 'dev-addresses' and 'dev-dependencies' fields will be used if
    /// this flag is set. This flag is useful for development of packages that expose named
    /// addresses that are not set to a specific value.
    #[clap(name = "dev-mode", short = 'd', long = "dev", global = true)]
    pub dev_mode: bool,

    /// Compile in 'test' mode. The 'dev-addresses' and 'dev-dependencies' fields will be used
    /// along with any code in the 'tests' directory.
    #[clap(name = "test-mode", long = "test", global = true)]
    pub test_mode: bool,

    /// Generate documentation for packages
    #[clap(name = "generate-docs", long = "doc", global = true)]
    pub generate_docs: bool,

    /// Installation directory for compiled artifacts. Defaults to current directory.
    #[clap(long = "install-dir", global = true)]
    pub install_dir: Option<PathBuf>,

    /// Force recompilation of all packages
    #[clap(name = "force-recompilation", long = "force", global = true)]
    pub force_recompilation: bool,

    /// Optional location to save the lock file to, if package resolution succeeds.
    #[clap(skip)]
    pub lock_file: Option<PathBuf>,

    /// Only fetch dependency repos to MOVE_HOME
    #[clap(long = "fetch-deps-only", global = true)]
    pub fetch_deps_only: bool,

    /// Skip fetching latest git dependencies
    #[clap(long = "skip-fetch-latest-git-deps", global = true)]
    pub skip_fetch_latest_git_deps: bool,

    /// Default flavor for move compilation, if not specified in the package's config
    #[clap(long = "default-move-flavor", global = true)]
    pub default_flavor: Option<Flavor>,

    /// Default edition for move compilation, if not specified in the package's config
    #[clap(long = "default-move-edition", global = true)]
    pub default_edition: Option<Edition>,

    /// If set, dependency packages are treated as root packages. Notably, this will remove
    /// warning suppression in dependency packages.
    #[clap(long = "dependencies-are-root", global = true)]
    pub deps_as_root: bool,

    /// If set, ignore any compiler warnings
    #[clap(long = move_compiler::command_line::SILENCE_WARNINGS, global = true)]
    pub silence_warnings: bool,

    /// If set, warnings become errors
    #[clap(long = move_compiler::command_line::WARNINGS_ARE_ERRORS, global = true)]
    pub warnings_are_errors: bool,

    /// If set, reports errors at JSON
    #[clap(long = move_compiler::command_line::JSON_ERRORS, global = true)]
    pub json_errors: bool,

    /// Additional named address mapping. Useful for tools in rust
    #[clap(skip)]
    pub additional_named_addresses: BTreeMap<String, AccountAddress>,

    #[clap(flatten)]
    pub lint_flag: LintFlag,
}

#[derive(
    Parser, Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, PartialOrd, Default,
)]
pub struct LintFlag {
    /// If `true`, disable linters
    #[clap(
        name = "no-lint",
        long = "no-lint",
        global = true,
        group = "lint-level"
    )]
    no_lint: bool,

    /// If `true`, enables extra linters
    #[clap(name = "lint", long = "lint", global = true, group = "lint-level")]
    lint: bool,
}

impl LintFlag {
    pub const LEVEL_NONE: Self = Self {
        no_lint: true,
        lint: false,
    };
    pub const LEVEL_DEFAULT: Self = Self {
        no_lint: false,
        lint: false,
    };
    pub const LEVEL_ALL: Self = Self {
        no_lint: false,
        lint: true,
    };

    pub fn get(self) -> LintLevel {
        match self {
            Self::LEVEL_NONE => LintLevel::None,
            Self::LEVEL_DEFAULT => LintLevel::Default,
            Self::LEVEL_ALL => LintLevel::All,
            _ => unreachable!(),
        }
    }

    pub fn set(&mut self, level: LintLevel) {
        *self = level.into();
    }
}

impl From<LintLevel> for LintFlag {
    fn from(level: LintLevel) -> Self {
        match level {
            LintLevel::None => Self::LEVEL_NONE,
            LintLevel::Default => Self::LEVEL_DEFAULT,
            LintLevel::All => Self::LEVEL_ALL,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd)]
pub struct ModelConfig {
    /// If set, also files which are in dependent packages are considered as targets.
    pub all_files_as_targets: bool,
    /// If set, a string how targets are filtered. A target is included if its file name
    /// contains this string. This is similar as the `cargo test <string>` idiom.
    pub target_filter: Option<String>,
}

impl BuildConfig {
    /// Compile the package at `path` or the containing Move package. Exit process on warning or
    /// failure.
    pub fn compile_package<W: Write>(self, path: &Path, writer: &mut W) -> Result<CompiledPackage> {
        let resolved_graph = self.resolution_graph_for_package(path, writer)?;
        let _mutx = PackageLock::lock(); // held until function returns
        BuildPlan::create(resolved_graph)?.compile(writer)
    }

    /// Compile the package at `path` or the containing Move package. Exit process on warning or
    /// failure. Will trigger migration if the package is missing an edition.
    pub fn cli_compile_package<W: Write, R: BufRead>(
        self,
        path: &Path,
        writer: &mut W,
        _reader: &mut R, // Reader here for enabling migration mode
    ) -> Result<CompiledPackage> {
        let resolved_graph = self.resolution_graph_for_package(path, writer)?;
        let _mutx = PackageLock::lock(); // held until function returns
        let build_plan = BuildPlan::create(resolved_graph)?;
        // TODO: When we are ready to release and enable automatic migration, uncomment this.
        // if !build_plan.root_crate_edition_defined() {
        //     // We would also like to call build here, but the edition is already computed and
        //     // the lock + build config have been used for this build already. The user will
        //     // have to call build a second time -- this is reasonable...
        //     migration::migrate(build_plan, writer, _reader)?;
        // } else {
        //     build_plan.compile(writer)
        // }
        build_plan.compile(writer)
    }

    /// Compile the package at `path` or the containing Move package. Do not exit process on warning
    /// or failure.
    pub fn compile_package_no_exit<W: Write>(
        self,
        path: &Path,
        writer: &mut W,
    ) -> Result<CompiledPackage> {
        let resolved_graph = self.resolution_graph_for_package(path, writer)?;
        let _mutx = PackageLock::lock(); // held until function returns
        BuildPlan::create(resolved_graph)?.compile_no_exit(writer)
    }

    /// Compile the package at `path` or the containing Move package. Exit process on warning or
    /// failure.
    pub fn migrate_package<W: Write, R: BufRead>(
        mut self,
        path: &Path,
        writer: &mut W,
        reader: &mut R,
    ) -> Result<()> {
        // we set test and dev mode to migrate all the code
        self.test_mode = true;
        self.dev_mode = true;
        let resolved_graph = self.resolution_graph_for_package(path, writer)?;
        let _mutx = PackageLock::lock(); // held until function returns
        let build_plan = BuildPlan::create(resolved_graph)?;
        migration::migrate(build_plan, writer, reader)?;
        Ok(())
    }

    // NOTE: If there are no renamings, then the root package has the global resolution of all named
    // addresses in the package graph in scope. So we can simply grab all of the source files
    // across all packages and build the Move model from that.
    // TODO: In the future we will need a better way to do this to support renaming in packages
    // where we want to support building a Move model.
    pub fn move_model_for_package(
        self,
        path: &Path,
        model_config: ModelConfig,
    ) -> Result<GlobalEnv> {
        // resolution graph diagnostics are only needed for CLI commands so ignore them by passing a
        // vector as the writer
        let resolved_graph = self.resolution_graph_for_package(path, &mut Vec::new())?;
        let _mutx = PackageLock::lock(); // held until function returns
        ModelBuilder::create(resolved_graph, model_config).build_model()
    }

    pub fn download_deps_for_package<W: Write>(&self, path: &Path, writer: &mut W) -> Result<()> {
        let path = SourcePackageLayout::try_find_root(path)?;
        let manifest_string =
            std::fs::read_to_string(path.join(SourcePackageLayout::Manifest.path()))?;
        let lock_string = std::fs::read_to_string(path.join(SourcePackageLayout::Lock.path())).ok();
        let _mutx = PackageLock::lock(); // held until function returns

        resolution::download_dependency_repos(manifest_string, lock_string, self, &path, writer)?;
        Ok(())
    }

    pub fn resolution_graph_for_package<W: Write>(
        mut self,
        path: &Path,
        writer: &mut W,
    ) -> Result<ResolvedGraph> {
        if self.test_mode {
            self.dev_mode = true;
        }
        let path = SourcePackageLayout::try_find_root(path)?;
        let manifest_string =
            std::fs::read_to_string(path.join(SourcePackageLayout::Manifest.path()))?;
        let lock_path = path.join(SourcePackageLayout::Lock.path());
        let lock_string = std::fs::read_to_string(lock_path.clone()).ok();
        let _mutx = PackageLock::lock(); // held until function returns

        let install_dir_set = self.install_dir.is_some();
        let install_dir = self.install_dir.as_ref().unwrap_or(&path).to_owned();

        let mut dep_graph_builder = DependencyGraphBuilder::new(
            self.skip_fetch_latest_git_deps,
            writer,
            install_dir.clone(),
        );
        let (dependency_graph, modified) = dep_graph_builder.get_graph(
            &DependencyKind::default(),
            path,
            manifest_string,
            lock_string,
        )?;

        if modified || install_dir_set {
            // (1) Write the Move.lock file if the existing one is `modified`, or
            // (2) `install_dir` is set explicitly, which may be a different directory, and where a Move.lock does not exist yet.
            let lock = dependency_graph.write_to_lock(install_dir, Some(lock_path))?;
            if let Some(lock_path) = &self.lock_file {
                lock.commit(lock_path)?;
            }
        }

        let DependencyGraphBuilder {
            mut dependency_cache,
            progress_output,
            ..
        } = dep_graph_builder;

        ResolvedGraph::resolve(
            dependency_graph,
            self,
            &mut dependency_cache,
            progress_output,
        )
    }

    pub fn compiler_flags(&self) -> Flags {
        let flags = if self.test_mode {
            Flags::testing()
        } else {
            Flags::empty()
        };
        flags
            .set_warnings_are_errors(self.warnings_are_errors)
            .set_json_errors(self.json_errors)
            .set_silence_warnings(self.silence_warnings)
    }

    pub fn update_lock_file_toolchain_version(
        &self,
        path: &Path,
        compiler_version: String,
    ) -> Result<()> {
        let Some(lock_file) = self.lock_file.as_ref() else {
            return Ok(());
        };
        let path = &SourcePackageLayout::try_find_root(path)
            .map_err(|e| anyhow!("Unable to find package root for {}: {e}", path.display()))?;

        // Resolve edition and flavor from `Move.toml` or assign defaults.
        let manifest_string =
            std::fs::read_to_string(path.join(SourcePackageLayout::Manifest.path()))?;
        let toml_manifest = parse_move_manifest_string(manifest_string.clone())?;
        let root_manifest = parse_source_manifest(toml_manifest)?;
        let edition = root_manifest
            .package
            .edition
            .or(self.default_edition)
            .unwrap_or_default();
        let flavor = root_manifest
            .package
            .flavor
            .or(self.default_flavor)
            .unwrap_or_default();

        let install_dir = self.install_dir.as_ref().unwrap_or(path).to_owned();
        let mut lock = LockFile::from(install_dir, lock_file)?;
        update_compiler_toolchain(&mut lock, compiler_version, edition, flavor)?;
        let _mutx = PackageLock::lock();
        lock.commit(lock_file)?;
        Ok(())
    }
}
