// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::large_enum_variant)]
use crate::benchmark::bench_types::RunningMode;
use crate::benchmark::load_generator::spawn_authority_server;
use crate::config::{AccountConfig, Config, GenesisConfig, ObjectConfig};
use rocksdb::Options;
use std::env;
use std::fs;
use std::panic;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::process::Command;
use std::sync::Arc;
use std::thread;
use std::{thread::sleep, time::Duration};
use sui_adapter::genesis;
use sui_core::authority::*;
use sui_network::network::NetworkServer;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{random_key_pairs, KeyPair, PublicKeyBytes};
use sui_types::gas_coin::GasCoin;
use sui_types::object::Object;
use sui_types::{base_types::*, committee::*};
use tokio::runtime::{Builder, Runtime};
use tracing::{error, info};
const GENESIS_CONFIG_NAME: &str = "genesis_config.json";

pub const VALIDATOR_BINARY_NAME: &str = "validator";

/// A helper class to set up validators for benchmarking
#[allow(unused)]
pub struct ValidatorPreparer {
    running_mode: RunningMode,
    pub keys: Vec<KeyPair>,
    main_authority_address_hex: String,
    pub committee: Committee,
    validator_config: ValidatorConfig,
}

fn set_up_authorities_and_committee(
    committee_size: usize,
) -> Result<(Vec<KeyPair>, GenesisConfig), anyhow::Error> {
    let temp_dir = tempfile::tempdir()?;
    let key_pairs = random_key_pairs(committee_size);
    let key_pair = key_pairs[0].copy();
    let config = GenesisConfig::custom_genesis(
        temp_dir.path(),
        committee_size,
        0,
        0,
        Some((
            key_pairs
                .iter()
                .map(|kp| *kp.public_key_bytes())
                .collect::<Vec<_>>(),
            key_pair,
        )),
    )?;

    Ok((key_pairs, config))
}

pub enum ValidatorConfig {
    LocalSingleValidatorThreadConfig {
        authority_state: Option<AuthorityState>,
        authority_store: Arc<AuthorityStore>,
    },
    LocalSingleValidatorProcessConfig {
        genesis_config: Option<GenesisConfig>,
        working_dir: PathBuf,
        validator_process: Option<Child>,
    },
}

impl ValidatorPreparer {
    pub fn new(
        running_mode: RunningMode,
        working_dir: Option<PathBuf>,
        committee_size: usize,
        validator_host: &str,
        validator_port: u16,
        db_cpus: usize,
    ) -> Self {
        let (keys, mut genesis_config) = set_up_authorities_and_committee(committee_size)
            .expect("Got error in setting up committee");

        let main_authority_address_hex = format!(
            "{}",
            SuiAddress::from(genesis_config.key_pair.public_key_bytes())
        );
        info!("authority address hex: {}", main_authority_address_hex);

        let committee = Committee::new(
            0,
            genesis_config
                .authorities
                .iter()
                .map(|api| (api.public_key, 1))
                .collect(),
        );

        match running_mode {
            RunningMode::LocalSingleValidatorProcess => {
                // Honor benchmark's host:port setting
                genesis_config.authorities[0].port = validator_port;
                genesis_config.authorities[0].host = validator_host.into();
                Self {
                    running_mode,
                    keys,
                    main_authority_address_hex,
                    committee,
                    validator_config: ValidatorConfig::LocalSingleValidatorProcessConfig {
                        working_dir: working_dir.unwrap(),
                        genesis_config: Some(genesis_config),
                        validator_process: None,
                    },
                }
            }

            RunningMode::LocalSingleValidatorThread => {
                // Pick the first validator and create state.
                let public_auth0 = keys[0].public_key_bytes();
                let secret_auth0 = keys[0].copy();

                // Create a random directory to store the DB
                let path = env::temp_dir().join(format!("DB_{:?}", ObjectID::random()));
                let auth_state = make_authority_state(
                    &path,
                    db_cpus as i32,
                    &committee,
                    public_auth0,
                    secret_auth0,
                );

                Self {
                    running_mode,
                    keys,
                    main_authority_address_hex,
                    committee,
                    validator_config: ValidatorConfig::LocalSingleValidatorThreadConfig {
                        authority_state: Some(auth_state.0),
                        authority_store: auth_state.1,
                    },
                }
            }
        }
    }

