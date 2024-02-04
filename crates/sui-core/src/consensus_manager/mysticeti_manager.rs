// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use arc_swap::ArcSwapOption;
use async_trait::async_trait;
use consensus_core::NetworkType;
use fastcrypto::bls12381::min_sig::BLS12381PublicKey;
use fastcrypto::traits::{KeyPair, ToFromBytes};
use itertools::Itertools;
use mysten_metrics::{RegistryID, RegistryService};
use mysticeti_core::commit_observer::SimpleCommitObserver;
use mysticeti_core::committee::{Authority, Committee};
use mysticeti_core::config::{Identifier, Parameters, PrivateConfig, SynchronizerParameters};
use mysticeti_core::types::AuthorityIndex;
use mysticeti_core::validator::Validator;
use mysticeti_core::{CommitConsumer, PublicKey, Signer, SimpleBlockHandler};
use narwhal_executor::ExecutionState;
use prometheus::Registry;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_config::NodeConfig;
use sui_types::base_types::AuthorityName;
use sui_types::{
    committee::EpochId,
    crypto::{AuthorityKeyPair, NetworkKeyPair},
    sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait,
};
use tokio::sync::{mpsc::unbounded_channel, Mutex};

use crate::{
    authority::authority_per_epoch_store::AuthorityPerEpochStore,
    consensus_handler::{ConsensusHandlerInitializer, MysticetiConsensusHandler},
    consensus_manager::{
        ConsensusManagerMetrics, ConsensusManagerTrait, Running, RunningLockGuard,
    },
    consensus_validator::SuiTxValidator,
    mysticeti_adapter::{LazyMysticetiClient, MysticetiClient},
};

#[cfg(test)]
#[path = "../unit_tests/mysticeti_manager_tests.rs"]
pub mod mysticeti_manager_tests;

pub struct MysticetiManager {
    keypair: AuthorityKeyPair,
    _network_keypair: NetworkKeyPair,
    storage_base_path: PathBuf,
    running: Mutex<Running>,
    metrics: ConsensusManagerMetrics,
    registry_service: RegistryService,
    validator: ArcSwapOption<(
        Validator<SimpleBlockHandler, SimpleCommitObserver>,
        RegistryID,
    )>,
    // Use a shared lazy mysticeti client so we can update the internal mysticeti
    // client that gets created for every new epoch.
    client: Arc<LazyMysticetiClient>,
    consensus_handler: ArcSwapOption<MysticetiConsensusHandler>,
}

impl MysticetiManager {
    /// NOTE: Mysticeti protocol key uses Ed25519 instead of BLS.
    /// But for security, the protocol keypair must be different from the network keypair.
    pub fn new(
        keypair: AuthorityKeyPair,
        _network_keypair: NetworkKeyPair,
        storage_base_path: PathBuf,
        metrics: ConsensusManagerMetrics,
        registry_service: RegistryService,
        client: Arc<LazyMysticetiClient>,
    ) -> Self {
        Self {
            keypair,
            _network_keypair,
            storage_base_path,
            running: Mutex::new(Running::False),
            metrics,
            registry_service,
            validator: ArcSwapOption::empty(),
            client,
            consensus_handler: ArcSwapOption::empty(),
        }
    }

    #[allow(unused)]
    fn get_store_path(&self, epoch: EpochId) -> PathBuf {
        let mut store_path = self.storage_base_path.clone();
        store_path.push(format!("{}", epoch));
        store_path
    }
}

