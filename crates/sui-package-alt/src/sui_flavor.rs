// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use indexmap::IndexMap;
use move_compiler::editions::Edition;
use move_package_alt::{
    dependency::{self, CombinedDependency, PinnedDependencyInfo},
    errors::{FileHandle, PackageResult},
    flavor::MoveFlavor,
    git::GitCache,
    schema::{
        EnvironmentID, EnvironmentName, GitSha, LockfileDependencyInfo, LockfileGitDepInfo,
        ManifestDependencyInfo, ManifestGitDependency, PackageName, ParsedManifest,
        ReplacementDependency, SystemDepName,
    },
};
use serde::{Deserialize, Serialize};
use sui_package_management::system_package_versions::{
    SYSTEM_GIT_REPO, SystemPackagesVersion, latest_system_packages, system_packages_for_protocol,
};
use sui_sdk::types::{
    base_types::ObjectID,
    digests::{get_mainnet_chain_identifier, get_testnet_chain_identifier},
    supported_protocol_versions::Chain,
};

use crate::{mainnet_environment, testnet_environment};

const EDITION: &str = "2024";
const FLAVOR: &str = "sui";

#[derive(Debug, Serialize, Deserialize)]
pub struct SuiFlavor;

impl SuiFlavor {
    /// A map between system package names in the old style (capitalized) to the new naming style
    /// (lowercase).
    fn system_deps_names_map() -> BTreeMap<String, SystemDepName> {
        BTreeMap::from([
            ("Sui".into(), "sui".into()),
            ("SuiSystem".into(), "sui_system".into()),
            ("MoveStdlib".into(), "std".into()),
            ("Bridge".into(), "bridge".into()),
            ("DeepBook".into(), "deepbook".into()),
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BuildParams {
    pub flavor: String,
    pub edition: String,
}

/// Note: Every field should be optional, and the system can
/// pick sensible defaults (or error out) if fields are missing.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub struct PublishedMetadata {
    pub toolchain_version: Option<String>,
    pub build_config: Option<BuildParams>,
    pub upgrade_capability: Option<ObjectID>,
}

impl MoveFlavor for SuiFlavor {
    fn name() -> String {
        FLAVOR.to_string()
    }

    type PublishedMetadata = PublishedMetadata;

    type AddressInfo = (); // TODO

    type PackageMetadata = (); // TODO

    fn default_environments() -> IndexMap<EnvironmentName, EnvironmentID> {
        let testnet = testnet_environment();
        let mainnet = mainnet_environment();
        IndexMap::from([(testnet.name, testnet.id), (mainnet.name, mainnet.id)])
    }

    fn implicit_dependencies(
        environment: &EnvironmentID,
    ) -> BTreeMap<PackageName, ReplacementDependency> {
        BTreeMap::from([
            (
                PackageName::new("sui").expect("sui is a valid identifier"),
                ReplacementDependency::override_system_dep("sui"),
            ),
            (
                PackageName::new("std").expect("std is a valid identifier"),
                ReplacementDependency::override_system_dep("std"),
            ),
        ])
    }

    fn system_deps(environment: &EnvironmentID) -> BTreeMap<SystemDepName, LockfileDependencyInfo> {
        let mut deps = BTreeMap::new();
        let deps_to_skip = ["DeepBook".into()];

        // TODO DVX-1814: we need to use packages for protocol version instead of latest
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
            let info = LockfileDependencyInfo::Git(LockfileGitDepInfo {
                repo,
                path: PathBuf::from(&package.repo_path),
                rev: GitSha::try_from(sha.clone()).expect("manifest has valid sha"),
            });

            deps.insert(
                names
                    .get(&package.package_name)
                    .expect("package exists in the renaming table")
                    .clone(),
                info,
            );
        }

        deps
    }

    fn validate_manifest(manifest: &ParsedManifest) -> Result<(), String> {
        validate_modern_manifest_does_not_use_legacy_system_names(manifest)?;
        if manifest.package.edition == Some(Edition::DEVELOPMENT) {
            Err(Edition::DEVELOPMENT.unknown_edition_error().to_string())
        } else {
            Ok(())
        }
    }
}

/// We validate that a modern manifest cannot define the "legacy" system names.
/// This is mainly to protect users
fn validate_modern_manifest_does_not_use_legacy_system_names(
    manifest: &ParsedManifest,
) -> Result<(), String> {
    // For legacy data, we do not enforce this check.
    if manifest.legacy_data.is_some() {
        return Ok(());
    }

    // collect all manifest deps
    let mut dep_names = manifest
        .dependencies
        .keys()
        .map(|n| n.get_ref().to_string())
        .collect::<Vec<_>>();

    // Check "dep replacements" too.
    dep_names.extend(
        manifest
            .dep_replacements
            .values()
            .flat_map(|k| k.keys().map(|key| key.to_string()))
            .collect::<Vec<_>>(),
    );

    let legacy_names = SuiFlavor::system_deps_names_map();

    for name in dep_names {
        if legacy_names.contains_key(&name) {
            return Err(format!(
                "Dependency `{name}` is a legacy system name and cannot be used. See https://docs.sui.io/guides/developer/sui-101/move-package-management#system-dependencies"
            ));
        }
    }

    Ok(())
}

impl Default for BuildParams {
    fn default() -> Self {
        Self {
            flavor: FLAVOR.to_string(),
            edition: EDITION.to_string(),
        }
    }
}
