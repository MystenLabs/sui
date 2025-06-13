// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod build;
pub mod publish;
pub mod upgrade;

use anyhow::{anyhow, bail};

use crate::{sui_flavor::SuiMetadata, SuiFlavor};
pub use build::Build;
use move_package_alt::{
    errors::PackageError,
    flavor::MoveFlavor,
    package::RootPackage,
    schema::{OriginalID, ParsedLockfile, Publication, PublishedID},
};

use std::io::Write;

pub use move_package_compiling::build_config::BuildConfig;
pub use move_package_compiling::compiled_package::compile;
pub use move_package_compiling::compiled_package::CompiledPackage;

pub use publish::Publish;
use shared_crypto::intent::Intent;
use std::{collections::BTreeMap, path::PathBuf};
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_json_rpc_types::{
    get_new_package_obj_from_response, get_new_package_upgrade_cap_from_response,
    SuiExecutionStatus, SuiTransactionBlockResponse,
};
use sui_keys::keystore::AccountKeystore;
use sui_package_management::LockCommand;
use sui_sdk::{
    types::{
        base_types::ObjectID,
        move_package::MovePackage,
        transaction::{
            InputObjectKind, SenderSignedData, Transaction, TransactionData, TransactionKind,
        },
    },
    wallet_context::WalletContext,
};
use tracing::debug;
pub use upgrade::Upgrade;

pub(crate) async fn dry_run_or_execute_or_serialize(
    tx_kind: TransactionKind,
    context: &mut WalletContext,
) -> Result<SuiTransactionBlockResponse, anyhow::Error> {
    let gas_price = context.get_reference_gas_price().await?;
    let signer = context.active_address()?;

    let client = context.get_client().await?;

    let gas_budget = 50000000;

    let gas_payment = {
        let input_objects: Vec<_> = tx_kind
            .input_objects()?
            .iter()
            .filter_map(|o| match o {
                InputObjectKind::ImmOrOwnedMoveObject((id, _, _)) => Some(*id),
                _ => None,
            })
            .collect();

        let gas_payment = client
            .transaction_builder()
            .select_gas(signer, None, gas_budget, input_objects, gas_price)
            .await?;

        vec![gas_payment]
    };

    debug!("Preparing transaction data");
    let tx_data = TransactionData::new_with_gas_coins_allow_sponsor(
        tx_kind,
        signer,
        gas_payment,
        gas_budget,
        gas_price,
        signer,
    );
    debug!("Finished preparing transaction data");

    let mut signatures = vec![context
        .config
        .keystore
        .sign_secure(&signer, &tx_data, Intent::sui_transaction())?
        .into()];

    let sender_signed_data = SenderSignedData::new(tx_data, signatures);
    let transaction = Transaction::new(sender_signed_data);
    debug!("Executing transaction: {:?}", transaction);
    let mut response = context
        .execute_transaction_may_fail(transaction.clone())
        .await?;
    debug!("Transaction executed: {:?}", transaction);

    let effects = response
        .effects
        .as_ref()
        .ok_or_else(|| anyhow!("Effects from SuiTransactionBlockResult should not be empty"))?;

    let effects_status = effects.clone().into_status();
    if let SuiExecutionStatus::Failure { error } = effects_status {
        return Err(anyhow!(
            "Error executing transaction '{}': {error}",
            response.digest
        ));
    }

    println!(
        "Transaction executed successfully. Digest: {}",
        response.digest
    );

    Ok(response)
}

pub async fn update_lock_file_for_chain_env(
    lockfile: &mut ParsedLockfile<SuiFlavor>,
    lockfile_path: PathBuf,
    chain_id: &str,
    env: &str,
    command: LockCommand,
    response: &SuiTransactionBlockResponse,
    binary_version: &str,
    build_config: &BuildConfig,
) -> Result<(), anyhow::Error> {
    // Get the published package ID and version from the response
    let (published_id, version, _) =
        get_new_package_obj_from_response(response).ok_or_else(|| {
            anyhow!(
                "Expected a valid published package response but didn't see \
         one when attempting to update the `Move.lock`."
            )
        })?;

    match command {
        LockCommand::Publish => {
            let (upgrade_cap, _, _) = get_new_package_upgrade_cap_from_response(response)
                .ok_or_else(|| anyhow!("Expected a valid published package with a upgrade cap"))?;
            let publication_data = Publication::<SuiFlavor> {
                published_at: PublishedID(*published_id),
                original_id: OriginalID(*published_id),
                chain_id: chain_id.to_string(),
                toolchain_version: binary_version.to_string(),
                build_config: toml::from_str(&toml::to_string(build_config)?)?,
                metadata: SuiMetadata {
                    upgrade_cap: Some((*upgrade_cap).into()),
                    version: Some(version.value()),
                },
            };

            lockfile.published.insert(env.to_string(), publication_data);
        }
        LockCommand::Upgrade => {
            if let Some(pub_data) = lockfile.published.get_mut(env) {
                pub_data.published_at = PublishedID(*published_id);
                pub_data.metadata.version = Some(version.value());
            };
        }
    }

    let lockfile_str = lockfile.render_as_toml();

    std::fs::write(&lockfile_path, lockfile_str).map_err(|e| {
        anyhow!(
            "Failed to write lockfile at {}: {}",
            lockfile_path.display(),
            e
        )
    })?;

    Ok(())
}

/// Return a digest of the bytecode modules in this package.
pub fn get_package_digest(compiled_modules: &Vec<Vec<u8>>, object_ids: Vec<&ObjectID>) -> [u8; 32] {
    let hash_modules = true;
    MovePackage::compute_digest_for_modules_and_deps(compiled_modules, object_ids, hash_modules)
}

async fn compile_package(
    path: PathBuf,
    env: &String,
    build_config: &BuildConfig,
    chain_id: &str,
) -> Result<
    (
        RootPackage<SuiFlavor>,
        CompiledPackage,
        ParsedLockfile<SuiFlavor>,
        PathBuf,
    ),
    anyhow::Error,
> {
    let root_pkg = RootPackage::<SuiFlavor>::load(path.clone(), Some(env.clone())).await?;

    // check if the chain id matches the chian id in the env
    let envs = root_pkg.environments();
    let manifest_env_chain_id = envs.get(env);
    let cli_chain_id = Some(chain_id.to_owned());

    if manifest_env_chain_id != cli_chain_id.as_ref() {
        bail!("The chain id in the environment '{}' does not match the chain id of the connected network. Please check your Move.toml and ensure that the chain id matches the connected network.", env)
    }

    let compiled_package = move_package_compiling::compile_from_root_package::<SuiFlavor>(
        root_pkg,
        build_config,
        &mut std::io::stdout(),
    )
    .await?;

    root_pkg
        .update_deps_and_write_to_lockfile(&BTreeMap::from([(env.clone(), chain_id.to_owned())]))
        .await;

    let mut lockfile = root_pkg.load_lockfile().map_err(|e| {
        anyhow!(
            "Failed to load lockfile for package at {}\n: {e}",
            root_pkg.package_path().path().display()
        )
    })?;

    let lockfile_path = root_pkg.package_path().lockfile_path();

    // compile package
    Ok((root_pkg, compiled_package, lockfile, lockfile_path))
}
