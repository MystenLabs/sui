// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, Context};
use std::path::PathBuf;
use std::str::FromStr;

use move_package::{
    lock_file::{self, schema::ManagedPackage, LockFile},
    resolution::resolution_graph::Package,
    source_package::layout::SourcePackageLayout,
};
use move_symbol_pool::Symbol;
use sui_json_rpc_types::{get_new_package_obj_from_response, SuiTransactionBlockResponse};
use sui_sdk::{types::base_types::ObjectID, wallet_context::WalletContext};

const PUBLISHED_AT_MANIFEST_FIELD: &str = "published-at";

pub enum LockCommand {
    Publish,
    Upgrade,
}

/// Update the `Move.lock` file with automated address management info.
/// Expects a wallet context, the publish or upgrade command, its response.
/// The `Move.lock` principally file records the published address (i.e., package ID) of
/// a package under an environment determined by the wallet context config. See the
/// `ManagedPackage` type in the lock file for a complete spec.
pub async fn update_lock_file(
    context: &WalletContext,
    command: LockCommand,
    install_dir: Option<PathBuf>,
    lock_file: Option<PathBuf>,
    response: &SuiTransactionBlockResponse,
) -> Result<(), anyhow::Error> {
    let chain_identifier = context
        .get_client()
        .await
        .context("Network issue: couldn't use client to connect to chain when updating Move.lock")?
        .read_api()
        .get_chain_identifier()
        .await
        .context("Network issue: couldn't determine chain identifier for updating Move.lock")?;

    let (original_id, version, _) = get_new_package_obj_from_response(response).context(
        "Expected a valid published package response but didn't see \
         one when attempting to update the `Move.lock`.",
    )?;
    let Some(lock_file) = lock_file else {
        bail!(
            "Expected a `Move.lock` file to exist after publishing \
             package, but none found. Consider running `sui move build` to \
             generate the `Move.lock` file in the package directory."
        )
    };
    let install_dir = install_dir.unwrap_or(PathBuf::from("."));
    let env = context.config.get_active_env().context(
        "Could not resolve environment from active wallet context. \
         Try ensure `sui client active-env` is valid.",
    )?;

    let mut lock = LockFile::from(install_dir.clone(), &lock_file)?;
    match command {
        LockCommand::Publish => lock_file::schema::update_managed_address(
            &mut lock,
            &env.alias,
            lock_file::schema::ManagedAddressUpdate::Published {
                chain_id: chain_identifier,
                original_id: original_id.to_string(),
            },
        ),
        LockCommand::Upgrade => lock_file::schema::update_managed_address(
            &mut lock,
            &env.alias,
            lock_file::schema::ManagedAddressUpdate::Upgraded {
                latest_id: original_id.to_string(),
                version: version.into(),
            },
        ),
    }?;
    lock.commit(lock_file)?;
    Ok(())
}

/// Find the published on-chain ID in the `Move.lock` or
/// `Move.toml` file for the current environmnent. Resolving from the
/// `Move.lock` takes precedence, where addresses are automatically managed. The
/// `Move.toml` is inspected as a fallback. If conflicting IDs are found in the
/// `Move.lock` vs. `Move.toml`, an error message recommends actions to the
/// user to resolve these.
pub fn resolve_published_id(
    package: &Package,
    chain_id: Option<String>,
    env_alias: Option<String>,
) -> Result<ObjectID, anyhow::Error> {
    let lock = package.package_path.join(SourcePackageLayout::Lock.path());
    let mut lock_file =
        std::fs::File::open(lock.clone()).context("Could not read lock file at {lock}.")?;
    let managed_packages = ManagedPackage::read(&mut lock_file).ok(); // Warn on not successful

    // Find the environment and ManagedPackage data for this chain_id.
    let env_for_chain_id = managed_packages
        .and_then(|m| {
            m.into_iter().find(|(_, v)| {
                if let Some(chain_id) = &chain_id {
                    v.chain_id == *chain_id
                } else {
                    false
                }
            })
        })
        .map(|(k, v)| (k, v.original_published_id));

    // Look up a valid `published-at` in the `Move.toml`.
    let published_id_in_manifest = package
        .source_package
        .package
        .custom_properties
        .get(&Symbol::from(PUBLISHED_AT_MANIFEST_FIELD))
        .map(|s| s.clone());

    let package_id = match (env_for_chain_id, published_id_in_manifest) {
        (Some((env, id_lock)), Some(id_manifest)) if id_lock != id_manifest => bail!(
            "Published ID in lock {id_lock} does not match `published-id` in manifest {id_manifest}. \
             This means the package was published in your environment {env} to a different address \
             than the one placed in the `published-id` field of your `Move.toml`. To resolve, consider:

             - Moving your `published-id` in the `Move.toml` into the `Move.lock`, where it will be \
             automatically tracked for a chain like `mainnet` or `testnet`. \
             To do so, run this command: `FIXME`; OR

             - Manually remove the `published-id` in your `Move.toml` if it is no longer relevant; OR

             - If `published-id` is still relevant and you do not want to move it to `Move.lock`, \
             you can switch to an environment where the `published-at` address in your `Move.toml` \
             corresponds to the environment where the package is published. \
             To do so, run this command: `FIXME`."
        ),
        (Some((_, id_lock)), _) => id_lock,
        (None, Some(id_manifest)) => {
	    let env = env_alias.unwrap_or_else(|| "unknown".into());
            eprintln!(
                "Resolving published ID from manifest since there is no managed package for the \
                 current environment {env} in `Move.lock` ({}). Consider tracking your published package \
                 in the `Move.lock` by running: `FIXME`.",
                lock.display()
            );
            id_manifest
        }
        _ => bail!("No published ID for package found in the `Move.toml` or `Move.lock`."),
    };
    ObjectID::from_str(package_id.as_str()).context("Could not convert string {package_id} to ID")
}