#[async_trait]
impl ConsensusManagerTrait for MysticetiManager {
    async fn start(
        &self,
        _config: &NodeConfig,
        epoch_store: Arc<AuthorityPerEpochStore>,
        consensus_handler_initializer: ConsensusHandlerInitializer,
        tx_validator: SuiTxValidator,
    ) {
        let system_state = epoch_store.epoch_start_state();
        let committee: narwhal_config::Committee = system_state.get_narwhal_committee();
        let epoch = epoch_store.epoch();
        let protocol_config = epoch_store.protocol_config();
        let _network_type = match std::env::var("CONSENSUS_NETWORK") {
            Ok(type_str) => {
                if type_str.to_lowercase() == "tonic" {
                    NetworkType::Tonic
                } else {
                    NetworkType::Anemo
                }
            }
            Err(_) => NetworkType::Anemo,
        };

        let Some(_guard) = RunningLockGuard::acquire_start(
            &self.metrics,
            &self.running,
            epoch,
            protocol_config.version,
        )
        .await
        else {
            return;
        };

        let parameters = mysticeti_parameters(&committee);
        let committee = mysticeti_committee(&committee);

        let name: AuthorityName = self.keypair.public().into();
        let authority_index: AuthorityIndex = epoch_store
            .committee()
            .authority_index(&name)
            .unwrap()
            .into();
        let config = PrivateConfig::new(self.get_store_path(epoch), authority_index);

        let registry = Registry::new_custom(Some("consensus".to_string()), None).unwrap();

        const MAX_RETRIES: u32 = 2;
        let mut retries = 0;

        loop {
            // TODO: that should be replaced by a metered channel. We can discuss if unbounded approach
            // is the one we want to go with.
            #[allow(clippy::disallowed_methods)]
            let (commit_sender, commit_receiver) = unbounded_channel();

            let consensus_handler = consensus_handler_initializer.new_consensus_handler();
            let consumer = CommitConsumer::new(
                commit_sender,
                consensus_handler.last_executed_sub_dag_index(),
            );

            match Validator::start_production(
                authority_index,
                committee.clone(),
                &parameters,
                config.clone(),
                registry.clone(),
                Signer(Box::new(self.keypair.copy())),
                consumer,
                tx_validator.clone(),
            )
            .await
            {
                Ok((validator, tx_sender)) => {
                    let registry_id = self.registry_service.add(registry);

                    self.validator
                        .swap(Some(Arc::new((validator, registry_id))));

                    // create the client to send transactions to Mysticeti and update it.
                    self.client.set(MysticetiClient::new(tx_sender));

                    // spin up the new mysticeti consensus handler to listen for committed sub dags
                    let handler =
                        MysticetiConsensusHandler::new(consensus_handler, commit_receiver);
                    self.consensus_handler.store(Some(Arc::new(handler)));

                    break;
                }
                Err(err) => {
                    retries += 1;

                    if retries >= MAX_RETRIES {
                        panic!(
                            "Failed starting Mysticeti, maxed out retries {}: {:?}",
                            retries, err
                        );
                    }

                    tracing::error!("Failed starting Mysticeti, retry {}: {:?}", retries, err);
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            }
        }
    }

    async fn shutdown(&self) {
        let Some(_guard) = RunningLockGuard::acquire_shutdown(&self.metrics, &self.running).await
        else {
            return;
        };

        // swap with empty to ensure there is no other reference to authority and we can safely do Arc unwrap
        let r = self.validator.swap(None).unwrap();
        let Ok((authority, registry_id)) = Arc::try_unwrap(r) else {
            panic!("Failed to retrieve the mysticeti authority");
        };

        // shutdown the authority and wait for it
        authority.stop().await;

        // drop the old consensus handler to force stop any underlying task running.
        self.consensus_handler.store(None);

        // unregister the registry id
        self.registry_service.remove(registry_id);
    }

    async fn is_running(&self) -> bool {
        Running::False != *self.running.lock().await
    }

    fn get_storage_base_path(&self) -> PathBuf {
        self.storage_base_path.clone()
    }
}

fn mysticeti_committee(committee: &narwhal_config::Committee) -> Arc<Committee> {
    let authorities = committee
        .authorities()
        .map(|authority| {
            Authority::new(
                authority.stake(),
                PublicKey(
                    BLS12381PublicKey::from_bytes(authority.protocol_key().as_bytes()).unwrap(),
                ),
                authority.hostname().to_string(),
            )
        })
        .collect_vec();
    Committee::new(authorities.clone(), committee.epoch())
}

fn mysticeti_parameters(committee: &narwhal_config::Committee) -> Parameters {
    let identifiers = committee
        .authorities()
        .map(|authority| {
            // By converting first to anemo address it ensures that best effort parsing is done
            // to extract ip & port irrespective of the dictated protocol.
            let addr = authority.primary_address().to_anemo_address().unwrap();
            let network_address = addr.to_socket_addrs().unwrap().collect_vec().pop().unwrap();

            Identifier {
                public_key: PublicKey(
                    BLS12381PublicKey::from_bytes(authority.protocol_key().as_bytes()).unwrap(),
                ),
                network_address,
                metrics_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0), // not relevant as it won't be used
            }
        })
        .collect_vec();

    //TODO: for now fallback to default parameters - will read from properties
    Parameters {
        identifiers,
        enable_pipelining: true,
        leader_timeout: Duration::from_millis(250),
        enable_cleanup: true,
        synchronizer_parameters: SynchronizerParameters {
            sample_precision: Duration::from_millis(300),
            batch_size: 20,
            ..Default::default()
        },
        ..Default::default()
    }
}
