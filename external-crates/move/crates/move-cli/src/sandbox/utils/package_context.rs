// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::{DEFAULT_BUILD_DIR, sandbox::utils::OnDiskStateView};
use move_package_alt::flavor::MoveFlavor;
use move_package_alt_compilation::{
    build_config::BuildConfig, compiled_package::CompiledPackage, find_env,
};

/// The PackageContext controls the package that the CLI is executing with respect to, and handles the
/// creation of the `OnDiskStateView` with the package's dependencies.
pub struct PackageContext {
    package: CompiledPackage,
    build_dir: PathBuf,
}

impl PackageContext {
    pub async fn new<F: MoveFlavor>(
        path: &Option<PathBuf>,
        build_config: &BuildConfig,
    ) -> Result<Self> {
        let path = path.as_deref().unwrap_or_else(|| Path::new("."));
        let env = find_env::<F>(path, build_config)?;
        let build_dir = build_config
            .install_dir
            .as_ref()
            .unwrap_or(&PathBuf::from(DEFAULT_BUILD_DIR))
            .clone();

        let package = build_config
            .compile_package::<F, _>(path, &env, &mut Vec::new())
            .await?;
        Ok(PackageContext { package, build_dir })
    }

    /// Prepare an OnDiskStateView that is ready to use. Library modules will be preloaded into the
    /// storage if `load_libraries` is true.
    ///
    /// NOTE: this is the only way to get a state view in Move CLI, and thus, this function needs
    /// to be run before every command that needs a state view, i.e., `publish`, `run`,
    /// `view`, and `doctor`.
    pub fn prepare_state(&self, storage_dir: &Path) -> Result<OnDiskStateView> {
        let state = OnDiskStateView::create(self.build_dir.as_path(), storage_dir)?;

        // preload the storage with library modules (if such modules do not exist yet)
        let package = self.package();
        let new_modules = package
            .deps_compiled_units
            .iter()
            .map(|(_, unit)| &unit.unit.module)
            .filter(|m| !state.has_module(&m.self_id()));

        let mut serialized_modules = vec![];
        for module in new_modules {
            let self_id = module.self_id();
            let mut module_bytes = vec![];
            module.serialize_with_version(module.version, &mut module_bytes)?;
            serialized_modules.push((self_id, module_bytes));
        }
        state.save_modules(&serialized_modules)?;

        Ok(state)
    }

    pub fn package(&self) -> &CompiledPackage {
        &self.package
    }
}
