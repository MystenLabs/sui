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

#[derive(Debug, Clone)]
pub enum PublishedAtError {
    Invalid(String),
    Conflict(String, String),
    NotPresent,
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

/// Find the published on-chain ID in the `Move.lock` or `Move.toml` file.
/// A chain ID of `None` means that we will only try to resolve a published ID from the Move.toml.
/// The published ID is resolved from the `Move.toml` if the Move.lock does not exist.
/// Else, we resolve from the `Move.lock`, where addresses are automatically
/// managed. If conflicting IDs are found in the `Move.lock` vs. `Move.toml`, a
/// "Conflict" error message returns.
pub fn resolve_published_id(
    package: &Package,
    chain_id: Option<String>,
) -> Result<ObjectID, PublishedAtError> {
    // Look up a valid `published-at` in the `Move.toml` first, which we'll
    // return if the Move.lock does not manage addresses.
    let published_id_in_manifest = match published_at_property(package) {
        Ok(v) => Some(v),
        Err(PublishedAtError::NotPresent) => None,
        Err(e) => return Err(e), // An existing but invalid `published-at` in `Move.toml` should fail early.
    };

    let lock = package.package_path.join(SourcePackageLayout::Lock.path());
    let mut lock_file = match std::fs::File::open(lock.clone()) {
        Ok(f) => f,
        Err(_) => match published_id_in_manifest {
            Some(v) => {
                return ObjectID::from_str(v.as_str())
                    .map_err(|_| PublishedAtError::Invalid(v.to_owned()))
            }
            None => return Err(PublishedAtError::NotPresent),
        },
    };
    let managed_packages = ManagedPackage::read(&mut lock_file).ok();
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
        .map(|(k, v)| (k, v.latest_published_id));

    let package_id = match (env_for_chain_id, published_id_in_manifest) {
        (Some((_env, id_lock)), Some(id_manifest)) if id_lock != id_manifest => {
            return Err(PublishedAtError::Conflict(id_lock, id_manifest))
        }
        (Some((_, id_lock)), _) => id_lock,
        (None, Some(id_manifest)) => id_manifest, /* No info in Move.lock: Fall back to manifest */
        _ => return Err(PublishedAtError::NotPresent), /* Neither in Move.toml nor Move.lock */
    };
    ObjectID::from_str(package_id.as_str())
        .map_err(|_| PublishedAtError::Invalid(package_id.to_owned()))
}

fn published_at_property(package: &Package) -> Result<String, PublishedAtError> {
    let Some(value) = package
        .source_package
        .package
        .custom_properties
        .get(&Symbol::from(PUBLISHED_AT_MANIFEST_FIELD))
    else {
        return Err(PublishedAtError::NotPresent);
    };
    Ok(value.to_string())
}
