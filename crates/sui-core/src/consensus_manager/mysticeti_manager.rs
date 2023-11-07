// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::consensus_handler::ConsensusHandlerInitializer;
use crate::consensus_manager::{
    ConsensusManagerMetrics, ConsensusManagerTrait, Running, RunningLockGuard,
};
use crate::consensus_validator::SuiTxValidator;
use arc_swap::ArcSwapOption;
use async_trait::async_trait;
use fastcrypto::traits::KeyPair;
use itertools::Itertools;
use mysten_metrics::{RegistryID, RegistryService};
use mysticeti_core::committee::{Authority, Committee};
use mysticeti_core::config::{Identifier, Parameters, PrivateConfig};
use mysticeti_core::types::AuthorityIndex;
use mysticeti_core::validator::Validator;
use mysticeti_core::{PublicKey, Signer};
use prometheus::Registry;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::Arc;
use sui_config::NodeConfig;
use sui_types::base_types::AuthorityName;
use sui_types::committee::EpochId;
use sui_types::crypto::{AuthorityKeyPair, NetworkKeyPair};
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use tokio::sync::Mutex;

#[cfg(test)]
#[path = "../unit_tests/mysticeti_manager_tests.rs"]
pub mod mysticeti_manager_tests;

pub struct MysticetiManager {
    keypair: AuthorityKeyPair,
    network_keypair: NetworkKeyPair,
    storage_base_path: PathBuf,
    running: Mutex<Running>,
    metrics: ConsensusManagerMetrics,
    registry_service: RegistryService,
    validator: ArcSwapOption<(Validator, RegistryID)>,
}

impl MysticetiManager {
    pub fn new(
        keypair: AuthorityKeyPair,
        network_keypair: NetworkKeyPair,
        storage_base_path: PathBuf,
        metrics: ConsensusManagerMetrics,
        registry_service: RegistryService,
    ) -> MysticetiManager {
        Self {
            keypair,
            network_keypair,
            storage_base_path,
            running: Mutex::new(Running::False),
            metrics,
            registry_service,
            validator: ArcSwapOption::empty(),
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
    #[allow(unused)]
    async fn start(
        &self,
        config: &NodeConfig,
        epoch_store: Arc<AuthorityPerEpochStore>,
        consensus_handler_initializer: ConsensusHandlerInitializer,
        tx_validator: SuiTxValidator,
    ) {
        let system_state = epoch_store.epoch_start_state();
        let committee: narwhal_config::Committee = system_state.get_narwhal_committee();
        let epoch = epoch_store.epoch();
        let protocol_config = epoch_store.protocol_config();

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

        let registry = Registry::new_custom(Some("mysticeti_".to_string()), None).unwrap();

        const MAX_RETRIES: u32 = 2;
        let mut retries = 0;

        loop {
            let private_key = self.network_keypair.copy().private();

            match Validator::start(
                authority_index,
                committee.clone(),
                &parameters,
                config.clone(),
                Some(registry.clone()),
                Signer(Box::new(private_key.0.clone())),
            )
            .await
            {
                Ok(validator) => {
                    let registry_id = self.registry_service.add(registry);

                    self.validator
                        .swap(Some(Arc::new((validator, registry_id))));

                    break;
                }
                Err(err) => {
                    retries += 1;

                    self.metrics.start_mysticeti_retries.set(retries as i64);

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

        // swap with empty to ensure there is no other reference to validator and we can safely do Arc unwrap
        let r = self.validator.swap(None).unwrap();
        let Ok((validator, registry_id)) = Arc::try_unwrap(r) else {
            panic!("Failed to retrieve the mysticeti validator");
        };

        // shutdown the validator and wait for it
        validator.stop().await;

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
            // TODO: using the  Ed25519 network key which is compatible with Mysticeti which also uses Ed25519. Should
            // switch to using the authority's protocol key (BLS) instead.
            Authority::new(authority.stake(), PublicKey(authority.network_key().0))
        })
        .collect_vec();
    Committee::new(authorities.clone())
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
                // TODO: using the  Ed25519 network key which is compatible with Mysticeti which also uses Ed25519. Should
                // switch to using the authority's protocol key (BLS) instead.
                public_key: PublicKey(authority.network_key().0),
                network_address,
                metrics_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0), // not relevant as it won't be used
            }
        })
        .collect_vec();

    //TODO: for now fallback to default parameters - will read from properties
    Parameters {
        identifiers,
        ..Default::default()
    }
}
