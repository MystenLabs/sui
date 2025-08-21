// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use move_package_alt::{
    dependency::{self, CombinedDependency, DependencySet, PinnedDependencyInfo},
    errors::{FileHandle, PackageResult},
    flavor::MoveFlavor,
    git::GitCache,
    schema::{
        EnvironmentID, EnvironmentName, GitSha, ManifestDependencyInfo, ManifestGitDependency,
        PackageName, ReplacementDependency,
    },
};
use serde::{Deserialize, Serialize};
use sui_package_management::system_package_versions::{
    latest_system_packages, system_packages_for_protocol, SystemPackagesVersion, SYSTEM_GIT_REPO,
};

#[derive(Debug)]
pub struct SuiFlavor;

impl SuiFlavor {
    /// A map between system package names in the old style (capitalized) to the new naming style
    /// (lowercase).
    fn system_deps_names_map() -> BTreeMap<PackageName, PackageName> {
        BTreeMap::from([
            (
                PackageName::new("Sui").unwrap(),
                PackageName::new("sui").unwrap(),
            ),
            (
                PackageName::new("SuiSystem").unwrap(),
                PackageName::new("sui_system").unwrap(),
            ),
            (
                PackageName::new("MoveStdlib").unwrap(),
                PackageName::new("std").unwrap(),
            ),
            (
                PackageName::new("Bridge").unwrap(),
                PackageName::new("bridge").unwrap(),
            ),
            (
                PackageName::new("DeepBook").unwrap(),
                PackageName::new("deepbook").unwrap(),
            ),
        ])
    }

    /// The default dependencies are `sui` and `std`
    fn default_system_dep_names() -> BTreeSet<PackageName> {
        BTreeSet::from([
            PackageName::new("sui").unwrap(),
            PackageName::new("std").unwrap(),
        ])
    }
}

impl MoveFlavor for SuiFlavor {
    fn name() -> String {
        "sui".to_string()
    }

    type PublishedMetadata = (); // TODO

    type AddressInfo = (); // TODO

    type PackageMetadata = (); // TODO

    fn default_environments() -> BTreeMap<EnvironmentName, EnvironmentID> {
        BTreeMap::from([
            ("mainnet".to_string(), "35834a8a".to_string()),
            ("testnet".to_string(), "4c78adac".to_string()),
        ])
    }

    // TODO this needs fixing, see todos
    fn system_dependencies(
        environment: EnvironmentID,
    ) -> BTreeMap<PackageName, ReplacementDependency> {
        let mut deps = BTreeMap::new();
        let deps_to_skip = ["DeepBook".into()];
        // TODO: we need to use packages for protocol version as well, so we need to fix this
        let packages = latest_system_packages();
        let sha = &packages.git_revision;
        // filter out the packages that we want to skip
        let pkgs = packages
            .packages
            .iter()
            .filter(|package| !deps_to_skip.contains(&package.package_name));

        let names = Self::system_deps_names_map();
        for package in pkgs {
            let repo = SYSTEM_GIT_REPO.to_string();
            let dependency_info = ManifestDependencyInfo::Git(ManifestGitDependency {
                repo: repo.clone(),
                rev: Some(sha.clone()),
                subdir: PathBuf::from(&package.repo_path),
            });

            let replacement_dep = ReplacementDependency {
                dependency: Some(move_package_alt::schema::DefaultDependency {
                    dependency_info,
                    is_override: true,
                    rename_from: None,
                }),
                addresses: None,
                use_environment: None,
            };

            deps.insert(
                names
                    .get(
                        &PackageName::new(package.package_name.clone())
                            .expect("valid package name"),
                    )
                    .expect("package exists in the renaming table")
                    .clone(),
                replacement_dep,
            );
        }

        deps
    }

    fn default_system_dependencies(
        environment: EnvironmentID,
    ) -> BTreeMap<PackageName, ReplacementDependency> {
        let default_deps = Self::default_system_dep_names();

        Self::system_dependencies(environment)
            .into_iter()
            .filter(|(name, _)| default_deps.contains(name))
            .collect()
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use move_package_alt::package::manifest::Manifest;
    use move_package_alt::package::RootPackage;
    use move_package_alt::schema::{Environment, PackageName};

    #[test]
    fn test_implicit_deps() {
        let implicit_deps = ["sui", "std", "sui_system", "bridge"];
        let env = Environment::new("mainnet".into(), "35834a8a".into());

        let deps = SuiFlavor::system_dependencies(env.id().into());

        for i in implicit_deps {
            assert!(
                deps.contains_key(&PackageName::new(i).unwrap()),
                "Dependency {} not found in implicit dependencies",
                i
            );
            assert!(
                !deps.contains_key(&PackageName::new("DeepBook").unwrap()),
                "Dependency DeepBook should not be in the implicit dependencies"
            );
            assert!(
                !deps.contains_key(&PackageName::new("deepbook").unwrap()),
                "Dependency deepbook should not be in the implicit dependencies"
            );
        }
    }
}
