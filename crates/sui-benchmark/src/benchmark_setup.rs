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
use std::str::FromStr;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use sui_config::utils;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{deterministic_random_account_key, AccountKeyPair};
use sui_types::object::generate_max_test_gas_objects_with_owner;
use sui_types::object::Owner;
use test_utils::authority::test_and_configure_authority_configs_with_objects;
use test_utils::authority::{spawn_fullnode, spawn_test_authorities};
use tokio::runtime::Builder;
use tokio::sync::{oneshot, Barrier};
use tokio::time::sleep;
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
        server_metric_port: u16,
        num_server_threads: u64,
    ) -> Result<BenchmarkSetup> {
        info!("Running benchmark setup in local mode..");
        let (address, keypair): (SuiAddress, AccountKeyPair) = deterministic_random_account_key();
        let generated_gas = generate_max_test_gas_objects_with_owner(1, address);
        let (mut network_config, generated_gas) =
            test_and_configure_authority_configs_with_objects(committee_size, generated_gas);
        let mut metric_port = server_metric_port;
        for node_config in network_config.validator_configs.iter_mut() {
            // Benchmark setup allocates very large gas objects, which will lead to overflow if we attempt
            // to calculate the amount of SUI in the network. Hence we disable SUI conservation checks
            // even when we are running in debug mode.
            node_config
                .expensive_safety_check_config
                .force_disable_epoch_sui_conservation_check();
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
        let primary_gas = (primary_gas.compute_object_reference(), address, keypair);
        Ok(BenchmarkSetup {
            server_handle: join_handle,
            shutdown_notifier: sender,
            bank: BenchmarkBank::new(proxy.clone(), primary_gas),
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
            let fullnode_rpc_url: &String = &fullnode_rpc_urls[0];
            let fn_proxy = Arc::new(FullNodeProxy::from_url(fullnode_rpc_url).await?);
            let mut gas_objects = fn_proxy
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
            // let mut genesis_gas_objects = Vec::new();

            let custom_gas_object_ids = vec![
                "0x06b9977c52fa395b60d91a8deef70f474a24c2c046be6e448ab2e536f66c25ed",
                "0x10b847ecdbcd164632d605099041e27d2dea2736846865c0b45f986cd647adf2",
                "0x122c23c57b2fec02c0550910fc5eef0f1c59553a44aa9007e1253d1c07da912e",
                "0x138046f4273d1f05407874e712c3f27135183d35800e77987d50d7bc6d7d615a",
                "0x1c978a7a09d8432d65900f58c2a3e9293c17cba460b3e94d1bdf2dfc6a1d0685",
                "0x21e0153041770519442e8cfdbdf088f5db5afa329b1c65fc782d95000124c8bc",
                "0x25d3b250a868b64815be3a61269aa40634f1dd263d9f1c9021c39539ee7f2933",
                "0x49e283a42c78d49f0ca93dcf69ea913bb5bcf29ca25cff12127b4fd487bc6b23",
                "0x5c427cdf9aeb8a29ab63f42e0270f8fe24bb38b96929b98272a6a9e6d59b9973",
                "0x5f85d2aaa8712de3c11d5db8d64952f064f0ad419b50fdc1ee1b298f6e859ea5",
                "0x75ebf54316094610f6221d67bd50680b8abf62fa56c53f463d8685515c0e5c91",
                "0x832a178ec3dffc5cc2e0e8c55405f2d03d45518c9adc7155e51369744c24947f",
                "0xfad8c80ba5555a488b113547d15612d7b1c2d733cabbe9e1106b3c23c77ef21c",
                "0x92364710f541181e0534895515412f30b37600a056b3d6cbd674cb119624b5f7",
                "0x9aa902dd01ef574908f7fba3d71a1a37810ffd0d7f19748bca008caf74c80f04",
                "0xc6f0555f63544afbbb6b97eb1519f9e451576257ad0eadc0c85dfd1db51e79c7",
                "0xe5f5e91d7ec1b4d74e5446ff5335ee5ba12ea8c092ef061854d0c64a776fc82d",
            ];
            // for obj in genesis.objects().iter() {
            //     let owner = &obj.owner;
            //     if let Owner::AddressOwner(addr) = owner {
            //         if *addr == primary_gas_owner_addr.into() {
            //             genesis_gas_objects.push(obj.clone());
            //         }
            //     }
            // }

            // let genesis_gas_obj = genesis_gas_objects
            //     .choose(&mut rand::thread_rng())
            //     .context("Failed to choose a random primary gas")?
            //     .clone();

            let genesis_gas_obj_id = custom_gas_object_ids
                .choose(&mut rand::thread_rng())
                .context("Failed to choose a random primary gas")?
                .clone();

            let current_gas_object = proxy
                .get_object(ObjectID::from_str(genesis_gas_obj_id).unwrap())
                .await?;
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
