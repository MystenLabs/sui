// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::Path};

use anyhow::bail;
use indexmap::IndexMap;
use move_package_alt::{
    package::RootPackage,
    schema::{Environment, EnvironmentID, EnvironmentName},
};
use sui_sdk::{SuiClient, wallet_context::WalletContext};

use crate::SuiFlavor;

/// Binds together the context for `find_environment` for helper functions
struct EnvFinder<'a> {
    package_path: &'a Path,
    explicit_env: Option<EnvironmentName>,
    wallet: &'a WalletContext,
    manifest_envs: IndexMap<EnvironmentName, EnvironmentID>,
}

/// Determine the correct environment to use for the package system based on
///  - the path to a directory containing a Move.toml file
///  - the `-e <env>` argument that was passed, if any
///  - the CLI's active environment (`wallet`)
pub async fn find_environment(
    package_path: &Path,
    explicit_env: Option<EnvironmentName>,
    wallet: &WalletContext,
) -> anyhow::Result<Environment> {
    let mut manifest_envs = RootPackage::<SuiFlavor>::environments(package_path)?;
    let finder = EnvFinder {
        package_path,
        explicit_env,
        wallet,
        manifest_envs,
    };

    // use explicit environment if provided
    if let Some(explicit) = finder.get_explicit()? {
        return Ok(explicit);
    }

    // figure out an active environment to use
    let active_env = finder.active_environment().await?;

    // if the manifest has exactly that (name, chain ID) pair, use it
    if let Some(exact) = finder.check_exact(&active_env)? {
        return Ok(exact);
    }

    // if the manifest has exactly one entry with matching chain ID, use it
    finder.get_unique_by_chain_id(active_env)
}

impl EnvFinder<'_> {
    /// Return the explicitly passed environment if it exists. Fails if an environment was passed
    /// explicitly but doesn't exist in the manifest.
    fn get_explicit(&self) -> anyhow::Result<Option<Environment>> {
        let Some(ref env_name) = self.explicit_env else {
            return Ok(None);
        };

        let Some(env_id) = self.manifest_envs.get(env_name) else {
            bail!("Environment `{env_name}` is not present in Move.toml");
        };

        Ok(Some(Environment::new(env_name.clone(), env_id.clone())))
    }

    /// Find the active environment. Checks the cache first and fails if the chain ID cannot be
    /// determined (either from the cache or from the network or the manifest)
    async fn active_environment(&self) -> anyhow::Result<Environment> {
        let mut active_env = self.wallet.get_active_env()?.clone();
        let chain_id = if let Some(chain_id) = active_env.chain_id {
            // cached
            chain_id
        } else if let Ok(client) = self.wallet.get_client().await
            && let Ok(chain_id) = self.wallet.cache_chain_id(&client).await
        {
            // fetched
            chain_id
        } else if let Some(chain_id) = self.manifest_envs.get(&active_env.alias) {
            // couldn't fetch but an environment with same name is in the manifest
            chain_id.clone()
        } else {
            let mut s = String::new();
            for e in self.manifest_envs.keys() {
                s.push_str(&format!("\n\t-e {e}"))
            }
            bail!(
                "Active environment `{}` does not correspond to any of environments defined for \
                the package. Specify the environment by passing one of the following:{s}",
                active_env.alias
            );
        };

        Ok(Environment::new(active_env.alias, chain_id))
    }

    /// Check that the exact environment `active_env` is present in the manifest; returns an error
    /// if the name is present but the chain ID differs; returns `None` if it's not present
    fn check_exact(&self, active_env: &Environment) -> anyhow::Result<Option<Environment>> {
        let Some(manifest_id) = self.manifest_envs.get(active_env.name()) else {
            return Ok(None);
        };

        if manifest_id != active_env.id() {
            bail!(
                "Error: Environment `{active_env}` has chain ID `{chain_id}` in your CLI \
                environment, but `Move.toml` expects `{active_env}` to have chain ID \
                `{env_chain_id}`; this may indicate that `{active_env}` has been wiped or that you \
                have a misconfigured CLI environment. If you want to ignore your CLI's chain ID and build for the `{active_env}` environment, you can pass `-e {active_env}`.",
                active_env = active_env.name,
                env_chain_id = manifest_id,
                chain_id = active_env.id,
            );
        }

        Ok(Some(active_env.clone()))
    }

    /// Check that there is exactly one entry of the manifest that matches the provided `chain_id`;
    /// fails if there are either too many or too few
    fn get_unique_by_chain_id(&self, env: Environment) -> anyhow::Result<Environment> {
        let Environment {
            name: active_env,
            id: chain_id,
        } = env;
        let candidates: BTreeMap<&EnvironmentName, &EnvironmentID> = self
            .manifest_envs
            .iter()
            .filter(|(k, v)| v == &&chain_id)
            .collect();

        if candidates.is_empty() {
            // ephemeral case, no environment found with that name, we error
            bail!(
                "Your active environment `{active_env}` is not present in `Move.toml`, so you cannot \
                publish to `{active_env}`.

            - If you want to create a temporary publication on `{active_env}` and record the addresses \
               in an ephemeral file, use the `test-publish` command instead.

                sui client test-publish --help

            - If you want to publish to `{active_env}` and record the addresses in the shared \
            `Publications.toml` file, you will need to add the following to `Move.toml`:

                [environments]
                {active_env} = \"{chain_id}\""
            );
        }

        if candidates.len() > 1 {
            let mut s = String::new();
            for e in candidates.keys() {
                s.push_str(&format!("\n\t--build-env {e}"))
            }
            bail!(
                "Found multiple environments defined in Move.toml with chain-id `{chain_id}`. Please pass one of the following:{s}"
            );
        }

        let (env_name, env_id) = candidates
            .first_key_value()
            .expect("candidates has length 1");

        eprintln!(
            "Note: `Move.toml` does not define an `{active_env}` environment; building for `{env_name}` instead"
        );

        Ok(Environment::new(env_name.to_string(), env_id.to_string()))
    }
}
