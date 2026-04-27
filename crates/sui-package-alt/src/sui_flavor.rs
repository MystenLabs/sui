// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

use anyhow::Context;
use async_trait::async_trait;
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
use sui_protocol_config::ProtocolVersion;
use sui_rpc_api::Client as RpcClient;
use sui_sdk::types::{base_types::ObjectID, is_system_package};
use sui_sdk::wallet_context::WalletContext;
use tokio::sync::OnceCell;
use tracing::warn;

use crate::{mainnet_environment, testnet_environment};

const EDITION: &str = "2024";
const FLAVOR: &str = "sui";

/// The Sui-specific implementation of the [MoveFlavor] trait.
///
/// Can be constructed in offline mode ([`SuiFlavor::new`]) or connected mode
/// ([`SuiFlavor::with_client`]). In connected mode, queries the network via gRPC to determine the
/// protocol version for correct system dependency resolution.
#[derive(Clone, Default)]
pub struct SuiFlavor {
    /// The gRPC client for the target network. `None` for offline/test usage.
    client: Option<RpcClient>,
    /// Lazily populated from gRPC when needed.
    protocol_version: OnceCell<ProtocolVersion>,
}

impl std::fmt::Debug for SuiFlavor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SuiFlavor")
            .field("connected", &self.client.is_some())
            .field("protocol_version", &self.protocol_version)
            .finish()
    }
}

impl SuiFlavor {
    /// Create a `SuiFlavor` in offline mode. Uses the latest known system packages.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a `SuiFlavor` connected to the network via the gRPC client from `wallet`. Falls back
    /// to offline mode if the client can't be created.
    pub fn with_client(wallet: &WalletContext) -> Self {
        match wallet.grpc_client() {
            Ok(client) => Self {
                client: Some(client),
                ..Default::default()
            },
            Err(_) => Self::new(),
        }
    }

    /// Return the protocol version for the target network. Lazily queries the gRPC endpoint if
    /// available, falling back to the latest known version.
    async fn protocol_version(&self) -> ProtocolVersion {
        *self
            .protocol_version
            .get_or_init(|| async {
                if let Some(ref client) = self.client {
                    Self::query_protocol_version(client)
                        .await
                        .inspect_err(|e| {
                            warn!(
                                "Failed to query protocol version: {e}. \
                                 Falling back to protocol version {}.",
                                ProtocolVersion::MAX.as_u64()
                            )
                        })
                        .unwrap_or(ProtocolVersion::MAX)
                } else {
                    ProtocolVersion::MAX
                }
            })
            .await
    }

    /// Query the protocol version from the network via gRPC.
    async fn query_protocol_version(client: &RpcClient) -> anyhow::Result<ProtocolVersion> {
        let config = client
            .get_protocol_config(None)
            .await
            .with_context(|| "Failed to query protocol config")?;
        let version = config
            .protocol_version_opt()
            .context("Protocol config response missing protocol_version")?;
        Ok(ProtocolVersion::new(version))
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

#[async_trait]
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
                     Falling back to latest known packages (protocol version {}).",
                    version.as_u64(),
                    ProtocolVersion::MAX.as_u64()
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
