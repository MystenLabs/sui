// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

use move_package_alt::{
    dependency::{self, CombinedDependency, Dependency, DependencySet, PinnedDependencyInfo},
    errors::{FileHandle, PackageResult},
    flavor::MoveFlavor,
    git::GitCache,
    package::PackageName,
    schema::{EnvironmentID, GitSha, ManifestDependencyInfo, ManifestGitDependency},
};
use serde::{Deserialize, Serialize};
use sui_package_management::system_package_versions::{
    latest_system_packages, SystemPackagesVersion, SYSTEM_GIT_REPO,
};

#[derive(Debug)]
pub struct SuiFlavor;

impl MoveFlavor for SuiFlavor {
    fn name() -> String {
        "sui".to_string()
    }

    type PublishedMetadata = (); // TODO

    type AddressInfo = (); // TODO

    type PackageMetadata = (); // TODO

    fn implicit_deps(
        file_handle: FileHandle,
        environments: impl Iterator<Item = EnvironmentID>,
    ) -> DependencySet<CombinedDependency> {
        let mut deps = DependencySet::new();
        let deps_to_skip = ["DeepBook".to_string()];
        let packages = latest_system_packages();
        let sha = &packages.git_revision;
        for env in environments {
            let pkgs = packages
                .packages
                .iter()
                .filter(|package| !deps_to_skip.contains(&package.package_name));

            for package in pkgs {
                let repo = SYSTEM_GIT_REPO.to_string();
                let dep = ManifestDependencyInfo::Git(ManifestGitDependency {
                    repo: repo.clone(),
                    rev: Some(sha.clone()),
                    subdir: PathBuf::from(&package.repo_path),
                });

                let dep_info = CombinedDependency::new(Dependency::new(
                    dep,
                    env.clone(),
                    None, // TODO: is this correct? We don't have this information
                    // here
                    file_handle,
                    true,
                ));

                // TODO: should we change the package name to be of tyep Identifier rather than
                // string?
                deps.insert(
                    env.clone(),
                    move_core_types::identifier::Identifier::new(package.package_name.clone())
                        .expect("valid package name"),
                    dep_info,
                );
            }
        }

        deps
    }
}
