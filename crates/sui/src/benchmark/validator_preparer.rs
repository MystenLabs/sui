// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::large_enum_variant)]
use crate::benchmark::bench_types::RunningMode;
use crate::benchmark::load_generator::spawn_authority_server;
use sui_config::genesis_config::ObjectConfig;
use sui_config::NetworkConfig;

use multiaddr::Multiaddr;
use rocksdb::Options;
use std::{
    env, fs, panic,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::Arc,
    thread,
    thread::sleep,
    time::Duration,
};
use sui_core::authority::*;
use sui_types::{
    base_types::{SuiAddress, *},
    committee::*,
    crypto::{KeyPair, PublicKeyBytes},
    gas_coin::GasCoin,
    object::Object,
};
use tokio::runtime::{Builder, Runtime};
use tracing::{error, info};

pub const VALIDATOR_BINARY_NAME: &str = "validator";

/// A helper class to set up validators for benchmarking
#[allow(unused)]
pub struct ValidatorPreparer {
    running_mode: RunningMode,
    pub network_config: NetworkConfig,
    main_authority_address_hex: String,
    pub committee: Committee,
    validator_config: ValidatorConfig,
}

pub enum ValidatorConfig {
    LocalSingleValidatorThreadConfig {
        authority_state: Option<AuthorityState>,
        authority_store: Arc<AuthorityStore>,
    },
    LocalSingleValidatorProcessConfig {
        working_dir: PathBuf,
        validator_process: Option<Child>,
    },
    RemoteValidatorConfig,
}

impl ValidatorPreparer {
    pub fn new_for_remote(network_config: NetworkConfig) -> Self {
        let committee = network_config.committee();

        Self {
            running_mode: RunningMode::RemoteValidator,
            network_config,
            main_authority_address_hex: "".to_string(),
            committee,
            validator_config: ValidatorConfig::RemoteValidatorConfig,
        }
    }
    pub fn new_for_local(
        running_mode: RunningMode,
        working_dir: Option<PathBuf>,
        committee_size: usize,
        _validator_address: Multiaddr,
        db_cpus: usize,
    ) -> Self {
        let temp_dir = tempfile::tempdir().unwrap();
        let network_config = NetworkConfig::generate(temp_dir.path(), committee_size);

        let main_authority_address_hex =
            format!("{}", network_config.validator_configs()[0].sui_address());
        info!("authority address hex: {}", main_authority_address_hex);

        let committee = network_config.committee();

        match running_mode {
            RunningMode::SingleValidatorProcess => Self {
                running_mode,
                network_config,
                main_authority_address_hex,
                committee,
                validator_config: ValidatorConfig::LocalSingleValidatorProcessConfig {
                    working_dir: working_dir.unwrap(),
                    validator_process: None,
                },
            },

            RunningMode::SingleValidatorThread => {
                // Pick the first validator and create state.
                let validator_config = &network_config.validator_configs()[0];
                let committee = network_config.committee();

                // Create a random directory to store the DB
                let path = env::temp_dir().join(format!("DB_{:?}", ObjectID::random()));
                let auth_state = make_authority_state(
                    &path,
                    db_cpus as i32,
                    &committee,
                    &validator_config.public_key(),
                    validator_config.key_pair().copy(),
                );

                Self {
                    running_mode,
                    network_config,
                    main_authority_address_hex,
                    committee,
                    validator_config: ValidatorConfig::LocalSingleValidatorThreadConfig {
                        authority_state: Some(auth_state.0),
                        authority_store: auth_state.1,
                    },
                }
            }
            RunningMode::RemoteValidator => panic!("Use new_for_remote"),
        }
    }

