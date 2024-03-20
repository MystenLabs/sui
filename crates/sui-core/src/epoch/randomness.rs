use std::collections::BTreeMap;
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::sync::{Arc, Mutex, Weak};

use fastcrypto::encoding::{Encoding, Hex};
use fastcrypto::groups::bls12381;
use fastcrypto::serde_helpers::ToFromByteArray;
use fastcrypto::traits::{KeyPair, ToFromBytes};
use fastcrypto_tbls::nodes::PartyId;
use fastcrypto_tbls::{dkg, nodes};
use sui_types::base_types::AuthorityName;
use sui_types::committee::{Committee, StakeUnit};
use sui_types::crypto::AuthorityKeyPair;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages_consensus::ConsensusTransaction;
use tokio::sync::OnceCell;
use tracing::{debug, error, info};
use typed_store::rocks::DBBatch;
use typed_store::Map;

use crate::authority::authority_per_epoch_store::{AuthorityEpochTables, AuthorityPerEpochStore};
use crate::consensus_adapter::ConsensusAdapter;

type PkG = bls12381::G2Element;
type EncG = bls12381::G2Element;

const SINGLETON_KEY: u64 = 0;

// State machine for randomness DKG and generation.
//
// DKG protocol:
// 1. This node sends out a `Message` to all other nodes.
// 2. Once sufficient valid `Message`s are received from other nodes via consensus and processed,
//    this node sends out a `Confirmation` to all other nodes.
// 3. Once sufficient `Confirmation`s are received from other nodes via consensus and processed,
//    they are combined to form a public VSS key and local private key shares.
// 4. Randomness generation begins.
//
// Randomness generation:
// TODO: document when implemented.
pub struct RandomnessManager {
    inner: Mutex<Inner>,
}

pub struct Inner {
    epoch_store: Weak<AuthorityPerEpochStore>,
    consensus_adapter: Arc<ConsensusAdapter>,

    // State for DKG.
    party: dkg::Party<PkG, EncG>,
    processed_messages: BTreeMap<PartyId, dkg::ProcessedMessage<PkG, EncG>>,
    used_messages: OnceCell<dkg::UsedProcessedMessages<PkG, EncG>>,
    confirmations: BTreeMap<PartyId, dkg::Confirmation<EncG>>,
    dkg_output: OnceCell<dkg::Output<PkG, EncG>>,
}

