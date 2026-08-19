// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    time::Duration,
};

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
    user_note,
};

use serde::{Deserialize, Serialize};
use sui_package_management::system_package_versions::{
    SYSTEM_GIT_REPO, SystemPackagesVersion, latest_system_packages, system_packages_for_protocol,
};
use sui_protocol_config::ProtocolVersion;
use sui_sdk::sui_client_config::SuiEnv;
use sui_sdk::types::{base_types::ObjectID, is_system_package};
use sui_sdk::wallet_context::WalletContext;

use sui_sdk::digests::chain_ids_match;

use crate::{mainnet_environment, testnet_environment};

const EDITION: &str = "2024";
const FLAVOR: &str = "sui";

/// Give up on a candidate RPC endpoint after this long, so that an unreachable endpoint doesn't
/// stall the build; resolution moves on to the next candidate.
const ENDPOINT_TIMEOUT: Duration = Duration::from_secs(10);

/// System packages that ship in the framework release but are not offered as system dependencies.
const UNSUPPORTED_SYSTEM_PACKAGES: [&str; 1] = ["DeepBook"];

/// The Sui-specific implementation of the [MoveFlavor] trait.
///
/// Can be constructed in offline mode ([`SuiFlavor::new`]), where the system dependencies resolve
/// to the newest framework this binary ships, or in connected mode ([`SuiFlavor::with_client`]),
/// where they are pinned to the protocol version reported by an RPC endpoint serving the chain of
/// the environment being built. See [`SuiFlavor::system_deps`] for why the distinction matters.
#[derive(Clone, Default)]
pub struct SuiFlavor {
    /// The CLI environments used to locate an RPC endpoint for the environment being built.
    /// `None` in offline mode.
    envs: Option<Vec<SuiEnv>>,

    /// Alias of the CLI's active environment; preferred when several configured environments serve
    /// the same chain.
    active_env: Option<EnvironmentName>,
}

impl std::fmt::Debug for SuiFlavor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SuiFlavor")
            .field("connected", &self.envs.is_some())
            .field("active_env", &self.active_env)
            .finish()
    }
}

impl SuiFlavor {
    /// Create a `SuiFlavor` in offline mode, which pins the system dependencies to the newest
    /// framework known to this binary without consulting any chain.
    ///
    /// Only appropriate for callers that never publish and never write a lockfile that others will
    /// build from; anything that resolves dependencies on a user's behalf should use
    /// [`Self::with_client`] so that the pin is derived from the target chain.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a `SuiFlavor` that resolves the system dependencies for an environment by querying
    /// an RPC endpoint serving that environment's chain, chosen from `wallet`'s CLI configuration.
    pub fn with_client(wallet: &WalletContext) -> Self {
        Self {
            envs: Some(wallet.config.envs.clone()),
            active_env: wallet.get_active_env().ok().map(|env| env.alias.clone()),
        }
    }

    /// Return the framework release that environment `env_id` requires, or a message explaining
    /// why it could not be determined.
    ///
    /// In offline mode this is the newest release this binary knows about. Otherwise it is the
    /// release for the protocol version reported by `env_id`'s chain, clamped down to the newest
    /// release this binary ships if the chain is ahead of it (with a note to the user, since that
    /// ceiling is the one they can lift by upgrading the CLI).
    async fn framework_release(
        &self,
        env_id: &EnvironmentID,
    ) -> Result<&'static SystemPackagesVersion, String> {
        let Some(envs) = &self.envs else {
            return Ok(latest_system_packages());
        };

        let version =
            Self::resolve_protocol_version(envs, self.active_env.as_deref(), env_id).await?;

        if version > ProtocolVersion::MAX {
            user_note!(
                "The chain `{env_id}` is running protocol version {}, but this CLI only knows \
                 about framework versions up to protocol {}. Pinning the newest framework it \
                 knows; upgrade the CLI to use newer framework APIs.",
                version.as_u64(),
                ProtocolVersion::MAX.as_u64(),
            );
        }