    pub fn deploy_validator(&mut self, address: Multiaddr) {
        match self.running_mode {
            RunningMode::SingleValidatorProcess => {
                if let ValidatorConfig::LocalSingleValidatorProcessConfig {
                    working_dir,
                    validator_process,
                } = &mut self.validator_config
                {
                    let config_path = working_dir.clone().join("node.conf");

                    let config = &self.network_config.validator_configs()[0];

                    fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap())
                        .unwrap();

                    info!("Spawning a validator process...");
                    let child = Command::new(working_dir.clone().join(VALIDATOR_BINARY_NAME))
                        .arg("--network-config-path")
                        .arg(config_path)
                        .spawn()
                        .expect("failed to spawn a validator process");
                    validator_process.replace(child);
                } else {
                    panic!("Invalid validator config in local-single-validator-process mode");
                }
            }
            RunningMode::SingleValidatorThread => {
                if let ValidatorConfig::LocalSingleValidatorThreadConfig {
                    authority_state,
                    authority_store: _,
                } = &mut self.validator_config
                {
                    // Make multi-threaded runtime for the authority
                    let state = authority_state.take().unwrap();
                    thread::spawn(move || {
                        info!("Spawning a validator thread...");
                        get_multithread_runtime().block_on(async move {
                            let server = spawn_authority_server(address, state).await;
                            if let Err(e) = server.join().await {
                                error!("Server ended with an error: {e}");
                            }
                        });
                    });
                } else {
                    panic!("Invalid validator config in local-single-validator-thread mode");
                }
            }
            RunningMode::RemoteValidator => (),
        }
        // Wait for server start
        sleep(Duration::from_secs(3));
    }

    pub async fn update_objects_for_validator(
        &mut self,
        objects: Vec<Object>,
        _address: SuiAddress,
    ) {
        match self.running_mode {
            RunningMode::SingleValidatorProcess => {
                let _all_objects: Vec<ObjectConfig> = objects
                    .iter()
                    .map(|object| ObjectConfig {
                        object_id: object.id(),
                        gas_value: get_gas_value(object),
                    })
                    .collect();
                if let ValidatorConfig::LocalSingleValidatorProcessConfig {
                    working_dir: _,
                    validator_process: _,
                } = &mut self.validator_config
                {
                    //TODO objects need to be inserted at genesis time
                    todo!()
                } else {
                    panic!("invalid validator config in local-single-validator-process mode");
                }
            }
            RunningMode::SingleValidatorThread => {
                if let ValidatorConfig::LocalSingleValidatorThreadConfig {
                    authority_state: _,
                    authority_store,
                } = &mut self.validator_config
                {
                    authority_store
                        .bulk_object_insert(&objects[..].iter().collect::<Vec<&Object>>())
                        .await
                        .unwrap();
                } else {
                    panic!("invalid validator config in local-single-validator-thread mode");
                }
            }

            // Nothing to do here. Remote machine must be provisioned separately
            RunningMode::RemoteValidator => (),
        }
    }

    pub fn clean_up(&mut self) {
        if let RunningMode::SingleValidatorProcess = self.running_mode {
            info!("Cleaning up local validator process...");
            if let ValidatorConfig::LocalSingleValidatorProcessConfig {
                working_dir: _,
                validator_process,
            } = &mut self.validator_config
            {
                validator_process
                    .take()
                    .unwrap()
                    .kill()
                    .expect("Failed to kill validator process");
            } else {
                panic!("invalid validator config in local-single-validator-process mode");
            }
        }
    }
}

fn get_gas_value(o: &Object) -> u64 {
    GasCoin::try_from(o.data.try_as_move().unwrap())
        .unwrap()
        .value()
}

pub fn get_multithread_runtime() -> Runtime {
    Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .worker_threads(usize::min(num_cpus::get(), 24))
        .build()
        .unwrap()
}

fn make_authority_state(
    store_path: &Path,
    db_cpus: i32,
    committee: &Committee,
    pubx: &PublicKeyBytes,
    secx: KeyPair,
) -> (AuthorityState, Arc<AuthorityStore>) {
    fs::create_dir(&store_path).unwrap();
    info!("Open database on path: {:?}", store_path.as_os_str());

    let mut opts = Options::default();
    opts.increase_parallelism(db_cpus);
    opts.set_write_buffer_size(256 * 1024 * 1024);
    opts.enable_statistics();
    opts.set_stats_dump_period_sec(5);
    opts.set_enable_pipelined_write(true);

    // NOTE: turn off the WAL, but is not guaranteed to
    // recover from a crash. Keep turned off to max safety,
    // but keep as an option if we periodically flush WAL
    // manually.
    // opts.set_manual_wal_flush(true);

    let store = Arc::new(AuthorityStore::open(store_path, Some(opts)));
    (
        Runtime::new().unwrap().block_on(async {
            AuthorityState::new(
                committee.clone(),
                *pubx,
                Arc::pin(secx),
                store.clone(),
                None,
                None,
                None,
                &sui_config::genesis::Genesis::get_default_genesis(),
                &prometheus::Registry::new(),
            )
            .await
        }),
        store,
    )
}