impl RandomnessManager {
    // Returns None in case of invalid input or other failure to initialize DKG.
    pub fn try_new(
        epoch_store_weak: Weak<AuthorityPerEpochStore>,
        consensus_adapter: Arc<ConsensusAdapter>,
        authority_key_pair: &AuthorityKeyPair,
    ) -> Option<Self> {
        let epoch_store = match epoch_store_weak.upgrade() {
            Some(epoch_store) => epoch_store,
            None => {
                error!(
                    "could not construct RandomnessManager: AuthorityPerEpochStore already gone"
                );
                return None;
            }
        };
        let tables = match epoch_store.tables() {
            Ok(tables) => tables,
            Err(_) => {
                error!("could not construct RandomnessManager: AuthorityPerEpochStore tables already gone");
                return None;
            }
        };
        let protocol_config = epoch_store.protocol_config();

        let name: AuthorityName = authority_key_pair.public().into();
        let committee = epoch_store.committee();
        let info = RandomnessManager::randomness_dkg_info_from_committee(committee);
        if tracing::enabled!(tracing::Level::DEBUG) {
            // Log first few entries in DKG info for debugging.
            for (id, pk, stake) in info.iter().filter(|(id, _, _)| *id < 3) {
                let pk_bytes = pk.as_element().to_byte_array();
                debug!("random beacon: DKG info: id={id}, stake={stake}, pk={pk_bytes:x?}");
            }
        }
        let nodes = info
            .iter()
            .map(|(id, pk, stake)| nodes::Node::<EncG> {
                id: *id,
                pk: pk.clone(),
                weight: (*stake).try_into().expect("stake should fit in u16"),
            })
            .collect();
        let nodes = match nodes::Nodes::new(nodes) {
            Ok(nodes) => nodes,
            Err(err) => {
                error!("random beacon: error while initializing Nodes: {err:?}");
                return None;
            }
        };
        let (nodes, t) = nodes.reduce(
            committee
                .validity_threshold()
                .try_into()
                .expect("validity threshold should fit in u16"),
            protocol_config.random_beacon_reduction_allowed_delta(),
        );
        let total_weight = nodes.total_weight();
        let num_nodes = nodes.num_nodes();
        let prefix_str = format!(
            "dkg {} {}",
            Hex::encode(epoch_store.get_chain_identifier().as_bytes()),
            committee.epoch()
        );
        let randomness_private_key = bls12381::Scalar::from_byte_array(
            authority_key_pair
                .copy()
                .private()
                .as_bytes()
                .try_into()
                .expect("key length should match"),
        )
        .expect("should work to convert BLS key to Scalar");
        let party = match dkg::Party::<PkG, EncG>::new(
            fastcrypto_tbls::ecies::PrivateKey::<bls12381::G2Element>::from(randomness_private_key),
            nodes,
            t.into(),
            fastcrypto_tbls::random_oracle::RandomOracle::new(prefix_str.as_str()),
            &mut rand::thread_rng(),
        ) {
            Ok(party) => party,
            Err(err) => {
                error!("random beacon: error while initializing Party: {err:?}");
                return None;
            }
        };
        info!(
            "random beacon: state initialized with authority={name}, total_weight={total_weight}, t={t}, num_nodes={num_nodes}, oracle initial_prefix={prefix_str:?}",
        );

        // Load existing data from store.
        let mut inner = Inner {
            epoch_store: epoch_store_weak,
            consensus_adapter,
            party,
            processed_messages: BTreeMap::new(),
            used_messages: OnceCell::new(),
            confirmations: BTreeMap::new(),
            dkg_output: OnceCell::new(),
        };
        let dkg_output = tables
            .dkg_output
            .get(&SINGLETON_KEY)
            .expect("typed_store should not fail");
        if let Some(dkg_output) = dkg_output {
            info!(
                "random beacon: loaded existing DKG output for epoch {}",
                committee.epoch()
            );
            epoch_store
                .metrics
                .epoch_random_beacon_dkg_num_shares
                .set(dkg_output.shares.as_ref().map_or(0, |shares| shares.len()) as i64);
            inner
                .dkg_output
                .set(dkg_output)
                .expect("setting new OnceCell should succeed");
        } else {
            info!(
                "random beacon: no existing DKG output found for epoch {}",
                committee.epoch()
            );
            // Load intermediate data.
            inner.processed_messages.extend(
                tables
                    .dkg_processed_messages
                    .safe_iter()
                    .map(|result| result.expect("typed_store should not fail")),
            );
            if let Some(used_messages) = tables
                .dkg_used_messages
                .get(&SINGLETON_KEY)
                .expect("typed_store should not fail")
            {
                inner
                    .used_messages
                    .set(used_messages.clone())
                    .expect("setting new OnceCell should succeed");
            }
            inner.confirmations.extend(
                tables
                    .dkg_confirmations
                    .safe_iter()
                    .map(|result| result.expect("typed_store should not fail")),
            );
        }

        Some(RandomnessManager {
            inner: Mutex::new(inner),
        })
    }

    pub fn start_dkg(&self) -> SuiResult {
        self.inner.lock().unwrap().start_dkg()
    }

    pub fn advance_dkg(&self, batch: &mut DBBatch) -> SuiResult {
        self.inner.lock().unwrap().advance_dkg(batch)
    }

    pub fn add_message(&self, batch: &mut DBBatch, msg: dkg::Message<PkG, EncG>) -> SuiResult {
        self.inner.lock().unwrap().add_message(batch, msg)
    }

    pub fn add_confirmation(
        &self,
        batch: &mut DBBatch,
        conf: dkg::Confirmation<EncG>,
    ) -> SuiResult {
        self.inner.lock().unwrap().add_confirmation(batch, conf)
    }

    fn randomness_dkg_info_from_committee(
        committee: &Committee,
    ) -> Vec<(
        u16,
        fastcrypto_tbls::ecies::PublicKey<bls12381::G2Element>,
        StakeUnit,
    )> {
        committee
            .members()
            .map(|(name, stake)| {
                let index: u16 = committee
                    .authority_index(name)
                    .expect("lookup of known committee member should succeed")
                    .try_into()
                    .expect("authority index should fit in u16");
                let pk = bls12381::G2Element::from_byte_array(
                    committee
                        .public_key(name)
                        .expect("lookup of known committee member should succeed")
                        .as_bytes()
                        .try_into()
                        .expect("key length should match"),
                )
                .expect("should work to convert BLS key to G2Element");
                (index, fastcrypto_tbls::ecies::PublicKey::from(pk), *stake)
            })
            .collect()
    }
}

