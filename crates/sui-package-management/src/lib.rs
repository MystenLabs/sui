// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, Context};
use std::path::PathBuf;

use move_package::lock_file::{self, LockFile};
use sui_json_rpc_types::{get_new_package_obj_from_response, SuiTransactionBlockResponse};
use sui_sdk::wallet_context::WalletContext;

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
