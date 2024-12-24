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
use std::time::Duration;
use sui_swarm_config::genesis_config::AccountConfig;
use sui_types::base_types::ConciseableName;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{deterministic_random_account_key, AccountKeyPair};
use sui_types::gas_coin::TOTAL_SUPPLY_MIST;
use sui_types::object::Owner;
use test_cluster::TestClusterBuilder;
use tokio::runtime::Builder;
use tokio::sync::{oneshot, Barrier};
use tokio::time::sleep;
use tracing::info;

pub enum Env {
    // Mode where benchmark in run on a validator cluster that gets spun up locally
    Local,
    // Mode where benchmark is run on an already running remote cluster
    Remote,
}

pub struct BenchmarkSetup {
    pub server_handle: JoinHandle<()>,
    pub shutdown_notifier: oneshot::Sender<()>,
    pub bank: BenchmarkBank,
    pub proxies: Vec<Arc<dyn ValidatorProxy + Send + Sync>>,
}

impl Env {
    pub async fn setup(
        &self,
        barrier: Arc<Barrier>,
        registry: &Registry,
        opts: &Opts,
    ) -> Result<BenchmarkSetup> {
        match self {
            Env::Local => {
                self.setup_local_env(
                    barrier,
                    registry,
                    opts.committee_size as usize,
                    opts.num_server_threads,
                )
                .await
            }
            Env::Remote => {
                self.setup_remote_env(
                    barrier,
                    registry,
                    opts.primary_gas_owner_id.as_str(),
                    opts.keystore_path.as_str(),
                    opts.genesis_blob_path.as_str(),
                    opts.use_fullnode_for_reconfig,
                    opts.use_fullnode_for_execution,
                    opts.fullnode_rpc_addresses.clone(),
                )
                .await
            }
        }
    }

    async fn setup_local_env(
        &self,
        barrier: Arc<Barrier>,
        registry: &Registry,
        committee_size: usize,
        num_server_threads: u64,
    ) -> Result<BenchmarkSetup> {
        info!("Running benchmark setup in local mode..");
        let (primary_gas_owner, keypair): (SuiAddress, AccountKeyPair) =
            deterministic_random_account_key();
        let keypair = Arc::new(keypair);

        // spawn a thread to spin up sui nodes on the multi-threaded server runtime.
        // running forever
        let (shutdown_sender, shutdown_recv) = tokio::sync::oneshot::channel::<()>();
        let (genesis_sender, genesis_recv) = tokio::sync::oneshot::channel();
        let join_handle = std::thread::spawn(move || {
            // create server runtime
            let server_runtime = Builder::new_multi_thread()
                .thread_stack_size(32 * 1024 * 1024)
                .worker_threads(num_server_threads as usize)
                .enable_all()
                .build()
                .unwrap();
            server_runtime.block_on(async move {
                let cluster = TestClusterBuilder::new()
                    .with_accounts(vec![AccountConfig {
                        address: Some(primary_gas_owner),
                        // We can't use TOTAL_SUPPLY_MIST because we need to account for validator stakes in genesis allocation.
                        gas_amounts: vec![TOTAL_SUPPLY_MIST / 2],
                    }])
                    .with_num_validators(committee_size)
                    .build()
                    .await;
                let genesis = cluster.swarm.config().genesis.clone();
                for v in cluster.swarm.config().validator_configs() {
                    eprintln!(
                        "Metric address for validator {}: {}",
                        v.protocol_public_key().concise(),
                        v.metrics_address
                    );
                }
                let primary_gas = cluster
                    .wallet
                    .get_one_gas_object_owned_by_address(primary_gas_owner)
                    .await
                    .unwrap()
                    .unwrap();
                // Send genesis and primary gas object to the main thread.
                genesis_sender.send((genesis, primary_gas)).unwrap();
                barrier.wait().await;
                shutdown_recv
                    .await
                    .expect("Unable to wait for terminate signal");
            });
        });
        // Wait for the embedded reconfig observer.
        sleep(Duration::from_secs(5)).await;
        let (genesis, primary_gas) = genesis_recv.await.unwrap();
        let proxy: Arc<dyn ValidatorProxy + Send + Sync> =
            Arc::new(LocalValidatorAggregatorProxy::from_genesis(&genesis, registry, None).await);
        Ok(BenchmarkSetup {
            server_handle: join_handle,
            shutdown_notifier: shutdown_sender,
            bank: BenchmarkBank::new(proxy.clone(), (primary_gas, primary_gas_owner, keypair)),
            proxies: vec![proxy],
        })
    }

    async fn setup_remote_env(
        &self,
        barrier: Arc<Barrier>,
        registry: &Registry,
        primary_gas_owner_id: &str,
        keystore_path: &str,
        genesis_blob_path: &str,
        use_fullnode_for_reconfig: bool,
        use_fullnode_for_execution: bool,
        fullnode_rpc_address: Vec<String>,
    ) -> Result<BenchmarkSetup> {
        info!("Running benchmark setup in remote mode ..");
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

        let genesis = sui_config::node::Genesis::new_from_file(genesis_blob_path);
        let genesis = genesis.genesis()?;

        let fullnode_rpc_urls = fullnode_rpc_address.clone();
        info!("List of fullnode rpc urls: {:?}", fullnode_rpc_urls);
        let proxies: Vec<Arc<dyn ValidatorProxy + Send + Sync>> = if use_fullnode_for_execution {
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
            let reconfig_fullnode_rpc_url = if use_fullnode_for_reconfig {
                // Only need to use one full node for reconfiguration.
                Some(fullnode_rpc_urls.choose(&mut rand::thread_rng()).context(
                    "Failed to get fullnode-rpc-url which is required when use-fullnode-for-reconfig is true",
                )?)
            } else {
                None
            };
            vec![Arc::new(
                LocalValidatorAggregatorProxy::from_genesis(
                    genesis,
                    registry,
                    reconfig_fullnode_rpc_url.map(|x| &**x),
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

        let primary_gas_owner_addr = ObjectID::from_hex_literal(primary_gas_owner_id)?;
        let keystore_path = Some(&keystore_path)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| {
                anyhow!(format!(
                    "Failed to find keypair at path: {}",
                    &keystore_path
                ))
            })?;

        let current_gas = if use_fullnode_for_execution {
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
