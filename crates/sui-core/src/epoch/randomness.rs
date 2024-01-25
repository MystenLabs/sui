// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::sync::{Arc, Weak};

use fastcrypto::encoding::{Encoding, Hex};
use fastcrypto::groups::bls12381;
use fastcrypto::serde_helpers::ToFromByteArray;
use fastcrypto::traits::{KeyPair, ToFromBytes};
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
    epoch_store: Weak<AuthorityPerEpochStore>,
    consensus_adapter: Arc<ConsensusAdapter>,
    // TODO: metrics

    // State for DKG.
    party: dkg::Party<PkG, EncG>,
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
        let manager = Self {
            epoch_store: epoch_store_weak,
            consensus_adapter,
            party,
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
            // metrics
            //     .state_handler_random_beacon_dkg_num_shares
            //     .set(dkg_output.shares.as_ref().map_or(0, |shares| shares.len()) as i64);
            manager
                .dkg_output
                .set(dkg_output)
                .expect("setting new OnceCell should succeed");
        } else {
            info!(
                "random beacon: no existing DKG output found for epoch {}",
                committee.epoch()
            );
        }

        // metrics
        //     .state_handler_current_randomness_round
        //     .set(store.randomness_round().0 as i64);
        Some(manager)
    }

    pub fn start_dkg(&self) -> SuiResult<()> {
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

        let mut batch = self.tables()?.dkg_processed_messages.batch();
        self.add_message(&mut batch, msg)?;
        batch.write()?;
        Ok(())
    }

    pub fn advance_dkg(&self, batch: &mut DBBatch) -> SuiResult<()> {
        // Once we have enough ProcessedMessages, send a Confirmation.
        if !self
            .tables()?
            .dkg_used_messages
            .contains_key(&SINGLETON_KEY)
            .expect("typed_store should not fail")
        {
            match self.party.merge(
                &self
                    .tables()?
                    .dkg_processed_messages
                    .safe_iter()
                    .map(|result| result.expect("typed_store should not fail").1)
                    .collect::<Vec<_>>(),
            ) {
                Ok((conf, used_msgs)) => {
                    info!(
                        "random beacon: sending DKG Confirmation with {} complaints",
                        conf.complaints.len()
                    );
                    batch.insert_batch(
                        &self.tables()?.dkg_used_messages,
                        std::iter::once((SINGLETON_KEY, used_msgs)),
                    )?;

                    let epoch_store = self.epoch_store()?;
                    let transaction = ConsensusTransaction::new_randomness_dkg_confirmation(
                        epoch_store.name,
                        &conf,
                    );
                    self.consensus_adapter
                        .submit(transaction, None, &epoch_store)?;
                    self.add_confirmation(batch, conf)?;
                }
                Err(fastcrypto::error::FastCryptoError::NotEnoughInputs) => (), // wait for more input
                Err(e) => debug!("random beacon: error while merging DKG Messages: {e:?}"),
            }
        }

        // Once we have enough Confirmations, process them and update shares.
        if !self.dkg_output.initialized()
            && self
                .tables()?
                .dkg_used_messages
                .contains_key(&SINGLETON_KEY)
                .expect("typed_store should not fail")
        {
            match self.party.complete(
                self.tables()?
                    .dkg_used_messages
                    .get(&SINGLETON_KEY)
                    .expect("typed_store should not fail")
                    .as_ref()
                    .expect("existence checked above"),
                &self
                    .tables()?
                    .dkg_confirmations
                    .safe_iter()
                    .map(|result| result.expect("typed_store should not fail").1)
                    .collect::<Vec<_>>(),
                self.party.t() * 2 - 1, // t==f+1, we want 2f+1
                &mut rand::thread_rng(),
            ) {
                Ok(output) => {
                    let num_shares = output.shares.as_ref().map_or(0, |shares| shares.len());
                    info!("random beacon: DKG complete with {num_shares} shares for this node");
                    batch.insert_batch(
                        &self.tables()?.dkg_output,
                        std::iter::once((SINGLETON_KEY, output.clone())),
                    )?;
                    self.dkg_output
                        .set(output)
                        .expect("checked above that `dkg_output` is uninitialized");
                    // self.metrics
                    //     .state_handler_random_beacon_dkg_num_shares
                    //     .set(output.shares.as_ref().map_or(0, |shares| shares.len()) as i64);
                    // if let Err(e) = self.vss_key_output.set(output.vss_pk.clone()) {
                    //     error!("random beacon: unable to write VSS key to output: {e:?}")
                    // }
                }
                Err(fastcrypto::error::FastCryptoError::NotEnoughInputs) => (), // wait for more input
                Err(e) => error!("random beacon: error while processing DKG Confirmations: {e:?}"),
            }
            // TODO: Begin randomness generation, once implemented.
        }

        Ok(())
    }

    pub fn add_message(&self, batch: &mut DBBatch, msg: dkg::Message<PkG, EncG>) -> SuiResult<()> {
        if self
            .tables()?
            .dkg_used_messages
            .contains_key(&SINGLETON_KEY)?
        {
            // We've already sent a `Confirmation`, so we can't add any more messages.
            return Ok(());
        }
        match self.party.process_message(msg, &mut rand::thread_rng()) {
            Ok(processed) => {
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
        &self,
        batch: &mut DBBatch,
        conf: dkg::Confirmation<EncG>,
    ) -> SuiResult<()> {
        if self
            .tables()?
            .dkg_used_messages
            .contains_key(&SINGLETON_KEY)?
        {
            // We should never see a `Confirmation` before we've sent our `Message` because
            // DKG messages are processed in consensus order.
            return Ok(());
        }
        if self.dkg_output.initialized() {
            // Once we have completed DKG, no more `Confirmation`s are needed.
            return Ok(());
        }
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