    pub fn deploy_validator(&mut self, _network_server: NetworkServer) {
        match self.running_mode {
            RunningMode::LocalSingleValidatorProcess => {
                if let ValidatorConfig::LocalSingleValidatorProcessConfig {
                    working_dir,
                    genesis_config,
                    validator_process,
                } = &mut self.validator_config
                {
                    let config_path = working_dir.clone().join(GENESIS_CONFIG_NAME);
                    let config = genesis_config.take().unwrap().persisted(&config_path);
                    config.save().unwrap_or_else(|err| {
                        panic!("Can't save file {} due to {}", config_path.display(), err)
                    });

                    info!("Spawning a validator process...");
                    let child = Command::new(working_dir.clone().join(VALIDATOR_BINARY_NAME))
                        .arg("--genesis-config-path")
                        .arg(GENESIS_CONFIG_NAME)
                        .arg("--address")
                        .arg(&self.main_authority_address_hex)
                        .arg("--force-genesis")
                        .spawn()
                        .expect("failed to spawn a validator process");
                    validator_process.replace(child);
                } else {
                    panic!("Invalid validator config in local-single-validator-process mode");
                }
            }
            RunningMode::LocalSingleValidatorThread => {
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
                            let server = spawn_authority_server(_network_server, state).await;
                            if let Err(e) = server.join().await {
                                error!("Server ended with an error: {e}");
                            }
                        });
                    });
                } else {
                    panic!("Invalid validator config in local-single-validator-thread mode");
                }
            }
        }
        // Wait for server start
        sleep(Duration::from_secs(3));
    }

    pub fn update_objects_for_validator(&mut self, objects: Vec<Object>, address: SuiAddress) {
        match self.running_mode {
            RunningMode::LocalSingleValidatorProcess => {
                let all_objects: Vec<ObjectConfig> = objects
                    .iter()
                    .map(|object| ObjectConfig {
                        object_id: object.id(),
                        gas_value: get_gas_value(object),
                    })
                    .collect();
                if let ValidatorConfig::LocalSingleValidatorProcessConfig {
                    working_dir: _,
                    genesis_config,
                    validator_process: _,
                } = &mut self.validator_config
                {
                    genesis_config
                        .as_mut()
                        .unwrap()
                        .accounts
                        .push(AccountConfig {
                            address: Some(address),
                            gas_objects: all_objects,
                        })
                } else {
                    panic!("invalid validator config in local-single-validator-process mode");
                }
            }
            RunningMode::LocalSingleValidatorThread => {
                if let ValidatorConfig::LocalSingleValidatorThreadConfig {
                    authority_state: _,
                    authority_store,
                } = &mut self.validator_config
                {
                    authority_store
                        .bulk_object_insert(&objects[..].iter().collect::<Vec<&Object>>())
                        .unwrap();
                } else {
                    panic!("invalid validator config in local-single-validator-thread mode");
                }
            }
        }
    }

    pub fn clean_up(&mut self) {
        match self.running_mode {
            RunningMode::LocalSingleValidatorProcess => {
                info!("Cleaning up local validator process...");
                if let ValidatorConfig::LocalSingleValidatorProcessConfig {
                    working_dir: _,
                    genesis_config: _,
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
            RunningMode::LocalSingleValidatorThread => {}
        }
    }
}

fn get_gas_value(o: &Object) -> u64 {
    GasCoin::try_from(o.data.try_as_move().unwrap())
        .unwrap()
        .value()
}

fn get_multithread_runtime() -> Runtime {
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
                genesis::clone_genesis_compiled_modules(),
                &mut genesis::get_genesis_context(),
            )
            .await
        }),
        store,
    )
}
