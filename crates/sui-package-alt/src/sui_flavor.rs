// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

use indexmap::IndexMap;
use move_compiler::editions::Edition;
use move_package_alt::{
    MoveFlavor,
    schema::{
        EnvironmentID, EnvironmentName, GitSha, LockfileDependencyInfo, LockfileGitDepInfo,
        PackageName, ParsedManifest, ReplacementDependency, SystemDepName,
    },
};

use serde::{Deserialize, Serialize};
use sui_package_management::system_package_versions::{
    SYSTEM_GIT_REPO, latest_system_packages, system_packages_for_protocol,
};
use sui_sdk::types::{base_types::ObjectID, is_system_package};
use sui_protocol_config::ProtocolVersion;
use tokio::sync::OnceCell;
use tracing::warn;

use crate::{mainnet_environment, testnet_environment};

const EDITION: &str = "2024";
const FLAVOR: &str = "sui";

/// The Sui-specific implementation of the [MoveFlavor] trait.
///
/// Can be constructed in offline mode ([`SuiFlavor::new`]) or connected mode
/// ([`SuiFlavor::with_rpc`]). In connected mode, queries the RPC endpoint to determine the
/// network's protocol version for correct system dependency resolution.
#[derive(Debug)]
pub struct SuiFlavor {
    /// The RPC endpoint for the target network. `None` for offline/test usage.
    rpc_endpoint: Option<String>,
    /// Lazily populated from RPC when needed.
    protocol_version: OnceCell<ProtocolVersion>,
}

impl SuiFlavor {
    /// Create a `SuiFlavor` in offline mode. Uses the latest known system packages.
    pub fn new() -> Self {
        Self {
            rpc_endpoint: None,
            protocol_version: OnceCell::new(),
        }
    }

    /// Create a `SuiFlavor` connected to the given `rpc_endpoint`. Queries the network's protocol
    /// version to resolve the correct system dependencies.
    pub fn with_rpc(rpc_endpoint: String) -> Self {
        Self {
            rpc_endpoint: Some(rpc_endpoint),
            protocol_version: OnceCell::new(),
        }
    }

    /// Return the RPC endpoint, if configured.
    pub fn rpc_endpoint(&self) -> Option<&str> {
        self.rpc_endpoint.as_deref()
    }

    /// Return the protocol version for the target network. Lazily queries the RPC endpoint if
    /// available, falling back to the latest known version.
    async fn protocol_version(&self) -> ProtocolVersion {
        *self
            .protocol_version
            .get_or_init(|| async {
                if let Some(ref endpoint) = self.rpc_endpoint {
                    match Self::query_protocol_version(endpoint).await {
                        Ok(version) => version,
                        Err(e) => {
                            warn!(
                                "Failed to query protocol version from {endpoint}: {e}. \
                                 Falling back to latest known version."
                            );
                            ProtocolVersion::MAX
                        }
                    }
                } else {
                    ProtocolVersion::MAX
                }
            })
            .await
    }

    /// Query the protocol version from the given RPC `endpoint`.
    async fn query_protocol_version(endpoint: &str) -> anyhow::Result<ProtocolVersion> {
        // TODO: implement actual RPC query
        // For now, return MAX as a placeholder until we wire up the gRPC client
        let _ = endpoint;
        Ok(ProtocolVersion::MAX)
    }

    /// A map between system package names in the old style (capitalized) to the new naming style
    /// (lowercase).
    fn system_deps_by_name() -> BTreeMap<String, SystemDepName> {
        BTreeMap::from([
            ("Sui".into(), "sui".into()),
            ("SuiSystem".into(), "sui_system".into()),
            ("MoveStdlib".into(), "std".into()),
            ("Bridge".into(), "bridge".into()),
            ("DeepBook".into(), "deepbook".into()),
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
    fn name(&self) -> String {
        FLAVOR.to_string()
    }

    type PublishedMetadata = PublishedMetadata;

    type AddressInfo = (); // TODO

    type PackageMetadata = (); // TODO

    fn default_environments(&self) -> IndexMap<EnvironmentName, EnvironmentID> {
        let testnet = testnet_environment();
        let mainnet = mainnet_environment();
        IndexMap::from([(testnet.name, testnet.id), (mainnet.name, mainnet.id)])
    }

    async fn implicit_dependencies(
        &self,
        _: &EnvironmentID,
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

    async fn system_deps(
        &self,
        _: &EnvironmentID,
    ) -> BTreeMap<SystemDepName, LockfileDependencyInfo> {
        let mut deps = BTreeMap::new();
        let deps_to_skip = ["DeepBook".into()];

        let version = self.protocol_version().await;
        let packages = match system_packages_for_protocol(version) {
            Ok((pkgs, _)) => pkgs,
            Err(e) => {
                warn!(
                    "Failed to resolve system packages for protocol version {}: {e}. \
                     Falling back to latest.",
                    version.as_u64()
                );
                latest_system_packages()
            }
        };
        let sha = &packages.git_revision;
        // filter out the packages that we want to skip
        let pkgs = packages
            .packages
            .iter()
            .filter(|package| !deps_to_skip.contains(&package.package_name));

        let names = Self::system_deps_by_name();
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

    fn validate_manifest(&self, manifest: &ParsedManifest) -> Result<(), String> {
        validate_modern_manifest_does_not_use_legacy_system_names(manifest)?;
        if manifest.package.edition == Some(Edition::DEVELOPMENT) {
            Err(Edition::DEVELOPMENT.unknown_edition_error().to_string())
        } else {
            Ok(())
        }
    }

    fn is_system_address(&self, address: &move_package_alt::schema::OriginalID) -> bool {
        is_system_package(address.0)
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

    let legacy_names = SuiFlavor::system_deps_by_name();

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
