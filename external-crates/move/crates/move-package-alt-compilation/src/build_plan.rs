// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, io::Write, path::Path};

use toml_edit::{DocumentMut, value};
use vfs::VfsPath;

use crate::{
    build_config::BuildConfig,
    compilation::{build_all, build_for_driver},
    compiled_package::CompiledPackage,
    layout::CompiledPackageLayout,
    shared,
};

use move_compiler::{
    Compiler,
    compiled_unit::AnnotatedCompiledUnit,
    diagnostics::{Migration, report_diagnostics_to_buffer_with_env_color},
    editions::Edition,
    shared::{SaveFlag, SaveHook, files::MappedFiles},
};
use move_package_alt::{
    errors::{PackageError, PackageResult},
    flavor::MoveFlavor,
    package::RootPackage,
    schema::PackageName,
};
use move_symbol_pool::Symbol;

const EDITION_NAME: &str = "edition";
const PACKAGE_NAME: &str = "package";

#[derive(Debug)]
pub struct BuildPlan<'a, F: MoveFlavor> {
    root_pkg: &'a RootPackage<F>,
    compiler_vfs_root: Option<VfsPath>,
    build_config: BuildConfig,
}

impl<'a, F: MoveFlavor> BuildPlan<'a, F> {
    pub fn create(root_pkg: &'a RootPackage<F>, build_config: &BuildConfig) -> PackageResult<Self> {
        Ok(Self {
            root_pkg,
            build_config: build_config.clone(),
            compiler_vfs_root: None,
        })
    }

    pub fn set_compiler_vfs_root(mut self, vfs_root: VfsPath) -> Self {
        assert!(self.compiler_vfs_root.is_none());
        self.compiler_vfs_root = Some(vfs_root);
        self
    }

    /// Compilation results in the process exit upon warning/failure
    pub fn compile<W: Write>(
        self,
        writer: &mut W,
        modify_compiler: impl FnOnce(Compiler) -> Compiler,
    ) -> PackageResult<CompiledPackage> {
        self.compile_with_driver(writer, |compiler| {
            modify_compiler(compiler).build_and_report()
        })
    }

    pub fn compile_with_driver<W: Write>(
        &self,
        writer: &mut W,
        compiler_driver: impl FnOnce(
            Compiler,
        )
            -> anyhow::Result<(MappedFiles, Vec<AnnotatedCompiledUnit>)>,
    ) -> PackageResult<CompiledPackage> {
        let program_info_hook = SaveHook::new([SaveFlag::TypingInfo]);
        let compiled = build_all::<W, F>(
            writer,
            self.compiler_vfs_root.clone(),
            self.root_pkg,
            &self.build_config,
            |compiler| {
                let compiler = compiler.add_save_hook(&program_info_hook);
                compiler_driver(compiler)
            },
        )?;

        let project_root = self.root_pkg.package_path().path();
        let sorted_deps = self.root_pkg.sorted_deps().into_iter().cloned().collect();

        self.clean(
            &project_root.join(CompiledPackageLayout::Root.path()),
            sorted_deps,
        )?;

        Ok(compiled)
    }

    /// Compilation process does not exit even if warnings/failures are encountered
    pub fn compile_no_exit<W: Write>(
        &self,
        writer: &mut W,
        modify_compiler: impl FnOnce(Compiler) -> Compiler,
    ) -> PackageResult<CompiledPackage> {
        let mut diags = None;
        let res = self.compile_with_driver(writer, |compiler| {
            let (files, units_res) = modify_compiler(compiler).build()?;
            match units_res {
                Ok((units, warning_diags)) => {
                    diags = Some(report_diagnostics_to_buffer_with_env_color(
                        &files,
                        warning_diags,
                    ));
                    Ok((files, units))
                }
                Err(error_diags) => {
                    assert!(!error_diags.is_empty());
                    diags = Some(report_diagnostics_to_buffer_with_env_color(
                        &files,
                        error_diags,
                    ));
                    anyhow::bail!("Compilation error");
                }
            }
        });
        if let Some(diags) = diags {
            if let Err(err) = std::io::stdout().write_all(&diags) {
                return Err(PackageError::Generic(format!(
                    "Cannot output compiler diagnostics: {}",
                    err
                )));
            }
        }
        res
    }

    // Clean out old packages that are no longer used, or no longer used under the current
    // compilation flags
    fn clean(&self, project_root: &Path, keep_paths: BTreeSet<PackageName>) -> PackageResult<()> {
        // Compute the actual build directory based on install_dir configuration
        let build_root = shared::get_build_output_path(project_root, &self.build_config);

        // Skip cleaning if the build directory doesn't exist yet
        if !build_root.exists() {
            return Ok(());
        }

        for dir in std::fs::read_dir(&build_root)? {
            let path = dir?.path();
            if !keep_paths.iter().any(|name| path.ends_with(name.as_str())) {
                if path.is_file() {
                    std::fs::remove_file(&path)?;
                } else {
                    std::fs::remove_dir_all(&path)?;
                }
            }
        }
        Ok(())
    }

    /// Migrate the package from legacy to Move 2024 edition, if possible.
    pub fn migrate<W: Write>(&self, writer: &mut W) -> anyhow::Result<Option<Migration>> {
        let root_name = Symbol::from(self.root_pkg.name().to_string());
        let (files, res) = build_for_driver(
            writer,
            None,
            &self.build_config,
            self.root_pkg,
            |compiler| compiler.generate_migration_patch(&root_name),
        )?;
        let migration = match res {
            Ok(migration) => migration,
            Err(diags) => {
                let diags_buf = report_diagnostics_to_buffer_with_env_color(&files, diags);
                writeln!(
                    writer,
                    "Unable to generate migration patch due to compilation errors.\n\
                    Please fix the errors in your current edition before attempting to migrate."
                )?;
                if let Err(err) = writer.write_all(&diags_buf) {
                    anyhow::bail!("Cannot output compiler diagnostics: {}", err);
                }
                anyhow::bail!("Compilation error");
            }
        };

        let project_root = self.root_pkg.package_path().path();
        let sorted_deps = self.root_pkg.sorted_deps().into_iter().cloned().collect();

        self.clean(
            &project_root.join(CompiledPackageLayout::Root.path()),
            sorted_deps,
        )?;

        Ok(migration)
    }

    /// Rewrite the edition field in Move.toml to the given edition.
    pub fn record_package_edition(&self, edition: Edition) -> anyhow::Result<()> {
        let move_toml_path = self.root_pkg.path().manifest_path();
        let mut toml = std::fs::read_to_string(move_toml_path.clone())?
            .parse::<DocumentMut>()
            .expect("Failed to read TOML file to update edition");
        toml[PACKAGE_NAME][EDITION_NAME] = value(edition.to_string());
        std::fs::write(move_toml_path, toml.to_string())?;
        Ok(())
    }

    /// Get the path to the root package.
    pub fn root_package_path(&self) -> &Path {
        self.root_pkg.path().path()
    }
}
