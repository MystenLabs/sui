// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

use tracing::debug;

use anyhow::anyhow;

use move_core_types::account_address::AccountAddress;

use crate::{
    cli::{
        compile_package, dry_run_or_execute_or_serialize, get_package_digest,
        update_lock_file_for_chain_env,
    },
    sui_flavor::SuiMetadata,
    SuiFlavor,
};
use clap::{Command, Parser, Subcommand};
use move_package_alt::errors::{PackageError, PackageResult};

use move_package_compiling::{
    build_config::BuildConfig, compiled_package::compile, lint_flag::LintFlag,
};
use shared_crypto::intent::Intent;
use sui_config::{sui_config_dir, SUI_CLIENT_CONFIG};
use sui_sdk::{
    rpc_types::SuiExecutionStatus,
    types::{
        base_types::{ObjectID, SuiAddress},
        move_package::{MovePackage, UpgradeCap},
        transaction::{
            InputObjectKind, SenderSignedData, Transaction, TransactionData, TransactionKind,
        },
    },
    wallet_context::WalletContext,
};

use sui_json_rpc_types::{
    get_new_package_obj_from_response, get_new_package_upgrade_cap_from_response, SuiData,
    SuiObjectDataOptions, SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponse,
};
use sui_keys::keystore::AccountKeystore;
use sui_package_management::LockCommand;

/// Build the package
#[derive(Debug, Clone, Parser)]
pub struct Upgrade {
    /// Path to the project
    #[arg(name = "path", short = 'p', long = "path", default_value = ".")]
    path: Option<PathBuf>,

    #[arg(name = "env", short = 'e', long = "environment")]
    env: Option<String>,

    #[arg(name = "cap", short = 'c', long = "upgrade-cap")]
    cap: ObjectID,

    #[command(flatten)]
    build_config: BuildConfig,
}

impl Upgrade {
    pub async fn execute(&self, binary_version: &str) -> PackageResult<()> {
        let config_path = sui_config_dir()?.join(SUI_CLIENT_CONFIG);
        let mut context = WalletContext::new(&config_path)?;
        let sender = context.active_address()?;
        let client = context.get_client().await?;
        let read_api = client.read_api();
        let chain_id = read_api
            .get_chain_identifier()
            .await
            .map_err(|_| anyhow!("Cannot find the chain identifier, thus cannot publish"))?;

        let build_config = self.build_config.clone();
        let set_env = context.get_active_env()?.alias.clone();
        let env = &self.env.clone().unwrap_or(set_env.to_string());
        let path = self.path.clone().unwrap_or_else(|| PathBuf::from("."));
        println!("Publishing package to environment: {}", env);

        let (root_pkg, compiled_package, mut lockfile, lockfile_path) =
            compile_package(path, env, &build_config, &chain_id).await?;
        let compiled_modules = compiled_package.get_package_bytes();
        let dep_ids: Vec<ObjectID> = compiled_package
            .dependency_ids()
            .into_iter()
            .map(|x| x.0.into())
            .collect();
        let digest = get_package_digest(&compiled_modules, dep_ids.iter().collect());

        let resp = read_api
            .get_object_with_options(
                self.cap,
                SuiObjectDataOptions::default().with_bcs().with_owner(),
            )
            .await
            .unwrap();

        let Some(data) = resp.data else {
            return Err(anyhow!("Could not find upgrade capability at {}", self.cap).into());
        };

        let upgrade_cap: UpgradeCap = data
            .bcs
            .ok_or_else(|| anyhow!("Fetch upgrade capability object but no data was returned"))?
            .try_as_move()
            .ok_or_else(|| anyhow!("Upgrade capability is not a Move Object"))?
            .deserialize()?;
        // We keep the existing policy -- no fancy policies or changing the upgrade
        // policy at the moment. To change the policy you can call a Move function in the
        // `package` module to change this policy.
        let upgrade_policy = upgrade_cap.policy;

        debug!("Compiled modules {:?}", compiled_modules);
        debug!("Dependency IDs {:?}", dep_ids);
        println!("Package compiled successfully.");

        let published_data = root_pkg.root_pkg().publish_data(env)?;
        let package_id: ObjectID = published_data.publication.published_at.0.into();

        // create the publish tx kind
        let tx_kind = client
            .transaction_builder()
            .upgrade_tx_kind(
                package_id,
                compiled_modules,
                dep_ids,
                self.cap,
                upgrade_policy,
                digest.to_vec(),
            )
            .await?;

        let result = dry_run_or_execute_or_serialize(tx_kind, &mut context).await?;

        // update the lockfile with the published package information for this environment
        update_lock_file_for_chain_env(
            &mut lockfile,
            lockfile_path,
            &chain_id.to_string(),
            env,
            LockCommand::Upgrade,
            &result,
            binary_version,
            &build_config,
        )
        .await?;

        Ok(())
    }
}
