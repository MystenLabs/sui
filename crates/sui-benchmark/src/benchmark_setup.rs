// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::bank::BenchmarkBank;
use crate::options::Opts;
use crate::util::get_ed25519_keypair_from_keystore;
use crate::{FullNodeProxy, LocalValidatorAggregatorProxy, ValidatorProxy};
use anyhow::{anyhow, bail, Context, Result};
use prometheus::Registry;
use rand::seq::SliceRandom;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;
use sui_types::base_types::ObjectID;
use sui_types::object::Owner;
use tokio::runtime::Builder;
use tokio::sync::{oneshot, Barrier};
use tracing::info;

pub struct BenchmarkSetup {
    pub server_handle: JoinHandle<()>,
    pub shutdown_notifier: oneshot::Sender<()>,
    pub bank: BenchmarkBank,
    pub proxies: Vec<Arc<dyn ValidatorProxy + Send + Sync>>,
}

impl BenchmarkSetup {
    pub async fn new(
        barrier: Arc<Barrier>,
        registry: &Registry,
        opts: &Opts,
    ) -> Result<BenchmarkSetup> {
        info!("Running benchmark setup ..");
        let (sender, recv) = tokio::sync::oneshot::channel::<()>();
        let join_handle = std::thread::spawn(move || {
            Builder::new_multi_thread()
                .build()
                .unwrap()
                .block_on(async move {
                    barrier.wait().await;
                    recv.await.expect("Unable to wait for terminate signal");
                });
        });

        let genesis = sui_config::node::Genesis::new_from_file(&opts.genesis_blob_path);
        let genesis = genesis.genesis()?;

        let fullnode_rpc_urls = opts.fullnode_rpc_addresses.clone();
        info!("List of fullnode rpc urls: {:?}", fullnode_rpc_urls);
        let proxies: Vec<Arc<dyn ValidatorProxy + Send + Sync>> = if opts.use_fullnode_for_execution
        {
            if fullnode_rpc_urls.is_empty() {
                bail!("fullnode-rpc-url is required when use-fullnode-for-execution is true");
            }
            let mut fullnodes: Vec<Arc<dyn ValidatorProxy + Send + Sync>> = vec![];
            for fullnode_rpc_url in fullnode_rpc_urls.iter() {
                info!("Using FullNodeProxy: {:?}", fullnode_rpc_url);
                fullnodes.push(Arc::new(FullNodeProxy::from_url(fullnode_rpc_url).await?));
            }
            fullnodes
        } else {
            info!("Using LocalValidatorAggregatorProxy");
            if fullnode_rpc_urls.is_empty() {
                bail!("fullnode RPC url is required for reconfiguration");
            }
            let reconfig_fullnode_rpc_url =
                // Only need to use one full node for reconfiguration.
                fullnode_rpc_urls.choose(&mut rand::thread_rng()).context(
                    "Failed to get fullnode-rpc-url which is required for reconfiguration",
                )?;
            vec![Arc::new(
                LocalValidatorAggregatorProxy::from_genesis(
                    genesis,
                    registry,
                    reconfig_fullnode_rpc_url,
                    None,
                )
                .await,
            )]
        };
        let proxy = proxies
            .choose(&mut rand::thread_rng())
            .context("Failed to get proxy for reconfiguration")?;
        info!(
            "Reconfiguration - Reconfiguration to epoch {} is done",
            proxy.get_current_epoch(),
        );

        let primary_gas_owner_addr = ObjectID::from_hex_literal(&opts.primary_gas_owner_id)?;
        let keystore_path = Some(&opts.keystore_path)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| {
                anyhow!(format!(
                    "Failed to find keypair at path: {}",
                    &opts.keystore_path
                ))
            })?;

        let current_gas = if opts.use_fullnode_for_execution {
            // Go through fullnode to get the current gas object.
            let mut gas_objects = proxy
                .get_owned_objects(primary_gas_owner_addr.into())
                .await?;
            gas_objects.sort_by_key(|&(gas, _)| std::cmp::Reverse(gas));

            // TODO: Merge all owned gas objects into one and use that as the primary gas object.
            let (balance, primary_gas_obj) = gas_objects
                .iter()
                .max_by_key(|(balance, _)| balance)
                .context(
                    "Failed to choose the gas object with the largest amount of gas".to_string(),
                )?;

            info!(
                "Using primary gas id: {} with balance of {balance}",
                primary_gas_obj.id()
            );

            let primary_gas_account = primary_gas_obj.owner.get_owner_address()?;

            let keypair = Arc::new(get_ed25519_keypair_from_keystore(
                keystore_path,
                &primary_gas_account,
            )?);

            (
                primary_gas_obj.compute_object_reference(),
                primary_gas_account,
                keypair,
            )
        } else {
            // Go through local proxy to get the current gas object.
            let mut genesis_gas_objects = Vec::new();

            for obj in genesis.objects().iter() {
                let owner = &obj.owner;
                if let Owner::AddressOwner(addr) = owner {
                    if *addr == primary_gas_owner_addr.into() {
                        genesis_gas_objects.push(obj.clone());
                    }
                }
            }

            let genesis_gas_obj = genesis_gas_objects
                .choose(&mut rand::thread_rng())
                .context("Failed to choose a random primary gas")?
                .clone();

            let current_gas_object = proxy.get_object(genesis_gas_obj.id()).await?;
            let current_gas_account = current_gas_object.owner.get_owner_address()?;

            let keypair = Arc::new(get_ed25519_keypair_from_keystore(
                keystore_path,
                &current_gas_account,
            )?);

            info!("Using primary gas obj: {}", current_gas_object.id());

            (
                current_gas_object.compute_object_reference(),
                current_gas_account,
                keypair,
            )
        };

        Ok(BenchmarkSetup {
            server_handle: join_handle,
            shutdown_notifier: sender,
            bank: BenchmarkBank::new(proxy.clone(), current_gas),
            proxies,
        })
    }
}
