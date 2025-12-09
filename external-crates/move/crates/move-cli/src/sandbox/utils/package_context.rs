// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};

use crate::{DEFAULT_BUILD_DIR, sandbox::utils::OnDiskStateView};
use move_package_alt::flavor::MoveFlavor;
use move_package_alt_compilation::{
    build_config::BuildConfig, compiled_package::CompiledPackage, find_env,
};
use move_vm_runtime::dev_utils::storage::StoredPackage;

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
    /// storage. Note that only the package's dependencies will be "published" and the package
    /// itself will not be published.
    ///
    /// NOTE: this is the only way to get a state view in Move CLI, and thus, this function needs
    /// to be run before every command that needs a state view, i.e., `publish`, `run`,
    /// and `view`
    pub fn prepare_state(&self, storage_dir: &Path) -> Result<OnDiskStateView> {
        let state = OnDiskStateView::create(self.build_dir.as_path(), storage_dir)?;

        // preload the storage with library modules (if such modules do not exist yet)
        let package = self.package();

        // Separate dependencies into packages based on their package name, and verify that all
        // modules in a package have the same runtime address.
        let mut package_id_mapping = BTreeMap::new();
        for (name, module) in package.deps_compiled_units.iter() {
            let id = package_id_mapping
                .entry(name)
                .or_insert((*module.unit.module.self_id().address(), vec![]));
            if id.0 != *module.unit.module.self_id().address() {
                bail!(
                    "All modules in the package must have the same address but the address for {name} \
                     has value {} which is different from the runtime address of the package {}",
                    module.unit.module.self_id().address(),
                    id.0,
                );
            }
            id.1.push(module.unit.module.clone());
        }

        for (package_id, package) in package_id_mapping.into_values() {
            let pkg = StoredPackage::from_modules_for_testing(package_id, package)?;
            state.save_package(pkg.into_serialized_package())?;
        }

        Ok(state)
    }

    pub fn package(&self) -> &CompiledPackage {
        &self.package
    }
}
