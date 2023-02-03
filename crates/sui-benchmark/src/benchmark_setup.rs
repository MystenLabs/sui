// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::{anyhow, bail, Context, Result};
use move_core_types::language_storage::TypeTag;
use prometheus::Registry;
use rand::seq::SliceRandom;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_config::utils;

use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{deterministic_random_account_key, AccountKeyPair};
use tokio::time::sleep;

use crate::options::Opts;
use crate::util::get_ed25519_keypair_from_keystore;
use crate::workloads::Gas;
use crate::{FullNodeProxy, LocalValidatorAggregatorProxy, ValidatorProxy};
use sui_types::object::{generate_test_gas_objects_with_owner, Owner};
use test_utils::authority::test_and_configure_authority_configs;
use test_utils::authority::{spawn_fullnode, spawn_test_authorities};
use tokio::runtime::Builder;
use tokio::sync::Barrier;
use tracing::info;

pub enum Env {
    // Mode where benchmark in run on a validator cluster that gets spun up locally
    Local,
    // Mode where benchmark is run on a already running remote cluster
    Remote,
}

pub struct ProxyGasAndCoin {
    pub proxy: Arc<dyn ValidatorProxy + Send + Sync>,
    // Gas to use for execution of gas generation transaction
    pub primary_gas: Gas,
    // Coin to use for splitting and generating small gas coins
    pub pay_coin: Gas,
    pub pay_coin_type_tag: TypeTag,
}

impl Env {
    pub async fn setup(
        &self,
        barrier: Arc<Barrier>,
        registry: &Registry,
        opts: &Opts,
    ) -> Result<Vec<ProxyGasAndCoin>> {
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
    ) -> Result<Vec<ProxyGasAndCoin>> {
        info!("Running benchmark setup in local mode..");
        let mut network_config = test_and_configure_authority_configs(committee_size);
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
        let (owner, keypair): (SuiAddress, AccountKeyPair) = deterministic_random_account_key();
        let generated_gas = generate_test_gas_objects_with_owner(2, owner);
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
        let _validators = std::thread::spawn(move || {
            // create server runtime
            let server_runtime = Builder::new_multi_thread()
                .thread_stack_size(32 * 1024 * 1024)
                .worker_threads(num_server_threads as usize)
                .enable_all()
                .build()
                .unwrap();
            server_runtime.block_on(async move {
                // Setup the network
                let _validators: Vec<_> =
                    spawn_test_authorities(generated_gas, &cloned_config).await;
                let _fullnode = spawn_fullnode(&cloned_config, Some(fullnode_rpc_port)).await;
                fullnode_barrier_clone.wait().await;
                barrier.wait().await;
                // This thread cannot exit, otherwise validators will shutdown.
                loop {
                    sleep(Duration::from_secs(300)).await;
                }
            });
        });
        // Let fullnode be created.
        sleep(Duration::from_secs(5)).await;
        let fullnode_rpc_url = format!("http://{fullnode_ip}:{fullnode_rpc_port}");
        info!("Fullnode rpc url: {fullnode_rpc_url}");
        fullnode_barrier.wait().await;
        let proxy: Arc<dyn ValidatorProxy + Send + Sync> = Arc::new(
            LocalValidatorAggregatorProxy::from_network_config(
                &config,
                registry,
                Some(&fullnode_rpc_url),
            )
            .await,
        );
        let keypair = Arc::new(keypair);
        let ttag = pay_coin.get_move_template_type()?;
        Ok(vec![ProxyGasAndCoin {
            primary_gas: (
                primary_gas.compute_object_reference(),
                Owner::AddressOwner(owner),
                keypair.clone(),
            ),
            pay_coin: (
                pay_coin.compute_object_reference(),
                Owner::AddressOwner(owner),
                keypair,
            ),
            pay_coin_type_tag: ttag,
            proxy,
        }])
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
    ) -> Result<Vec<ProxyGasAndCoin>> {
        info!("Running benchmark setup in remote mode ..");
        std::thread::spawn(move || {
            Builder::new_multi_thread()
                .build()
                .unwrap()
                .block_on(async move {
                    barrier.wait().await;
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
            let reconfig_fullnode_rpc_url =
                if use_fullnode_for_reconfig {
                    // Only need to use one full node for reconfiguration.
                    Some(fullnode_rpc_urls.get(0).expect(
                        "fullnode-rpc-url is required when use-fullnode-for-reconfig is true",
                    ))
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
        info!(
            "Reconfiguration - Reconfiguration to epoch {} is done",
            proxies[0].get_current_epoch(),
        );

        let mut proxy_gas_and_coins = vec![];

        for proxy in proxies.iter() {
            let offset = ObjectID::from_hex_literal(primary_gas_id)?;
            let ids = ObjectID::in_range(offset, primary_gas_objects)?;
            let primary_gas_id = ids
                .choose(&mut rand::thread_rng())
                .context("Failed to choose a random primary gas id")?;
            let primary_gas = proxy.get_object(*primary_gas_id).await?;
            let pay_coin_id = ids
                .choose(&mut rand::thread_rng())
                .context("Failed to choose a random pay coin")?;
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
            let ttag = pay_coin.get_move_template_type()?;
            proxy_gas_and_coins.push(ProxyGasAndCoin {
                primary_gas: (
                    primary_gas.compute_object_reference(),
                    Owner::AddressOwner(primary_gas_account),
                    keypair.clone(),
                ),
                pay_coin: (pay_coin.compute_object_reference(), pay_coin.owner, keypair),
                pay_coin_type_tag: ttag,
                proxy: proxy.clone(),
            })
        }
        Ok(proxy_gas_and_coins)
    }
}