impl Inner {
    pub fn start_dkg(&mut self) -> SuiResult {
        if self.used_messages.initialized() || self.dkg_output.initialized() {
            // DKG already started (or completed).
            return Ok(());
        }

        let msg = self.party.create_message(&mut rand::thread_rng());
        info!(
                "random beacon: created DKG Message with sender={}, vss_pk.degree={}, encrypted_shares.len()={}",
                msg.sender,
                msg.vss_pk.degree(),
                msg.encrypted_shares.len(),
            );

        let epoch_store = self.epoch_store()?;
        let transaction = ConsensusTransaction::new_randomness_dkg_message(epoch_store.name, &msg);
        self.consensus_adapter
            .submit(transaction, None, &epoch_store)?;
        Ok(())
    }

    pub fn advance_dkg(&mut self, batch: &mut DBBatch) -> SuiResult {
        let epoch_store = self.epoch_store()?;

        // Once we have enough ProcessedMessages, send a Confirmation.
        if !self.dkg_output.initialized() && !self.used_messages.initialized() {
            match self.party.merge(
                &self
                    .processed_messages
                    .values()
                    .cloned()
                    .collect::<Vec<_>>(),
            ) {
                Ok((conf, used_msgs)) => {
                    info!(
                        "random beacon: sending DKG Confirmation with {} complaints",
                        conf.complaints.len()
                    );
                    if self.used_messages.set(used_msgs.clone()).is_err() {
                        error!("BUG: used_messages should only ever be set once");
                    }
                    batch.insert_batch(
                        &self.tables()?.dkg_used_messages,
                        std::iter::once((SINGLETON_KEY, used_msgs)),
                    )?;

                    let transaction = ConsensusTransaction::new_randomness_dkg_confirmation(
                        epoch_store.name,
                        &conf,
                    );
                    self.consensus_adapter
                        .submit(transaction, None, &epoch_store)?;
                }
                Err(fastcrypto::error::FastCryptoError::NotEnoughInputs) => (), // wait for more input
                Err(e) => debug!("random beacon: error while merging DKG Messages: {e:?}"),
            }
        }

        // Once we have enough Confirmations, process them and update shares.
        if !self.dkg_output.initialized() && self.used_messages.initialized() {
            match self.party.complete(
                self.used_messages
                    .get()
                    .expect("checked above that `used_messages` is initialized"),
                &self.confirmations.values().cloned().collect::<Vec<_>>(),
                self.party.t() * 2 - 1, // t==f+1, we want 2f+1
                &mut rand::thread_rng(),
            ) {
                Ok(output) => {
                    let num_shares = output.shares.as_ref().map_or(0, |shares| shares.len());
                    info!("random beacon: DKG complete with {num_shares} shares for this node");
                    epoch_store
                        .metrics
                        .epoch_random_beacon_dkg_num_shares
                        .set(output.shares.as_ref().map_or(0, |shares| shares.len()) as i64);
                    self.dkg_output
                        .set(output.clone())
                        .expect("checked above that `dkg_output` is uninitialized");
                    batch.insert_batch(
                        &self.tables()?.dkg_output,
                        std::iter::once((SINGLETON_KEY, output)),
                    )?;
                }
                Err(fastcrypto::error::FastCryptoError::NotEnoughInputs) => (), // wait for more input
                Err(e) => error!("random beacon: error while processing DKG Confirmations: {e:?}"),
            }
            // TODO: Begin randomness generation, once implemented.
        }

        Ok(())
    }

    pub fn add_message(&mut self, batch: &mut DBBatch, msg: dkg::Message<PkG, EncG>) -> SuiResult {
        if self.used_messages.initialized() || self.dkg_output.initialized() {
            // We've already sent a `Confirmation`, so we can't add any more messages.
            return Ok(());
        }
        match self.party.process_message(msg, &mut rand::thread_rng()) {
            Ok(processed) => {
                self.processed_messages
                    .insert(processed.message.sender, processed.clone());
                batch.insert_batch(
                    &self.tables()?.dkg_processed_messages,
                    std::iter::once((processed.message.sender, processed)),
                )?;
            }
            Err(err) => {
                debug!("random beacon: error while processing DKG Message: {err:?}");
            }
        }
        Ok(())
    }

    pub fn add_confirmation(
        &mut self,
        batch: &mut DBBatch,
        conf: dkg::Confirmation<EncG>,
    ) -> SuiResult {
        if self.dkg_output.initialized() {
            // Once we have completed DKG, no more `Confirmation`s are needed.
            return Ok(());
        }
        self.confirmations.insert(conf.sender, conf.clone());
        batch.insert_batch(
            &self.tables()?.dkg_confirmations,
            std::iter::once((conf.sender, conf)),
        )?;
        Ok(())
    }

    fn epoch_store(&self) -> SuiResult<Arc<AuthorityPerEpochStore>> {
        self.epoch_store.upgrade().ok_or(SuiError::EpochEnded)
    }

