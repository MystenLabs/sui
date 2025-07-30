// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::io::Write;

use vfs::VfsPath;

use crate::{
    build_config::BuildConfig,
    compiled_package::{CompiledPackage, build_all},
};
use move_compiler::{
    Compiler,
    compiled_unit::AnnotatedCompiledUnit,
    shared::{SaveFlag, SaveHook, files::MappedFiles},
};
use move_package_alt::{errors::PackageResult, flavor::MoveFlavor, package::RootPackage};

#[derive(Debug)]
pub struct BuildPlan<F: MoveFlavor> {
    root_pkg: RootPackage<F>,
    compiler_vfs_root: Option<VfsPath>,
    build_config: BuildConfig,
}

impl<F: MoveFlavor> BuildPlan<F> {
    pub fn create(root_pkg: RootPackage<F>, build_config: &BuildConfig) -> PackageResult<Self> {
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

    // TODO do we need this?
    pub fn root_crate_edition_defined(&self) -> bool {
        false
        // self.resolution_graph.package_table[&self.root]
        //     .source_package
        //     .package
        //     .edition
        //     .is_some()
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
        self,
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

        // let project_root = root_pkg.package_path().path();
        // let sorted_deps = compiled.get_dependency_sorted_modules();

        // clean(
        //     &project_root.join(CompiledPackageLayout::Root.path()),
        //     sorted_deps.iter().copied().collect(),
        // )?;

        Ok(compiled)
    }

    // Clean out old packages that are no longer used, or no longer used under the current
    // compilation flags
    // fn clean(&self, build_root: &Path, keep_paths: BTreeSet<PackageName>) -> PackageResult<()> {
    //     for dir in std::fs::read_dir(build_root)? {
    //         let path = dir?.path();
    //         if !keep_paths.iter().any(|name| path.ends_with(name.as_str())) {
    //             if path.is_file() {
    //                 std::fs::remove_file(&path)?;
    //             } else {
    //                 std::fs::remove_dir_all(&path)?;
    //             }
    //         }
    //     }
    //     Ok(())
    // }
}
