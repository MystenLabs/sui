// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::{anyhow, bail, Context, Result};
use prometheus::Registry;
use rand::seq::SliceRandom;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use sui_config::utils;

use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{deterministic_random_account_key, AccountKeyPair};
use tokio::time::sleep;

use crate::bank::BenchmarkBank;
use crate::options::Opts;
use crate::util::get_ed25519_keypair_from_keystore;
use crate::{FullNodeProxy, LocalValidatorAggregatorProxy, ValidatorProxy};
use sui_types::object::generate_max_test_gas_objects_with_owner;
use test_utils::authority::test_and_configure_authority_configs_with_objects;
use test_utils::authority::{spawn_fullnode, spawn_test_authorities};
use tokio::runtime::Builder;
use tokio::sync::{oneshot, Barrier};
use tracing::info;

pub enum Env {
    // Mode where benchmark in run on a validator cluster that gets spun up locally
    Local,
    // Mode where benchmark is run on a already running remote cluster
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
                    opts.server_metric_port,
                    opts.num_server_threads,
                )
                .await
            }
            Env::Remote => {
                self.setup_remote_env(
                    barrier,
                    registry,
                    opts.primary_gas_id.as_str(),
                    opts.primary_gas_objects,
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
        server_metric_port: u16,
        num_server_threads: u64,
    ) -> Result<BenchmarkSetup> {
        info!("Running benchmark setup in local mode..");
        let (address, keypair): (SuiAddress, AccountKeyPair) = deterministic_random_account_key();
        let generated_gas = generate_max_test_gas_objects_with_owner(2, address);
        let (mut network_config, generated_gas) =
            test_and_configure_authority_configs_with_objects(committee_size, generated_gas);
        let mut metric_port = server_metric_port;
        for node_config in network_config.validator_configs.iter_mut() {
            let parameters = &mut node_config
                .consensus_config
                .as_mut()
                .context("Missing consensus config")?
                .narwhal_config;
            parameters.batch_size = 12800;
            node_config.metrics_address = format!("127.0.0.1:{}", metric_port)
                .parse()
                .context("Failed to parse metric address")?;
            metric_port += 1;
        }
        let config = Arc::new(network_config);
        // bring up servers ..
        let primary_gas = generated_gas
            .get(0)
            .context("No gas found at index 0")?
            .clone();
        let pay_coin = generated_gas
            .get(1)
            .context("No gas found at index 1")?
            .clone();
        // Make the client runtime wait until we are done creating genesis objects
        let cloned_config = config.clone();
        let fullnode_ip = format!("{}", utils::get_local_ip_for_tests());
        let fullnode_rpc_port = utils::get_available_port(&fullnode_ip);
        let fullnode_barrier = Arc::new(Barrier::new(2));
        let fullnode_barrier_clone = fullnode_barrier.clone();
        // spawn a thread to spin up sui nodes on the multi-threaded server runtime.
        // running forever
        let (sender, recv) = tokio::sync::oneshot::channel::<()>();
        let join_handle = std::thread::spawn(move || {
            // create server runtime
            let server_runtime = Builder::new_multi_thread()
                .thread_stack_size(32 * 1024 * 1024)
                .worker_threads(num_server_threads as usize)
                .enable_all()
                .build()
                .unwrap();
            server_runtime.block_on(async move {
                // Setup the network
                let _validators: Vec<_> = spawn_test_authorities(&cloned_config).await;
                let _fullnode = spawn_fullnode(&cloned_config, Some(fullnode_rpc_port)).await;
                fullnode_barrier_clone.wait().await;
                barrier.wait().await;
                recv.await.expect("Unable to wait for terminate signal");
            });
        });
        // Let fullnode be created.
        sleep(Duration::from_secs(5)).await;
        let fullnode_rpc_url = format!("http://{fullnode_ip}:{fullnode_rpc_port}");
        info!("Fullnode rpc url: {fullnode_rpc_url}");
        fullnode_barrier.wait().await;
        let proxy: Arc<dyn ValidatorProxy + Send + Sync> = Arc::new(
            LocalValidatorAggregatorProxy::from_genesis(&config.genesis, registry, None).await,
        );
        let keypair = Arc::new(keypair);
        let primary_gas = (
            primary_gas.compute_object_reference(),
            address,
            keypair.clone(),
        );
        let pay_coin = (pay_coin.compute_object_reference(), address, keypair);
        Ok(BenchmarkSetup {
            server_handle: join_handle,
            shutdown_notifier: sender,
            bank: BenchmarkBank::new(proxy.clone(), primary_gas, pay_coin),
            proxies: vec![proxy],
        })
    }

    async fn setup_remote_env(
        &self,
        barrier: Arc<Barrier>,
        registry: &Registry,
        primary_gas_id: &str,
        primary_gas_objects: u64,
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
        let fullnode_rpc_urls = fullnode_rpc_address.clone();
        info!("List of fullnode rpc urls: {:?}", fullnode_rpc_urls);
        let proxies: Vec<Arc<dyn ValidatorProxy + Send + Sync>> = if use_fullnode_for_execution {
            if fullnode_rpc_urls.is_empty() {
                bail!("fullnode-rpc-url is required when use-fullnode-for-execution is true");
            }
            let mut fullnodes: Vec<Arc<dyn ValidatorProxy + Send + Sync>> = vec![];
            for fullnode_rpc_url in fullnode_rpc_urls {
                info!("Using FullNodeProxy: {:?}", fullnode_rpc_url);
                fullnodes.push(Arc::new(FullNodeProxy::from_url(&fullnode_rpc_url).await?));
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
            let genesis = sui_config::node::Genesis::new_from_file(genesis_blob_path);
            let genesis = genesis.genesis()?;
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

        let mut used_ids = vec![];
        let offset = ObjectID::from_hex_literal(primary_gas_id)?;
        let ids = ObjectID::in_range(offset, primary_gas_objects)?;
        let mut primary_gas_id = ids
            .choose(&mut rand::thread_rng())
            .context("Failed to choose a random primary gas id")?;
        while used_ids.contains(primary_gas_id) {
            primary_gas_id = ids
                .choose(&mut rand::thread_rng())
                .context("Failed to choose a random primary gas id")?;
        }
        used_ids.push(*primary_gas_id);
        let primary_gas = proxy.get_object(*primary_gas_id).await?;
        let mut pay_coin_id = ids
            .choose(&mut rand::thread_rng())
            .context("Failed to choose a random pay coin")?;
        while used_ids.contains(pay_coin_id) {
            pay_coin_id = ids
                .choose(&mut rand::thread_rng())
                .context("Failed to choose a random primary gas id")?;
        }
        used_ids.push(*pay_coin_id);
        let pay_coin = proxy.get_object(*pay_coin_id).await?;
        let primary_gas_account = primary_gas.owner.get_owner_address()?;
        let keystore_path = Some(&keystore_path)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| {
                anyhow!(format!(
                    "Failed to find keypair at path: {}",
                    &keystore_path
                ))
            })?;
        let keypair = Arc::new(get_ed25519_keypair_from_keystore(
            keystore_path,
            &primary_gas_account,
        )?);
        let primary_gas = (
            primary_gas.compute_object_reference(),
            primary_gas_account,
            keypair.clone(),
        );
        let pay_coin = (
            pay_coin.compute_object_reference(),
            pay_coin.owner.get_owner_address()?,
            keypair,
        );
        Ok(BenchmarkSetup {
            server_handle: join_handle,
            shutdown_notifier: sender,
            bank: BenchmarkBank::new(proxy.clone(), primary_gas, pay_coin),
            proxies,
        })
    }
}