        let (packages, _) = system_packages_for_protocol(version).map_err(|e| {
            format!(
                "the chain `{env_id}` is running protocol version {}, which is older than any \
                 framework version known to this CLI ({e}). Use a CLI release contemporary with \
                 that chain.",
                version.as_u64(),
            )
        })?;
        Ok(packages)
    }

    /// Return the protocol version reported by an endpoint that verifiably serves the chain
    /// `env_id`, or a message naming every endpoint that was tried and why each one failed.
    ///
    /// Individual failures are not reported as they happen: trying several endpoints is the normal
    /// path, so an endpoint that doesn't answer is only worth mentioning if none of them did.
    async fn resolve_protocol_version(
        envs: &[SuiEnv],
        active_env: Option<&str>,
        env_id: &EnvironmentID,
    ) -> Result<ProtocolVersion, String> {
        let candidates = Self::candidate_envs(envs, active_env, env_id);
        if candidates.is_empty() {
            return Err(format!(
                "none of your configured environments serve the chain `{env_id}`. Add one with \
                 `sui client new-env --alias <name> --rpc <url>`."
            ));
        }

        let mut failures = Vec::new();
        for env in candidates {
            let query = Self::query_protocol_version(&env, env_id);
            match tokio::time::timeout(ENDPOINT_TIMEOUT, query).await {
                Ok(Ok(version)) => return Ok(version),
                Ok(Err(e)) => failures.push(format!("`{}` ({}): {e:#}", env.alias, env.rpc)),
                Err(_) => failures.push(format!(
                    "`{}` ({}): timed out after {}s",
                    env.alias,
                    env.rpc,
                    ENDPOINT_TIMEOUT.as_secs()
                )),
            }
        }

        Err(format!(
            "could not reach an RPC endpoint serving the chain `{env_id}`. Tried:\n{}",
            failures
                .iter()
                .map(|f| format!("  - {f}"))
                .collect::<Vec<_>>()
                .join("\n"),
        ))
    }

    /// The endpoints that may serve the chain `env_id`, in the order they should be tried:
    /// configured environments whose cached chain ID matches, then configured environments whose
    /// chain has never been cached (their identity is verified when they are queried), and finally
    /// the default public endpoints of the well-known networks. Within each group the active
    /// environment comes first. Environments cached as a *different* chain are never candidates.
    fn candidate_envs(
        envs: &[SuiEnv],
        active_env: Option<&str>,
        env_id: &EnvironmentID,
    ) -> Vec<SuiEnv> {
        let serves_chain = |env: &SuiEnv| {
            env.chain_id
                .as_deref()
                .is_some_and(|id| chain_ids_match(id, env_id))
        };

        let mut ordered: Vec<&SuiEnv> = envs.iter().collect();
        ordered.sort_by_key(|env| Some(env.alias.as_str()) != active_env);

        let cached_matches = ordered.iter().filter(|env| serves_chain(env));
        let unknown_identity = ordered.iter().filter(|env| env.chain_id.is_none());

        let mut candidates: Vec<SuiEnv> = cached_matches
            .chain(unknown_identity)
            .map(|env| (*env).clone())
            .collect();

        for default_env in [SuiEnv::testnet(), SuiEnv::mainnet()] {
            if serves_chain(&default_env)
                && !candidates.iter().any(|env| env.rpc == default_env.rpc)
            {
                candidates.push(default_env);
            }
        }

        candidates
    }

    /// Query the protocol version from `env`'s endpoint, first verifying that the endpoint really
    /// serves the chain `env_id`. The check is required rather than advisory: a cached chain ID can
    /// be stale, and an answer from the wrong chain bounds nothing.
    async fn query_protocol_version(
        env: &SuiEnv,
        env_id: &EnvironmentID,
    ) -> anyhow::Result<ProtocolVersion> {
        let client = env.create_grpc_client()?;

        let chain_id = client
            .get_chain_identifier()
            .await
            .context("Failed to query chain identifier")?;
        anyhow::ensure!(
            chain_ids_match(&chain_id.to_string(), env_id),
            "endpoint serves chain `{chain_id}`, not `{env_id}`"
        );

        let config = client
            .get_protocol_config(None)
            .await
            .context("Failed to query protocol config")?;
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

    fn environment_ids_match(&self, a: &EnvironmentID, b: &EnvironmentID) -> bool {
        chain_ids_match(a, b)
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

    fn system_dep_names(&self) -> BTreeSet<SystemDepName> {
        Self::system_deps_by_name()
            .iter()
            .filter(|(package_name, _)| {
                !UNSUPPORTED_SYSTEM_PACKAGES.contains(&package_name.as_str())
            })
            .map(|(_, dep_name)| dep_name.clone())
            .collect()
    }

    /// Pin the framework packages to the git revision of the release that `environment`'s chain is
    /// running.
    ///
    /// Writing `need` for the oldest framework that compiles the developer's code, `network` for
    /// the version running on the target chain, and `CLI` for the newest framework this binary
    /// ships, the pin must satisfy
    ///
    /// ```text
    /// need <= pinned <= network     (correctness)
    ///         pinned <= CLI         (feasibility — we can only name revisions we ship)
    /// ```
    ///
    /// `need` is not observable here; we learn it later, when a call fails to compile. So the job
    /// is to make the pin as large as the upper bounds allow, and the developer's only lever on
    /// the lower bound is re-running `update-deps` — which is why a failure to advance has to be
    /// explained rather than swallowed.
    ///
    /// The two bounds fail very differently, and that asymmetry drives every decision here. Too
    /// old is a compile error on the developer's own machine, at the moment they try, with a fix
    /// they can perform themselves. Too new compiles fine and then fails at publish with a linker
    /// error against `0x2` — late, remote, cryptic, and (because the pin is committed to
    /// `Move.lock`) inflicted on everyone who builds the package. So when the version is unknown
    /// we fail rather than guess: guessing upward is unsafe, and guessing the top of the table is
    /// the maximally-unsafe guess, since its premise — that the chain is at or beyond this
    /// binary's ceiling — is systematically false for mainnet.
    ///
    /// Note that a stale pin is safe on its own: protocol versions only rise, so a pin that
    /// satisfied `pinned <= network` when it was made still does. Staleness can only push a pin
    /// toward "too old", never toward "too new".
    async fn system_deps(
        &self,
        environment: &EnvironmentID,
    ) -> Result<BTreeMap<SystemDepName, LockfileDependencyInfo>, String> {
        let packages = self.framework_release(environment).await?;
        let sha = &packages.git_revision;

        let names = Self::system_deps_by_name();
        let deps = packages
            .packages
            .iter()
            .filter(|package| !UNSUPPORTED_SYSTEM_PACKAGES.contains(&package.package_name.as_str()))
            .map(|package| {
                let info = LockfileDependencyInfo::Git(LockfileGitDepInfo {
                    repo: SYSTEM_GIT_REPO.to_string(),
                    path: PathBuf::from(&package.repo_path),
                    rev: GitSha::try_from(sha.clone()).expect("manifest has valid sha"),
                });
                let name = names
                    .get(&package.package_name)
                    .expect("package exists in the renaming table")
                    .clone();
                (name, info)
            })
            .collect();

        Ok(deps)
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
                "Dependency `{name}` is a legacy system name and cannot be used. See https://docs.sui.io/guides/developer/packages/move-package-management#system-dependencies"
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

#[cfg(test)]
mod tests {
    use super::*;

    fn env(alias: &str, rpc: &str, chain_id: Option<&str>) -> SuiEnv {
        SuiEnv {
            alias: alias.to_string(),
            rpc: rpc.to_string(),
            ws: None,
            basic_auth: None,
            chain_id: chain_id.map(|id| id.to_string()),
        }
    }

    fn aliases(candidates: &[SuiEnv]) -> Vec<&str> {
        candidates.iter().map(|env| env.alias.as_str()).collect()
    }

    #[test]
    fn active_env_first_and_other_chains_excluded() {
        let testnet_id = testnet_environment().id;
        let mainnet_id = mainnet_environment().id;
        let envs = [
            env("testnet-alt", "http://alt", Some(&testnet_id)),
            env("mainnet", "http://mainnet", Some(&mainnet_id)),
            env("testnet", "http://testnet", Some(&testnet_id)),
        ];

        let candidates = SuiFlavor::candidate_envs(&envs, Some("testnet"), &testnet_id);
        // active env first, then config order; the mainnet env is not a candidate; the default
        // public testnet endpoint is the last resort
        assert_eq!(aliases(&candidates), ["testnet", "testnet-alt", "testnet"]);
        assert_eq!(candidates[2].rpc, SuiEnv::testnet().rpc);
    }

    #[test]
    fn unknown_identity_envs_follow_cached_matches() {
        let testnet_id = testnet_environment().id;
        let envs = [
            env("uncached", "http://uncached", None),
            env("testnet", "http://testnet", Some(&testnet_id)),
        ];

        let candidates = SuiFlavor::candidate_envs(&envs, Some("uncached"), &testnet_id);
        // a cached match beats the active-but-unknown-identity env
        assert_eq!(
            aliases(&candidates),
            ["testnet", "uncached", SuiEnv::testnet().alias.as_str()]
        );
    }

    #[test]
    fn default_public_endpoint_deduplicated_by_url() {
        let mainnet_id = mainnet_environment().id;
        let default_mainnet = SuiEnv::mainnet();
        let envs = [env("my-mainnet", &default_mainnet.rpc, Some(&mainnet_id))];

        let candidates = SuiFlavor::candidate_envs(&envs, None, &mainnet_id);
        assert_eq!(aliases(&candidates), ["my-mainnet"]);
    }

    #[test]
    fn custom_chain_matches_only_cached_or_unknown_envs() {
        let envs = [
            env("localnet", "http://localhost:9000", Some("6674f21e")),
            env("mainnet", "http://mainnet", Some(&mainnet_environment().id)),
            env("uncached", "http://uncached", None),
        ];

        let candidates = SuiFlavor::candidate_envs(&envs, None, &"6674f21e".to_string());
        // no default public endpoint serves this chain
        assert_eq!(aliases(&candidates), ["localnet", "uncached"]);
    }

    #[test]
    fn no_candidates_for_unknown_chain_with_no_matching_envs() {
        let envs = [env(
            "mainnet",
            "http://mainnet",
            Some(&mainnet_environment().id),
        )];

        let candidates = SuiFlavor::candidate_envs(&envs, None, &"deadbeef".to_string());
        assert!(candidates.is_empty());
    }
}