    fn tables(&self) -> SuiResult<Arc<AuthorityEpochTables>> {
        self.epoch_store()?.tables()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        authority::test_authority_builder::TestAuthorityBuilder,
        consensus_adapter::{
            ConnectionMonitorStatusForTests, ConsensusAdapter, ConsensusAdapterMetrics,
            MockSubmitToConsensus,
        },
        epoch::randomness::*,
    };
    use std::num::NonZeroUsize;
    use sui_types::messages_consensus::ConsensusTransactionKind;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_dkg() {
        telemetry_subscribers::init_for_testing();

        let network_config =
            sui_swarm_config::network_config_builder::ConfigBuilder::new_with_temp_dir()
                .committee_size(NonZeroUsize::new(4).unwrap())
                .with_reference_gas_price(500)
                .build();

        let mut epoch_stores = Vec::new();
        let mut randomness_managers = Vec::new();
        let (tx_consensus, mut rx_consensus) = mpsc::channel(100);

        for validator in network_config.validator_configs.iter() {
            // Send consensus messages to channel.
            let mut mock_consensus_client = MockSubmitToConsensus::new();
            let tx_consensus = tx_consensus.clone();
            mock_consensus_client
                .expect_submit_to_consensus()
                .withf(move |transaction: &ConsensusTransaction, _epoch_store| {
                    tx_consensus.try_send(transaction.clone()).unwrap();
                    true
                })
                .returning(|_, _| Ok(()));

            let state = TestAuthorityBuilder::new()
                .with_genesis_and_keypair(&network_config.genesis, validator.protocol_key_pair())
                .build()
                .await;
            let consensus_adapter = Arc::new(ConsensusAdapter::new(
                Arc::new(mock_consensus_client),
                state.name,
                Arc::new(ConnectionMonitorStatusForTests {}),
                100_000,
                100_000,
                None,
                None,
                ConsensusAdapterMetrics::new_test(),
                state.epoch_store_for_testing().protocol_config().clone(),
            ));
            let epoch_store = state.epoch_store_for_testing();
            let randomness_manager = RandomnessManager::try_new(
                Arc::downgrade(&epoch_store),
                consensus_adapter.clone(),
                validator.protocol_key_pair(),
            )
            .unwrap();

            epoch_stores.push(epoch_store);
            randomness_managers.push(randomness_manager);
        }

        // Generate and distribute Messages.
        let mut dkg_messages = Vec::new();
        for randomness_manager in &randomness_managers {
            randomness_manager.start_dkg().unwrap();

            let dkg_message = rx_consensus.recv().await.unwrap();
            match dkg_message.kind {
                ConsensusTransactionKind::RandomnessDkgMessage(_, bytes) => {
                    let msg: fastcrypto_tbls::dkg::Message<PkG, EncG> = bcs::from_bytes(&bytes)
                        .expect("DKG message deserialization should not fail");
                    dkg_messages.push(msg);
                }
                _ => panic!("wrong type of message sent"),
            }
        }
        for i in 0..randomness_managers.len() {
            let mut batch = epoch_stores[i]
                .tables()
                .unwrap()
                .dkg_processed_messages
                .batch();
            for dkg_message in dkg_messages.iter().cloned() {
                randomness_managers[i]
                    .add_message(&mut batch, dkg_message)
                    .unwrap();
            }
            randomness_managers[i].advance_dkg(&mut batch).unwrap();
            batch.write().unwrap();
        }

        // Generate and distribute Confirmations.
        let mut dkg_confirmations = Vec::new();
        for _ in 0..randomness_managers.len() {
            let dkg_confirmation = rx_consensus.recv().await.unwrap();
            match dkg_confirmation.kind {
                ConsensusTransactionKind::RandomnessDkgConfirmation(_, bytes) => {
                    let msg: fastcrypto_tbls::dkg::Confirmation<EncG> = bcs::from_bytes(&bytes)
                        .expect("DKG confirmation deserialization should not fail");
                    dkg_confirmations.push(msg);
                }
                _ => panic!("wrong type of message sent"),
            }
        }
        for i in 0..randomness_managers.len() {
            let mut batch = epoch_stores[i].tables().unwrap().dkg_confirmations.batch();
            for dkg_confirmation in dkg_confirmations.iter().cloned() {
                randomness_managers[i]
                    .add_confirmation(&mut batch, dkg_confirmation)
                    .unwrap();
            }
            randomness_managers[i].advance_dkg(&mut batch).unwrap();
            batch.write().unwrap();
        }

        // Verify DKG completed.
        for randomness_manager in &randomness_managers {
            assert!(randomness_manager
                .inner
                .lock()
                .unwrap()
                .dkg_output
                .initialized());
        }
    }
}
