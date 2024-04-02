// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, bail};
use std::path::PathBuf;

use move_package::lock_file::{self, LockFile};
use sui_json_rpc_types::{get_new_package_obj_from_response, SuiTransactionBlockResponse};
use sui_sdk::wallet_context::WalletContext;

pub enum LockCommand {
    Publish,
    Upgrade,
}

pub async fn update_lock_file(
    context: &WalletContext,
    command: LockCommand,
    install_dir: Option<PathBuf>,
    lock_file: Option<PathBuf>,
    response: &SuiTransactionBlockResponse,
) -> Result<(), anyhow::Error> {
    let chain_identifier = context
        .get_client()
        .await?
        .read_api()
        .get_chain_identifier()
        .await?;

    let (original_id, version, _) = get_new_package_obj_from_response(response)
        .ok_or_else(|| anyhow!("No package object response"))?;

    let (install_dir, lock_file) = match (install_dir, lock_file) {
        (Some(install_dir), Some(lock_file)) => (install_dir, lock_file),
        (None, Some(lock_file)) => (PathBuf::from("."), lock_file),
        (Some(_), None) => bail!("No lock file exists."),
        // We need an install directory to have a working space for updating the lock file.
        _ => bail!("Could not resolve install directory of move package."),
    };

    let env = context
        .config
        .get_active_env()
        .map_err(|e| anyhow!("Issue resolving environment: {e}"))?;

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
    lock.commit(lock_file.clone())?;
    Ok(())
}
