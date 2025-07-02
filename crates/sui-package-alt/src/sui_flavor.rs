// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

use move_package_alt::{
    dependency::{self, CombinedDependency, Dependency, DependencySet, PinnedDependencyInfo},
    errors::{FileHandle, PackageResult},
    flavor::MoveFlavor,
    git::GitCache,
    package::PackageName,
    schema::{
        EnvironmentID, GitSha, ManifestDependencyInfo, ManifestGitDependency, ReplacementDependency,
    },
};
use serde::{Deserialize, Serialize};
use sui_package_management::system_package_versions::{
    latest_system_packages, system_packages_for_protocol, SystemPackagesVersion, SYSTEM_GIT_REPO,
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

    fn default_environments() -> BTreeMap<EnvironmentName, EnvironmentID> {
        todo!()
    }

    fn implicit_deps(environment: EnvironmentID) -> BTreeMap<PackageName, ReplacementDependency> {
        let names = BTreeMap::from([
            ("Sui", "sui"),
            ("SuiSystem", "sui_system"),
            ("MoveStdlib", "std"),
            ("Bridge", "bridge"),
            ("DeepBook", "deepbook"),
        ]);

        let mut deps = BTreeMap::new();
        let deps_to_skip = ["DeepBook".to_string()];
        let packages = latest_system_packages();
        let sha = &packages.git_revision;
        // filter out the packages that we want to skip
        let pkgs = packages
            .packages
            .iter()
            .filter(|package| !deps_to_skip.contains(&package.package_name));

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
                PackageName::new(
                    *names
                        .get(&package.package_name)
                        .expect("package exists in the renaming table"),
                )
                .expect("system package names are valid identifiers"),
                replacement_dep,
            );
        }

        deps
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use move_package_alt::package::manifest::Manifest;
    use move_package_alt::schema::PackageName;

    #[test]
    fn test_implicit_deps() {
        let implicit_deps = ["sui", "std", "sui_system", "bridge"];
        let temp_dir = tempfile::tempdir().unwrap();

        let pkg_manifest_path = temp_dir.path().join("Move.toml");

        std::fs::write(
            &pkg_manifest_path,
            r#"
        [package]
        name = "test"
        version = "1"
        authors = []
        edition = "2025"

        [environments]
        mainnet = "35834a8a"
        testnet = "4c78adac"
        hello   = "fc78adef"
    "#,
        );

        let pkg_graph = Manifest::<SuiFlavor>::read_from_file(&pkg_manifest_path)
            .expect("should read manifest");

        let deps = pkg_graph.dependencies();

        for e in ["mainnet", "testnet", "hello"] {
            for i in implicit_deps {
                assert!(
                    deps.contains(&e.to_string(), &PackageName::new(i).unwrap()),
                    "Dependency {} not found in manifest",
                    i
                );
            }

            assert!(
                !deps.contains(&e.to_string(), &PackageName::new("DeepBook").unwrap()),
                "Dependency DeepBook should not be in the implicit dependencies"
            );
            assert!(
                !deps.contains(&e.to_string(), &PackageName::new("deepbook").unwrap()),
                "Dependency deepbook should not be in the implicit dependencies"
            );
        }
    }
}
