// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anemo::PeerId;
use fastcrypto::encoding::{Encoding, Hex};
use fastcrypto::error::{FastCryptoError, FastCryptoResult};
use fastcrypto::groups::bls12381;
use fastcrypto::serde_helpers::ToFromByteArray;
use fastcrypto::traits::{KeyPair, ToFromBytes};
use fastcrypto_tbls::{dkg_v1, nodes, nodes::PartyId};
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use mysten_common::debug_fatal;
use parking_lot::Mutex;
use rand::SeedableRng;
use rand::rngs::{OsRng, StdRng};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Weak};
use std::time::Instant;
use sui_macros::fail_point_if;
use sui_network::randomness;
use sui_types::base_types::AuthorityName;
use sui_types::committee::{Committee, EpochId, StakeUnit};
use sui_types::crypto::{AuthorityKeyPair, RandomnessRound};
use sui_types::error::{SuiErrorKind, SuiResult};
use sui_types::messages_consensus::{
    ConsensusTransaction, Round, TimestampMs, VersionedDkgConfirmation, VersionedDkgMessage,
};
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use tokio::sync::OnceCell;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use typed_store::Map;

use crate::authority::authority_per_epoch_store::{
    AuthorityPerEpochStore, consensus_quarantine::ConsensusCommitOutput,
};
use crate::authority::epoch_start_configuration::EpochStartConfigTrait;
use crate::consensus_adapter::SubmitToConsensus;

type PkG = bls12381::G2Element;
type EncG = bls12381::G2Element;

pub const SINGLETON_KEY: u64 = 0;

/// DkgParticipant abstracts over Party (for validators) and Observer (for observer nodes).
/// This allows both types of nodes to participate in the DKG protocol, with observers
/// only tracking state without generating keys or signatures.
#[derive(Clone)]
enum DkgParticipant {
    /// A validator that actively participates in DKG with their private key
    Party(Arc<dkg_v1::Party<PkG, EncG>>),
    /// An observer that tracks DKG state without a private key
    Observer(Arc<dkg_v1::Observer<PkG, EncG>>),
}

impl DkgParticipant {
    /// Create initial DKG message - only works for Party (validators)
    pub fn create_message(
        &self,
        rng: &mut impl fastcrypto::traits::AllowedRng,
    ) -> FastCryptoResult<dkg_v1::Message<PkG, EncG>> {
        match self {
            DkgParticipant::Party(party) => party.create_message(rng),
            DkgParticipant::Observer(_) => Err(FastCryptoError::InvalidInput),
        }
    }

    /// Get the threshold value
    pub fn t(&self) -> u16 {
        match self {
            DkgParticipant::Party(party) => party.t(),
            DkgParticipant::Observer(observer) => observer.t(),
        }
    }

    /// Get party ID (if this is a validator)
    pub fn party_id(&self) -> Option<PartyId> {
        match self {
            DkgParticipant::Party(party) => Some(party.id),
            DkgParticipant::Observer(_) => None,
        }
    }

    /// Returns the Party reference if this is a validator participant
    fn as_party(&self) -> Option<&Arc<dkg_v1::Party<PkG, EncG>>> {
        match self {
            DkgParticipant::Party(party) => Some(party),
            DkgParticipant::Observer(_) => None,
        }
    }

    /// Returns the Observer reference if this is an observer participant
    fn as_observer(&self) -> Option<&Arc<dkg_v1::Observer<PkG, EncG>>> {
        match self {
            DkgParticipant::Observer(observer) => Some(observer),
            DkgParticipant::Party(_) => None,
        }
    }
}

// Wrappers for DKG messages (to simplify upgrades).

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum VersionedProcessedMessage {
    V0(), // deprecated
    V1(dkg_v1::ProcessedMessage<PkG, EncG>),
}

impl VersionedProcessedMessage {
    pub fn sender(&self) -> PartyId {
        match self {
            VersionedProcessedMessage::V0() => {
                panic!("BUG: invalid VersionedProcessedMessage version V0")
            }
            VersionedProcessedMessage::V1(msg) => msg.message.sender,
        }
    }

    pub fn unwrap_v1(self) -> dkg_v1::ProcessedMessage<PkG, EncG> {
        if let VersionedProcessedMessage::V1(msg) = self {
            msg
        } else {
            panic!("BUG: expected message version is 1")
        }
    }

    pub fn process(
        party: Arc<dkg_v1::Party<PkG, EncG>>,
        message: VersionedDkgMessage,
    ) -> FastCryptoResult<VersionedProcessedMessage> {
        // All inputs are verified in add_message, so we can assume they are of the correct version.
        let processed = party.process_message(message.unwrap_v1(), &mut rand::thread_rng())?;
        Ok(VersionedProcessedMessage::V1(processed))
    }

    pub fn merge(
        party: Arc<dkg_v1::Party<PkG, EncG>>,
        messages: Vec<Self>,
    ) -> FastCryptoResult<(VersionedDkgConfirmation, VersionedUsedProcessedMessages)> {
        // All inputs were created by this validator, so we can assume they are of the correct version.
        let (conf, msgs) = party.merge(
            &messages
                .into_iter()
                .map(|vm| vm.unwrap_v1())
                .collect::<Vec<_>>(),
        )?;
        Ok((
            VersionedDkgConfirmation::V1(conf),
            VersionedUsedProcessedMessages::V1(msgs),
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionedUsedProcessedMessages {
    V0(), // deprecated
    V1(dkg_v1::UsedProcessedMessages<PkG, EncG>),
}

// State machine for randomness DKG and generation.
//
// DKG protocol:
// 1. This validator sends out a `Message` to all other validators.
// 2. Once sufficient valid `Message`s are received from other validators via consensus and
//    processed, this validator sends out a `Confirmation` to all other validators.
// 3. Once sufficient `Confirmation`s are received from other validators via consensus and
//    processed, they are combined to form a public VSS key and local private key shares.
// 4. Randomness generation begins.
//
// Randomness generation:
// 1. For each new round, AuthorityPerEpochStore eventually calls `generate_randomness`.
// 2. This kicks off a process in RandomnessEventLoop to send partial signatures for the new
//    round to all other validators.
// 3. Once enough partial signautres for the round are collected, a RandomnessStateUpdate
//    transaction is generated and injected into the ExecutionScheduler.
// 4. Once the RandomnessStateUpdate transaction is seen in a certified checkpoint,
//    `notify_randomness_in_checkpoint` is called to complete the round and stop sending
//    partial signatures for it.
pub struct RandomnessManager {
    epoch_store: Weak<AuthorityPerEpochStore>,
    epoch: EpochId,
    consensus_adapter: Box<dyn SubmitToConsensus>,
    network_handle: randomness::Handle,
    authority_info: HashMap<AuthorityName, (PeerId, PartyId)>,

    // State for DKG.
    dkg_start_time: OnceCell<Instant>,
    participant: DkgParticipant,
    enqueued_messages: BTreeMap<PartyId, JoinHandle<Option<VersionedProcessedMessage>>>,
    processed_messages: BTreeMap<PartyId, VersionedProcessedMessage>,
    used_messages: OnceCell<VersionedUsedProcessedMessages>,
    confirmations: BTreeMap<PartyId, VersionedDkgConfirmation>,
    dkg_output: OnceCell<Option<dkg_v1::Output<PkG, EncG>>>,

    // State for randomness generation.
    next_randomness_round: RandomnessRound,
    highest_completed_round: Arc<Mutex<Option<RandomnessRound>>>,
}

impl RandomnessManager {
    /// Finalizes DKG after successful completion, handling metrics and state updates
    fn finalize_dkg(
        &mut self,
        output: dkg_v1::Output<PkG, EncG>,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        consensus_output: &mut ConsensusCommitOutput,
        consensus_round: Round,
        log_prefix: &str,
    ) {
        let num_shares = output.shares.as_ref().map_or(0, |shares| shares.len());
        let epoch_elapsed = epoch_store.epoch_open_time.elapsed().as_millis();
        let elapsed = self.dkg_start_time.get().map(|t| t.elapsed().as_millis());

        info!(
            "random beacon: {} at consensus round {} in {epoch_elapsed}ms since epoch start, {elapsed:?}ms since DKG start, with {num_shares} shares for this node",
            log_prefix, consensus_round
        );

        // Set metrics
        epoch_store
            .metrics
            .epoch_random_beacon_dkg_num_shares
            .set(num_shares as i64);
        epoch_store
            .metrics
            .epoch_random_beacon_dkg_epoch_start_completion_time_ms
            .set(epoch_elapsed as i64);
        epoch_store.metrics.epoch_random_beacon_dkg_failed.set(0);
        if let Some(elapsed) = elapsed {
            epoch_store
                .metrics
                .epoch_random_beacon_dkg_completion_time_ms
                .set(elapsed as i64);
        }

        // Store DKG output
        self.dkg_output
            .set(Some(output.clone()))
            .expect("checked above that `dkg_output` is uninitialized");

        // Update network handle
        self.network_handle.update_epoch(
            epoch_store.committee().epoch(),
            self.authority_info.clone(),
            output.clone(),
            self.participant.t(),
            None,
        );

        // Set consensus output
        consensus_output.set_dkg_output(output);
    }

    // Returns None in case of invalid input or other failure to initialize DKG.
    pub async fn try_new(
        epoch_store_weak: Weak<AuthorityPerEpochStore>,
        consensus_adapter: Box<dyn SubmitToConsensus>,
        network_handle: randomness::Handle,
        authority_key_pair: Option<&AuthorityKeyPair>,
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
                error!(
                    "could not construct RandomnessManager: AuthorityPerEpochStore tables already gone"
                );
                return None;
            }
        };
        let protocol_config = epoch_store.protocol_config();

        let committee = epoch_store.committee();
        let info = RandomnessManager::randomness_dkg_info_from_committee(committee);
        if tracing::enabled!(tracing::Level::DEBUG) {
            // Log first few entries in DKG info for debugging.
            for (id, name, pk, stake) in info.iter().filter(|(id, _, _, _)| *id < 3) {
                let pk_bytes = pk.as_element().to_byte_array();
                debug!(
                    "random beacon: DKG info: id={id}, stake={stake}, name={name}, pk={pk_bytes:x?}"
                );
            }
        }
        let authority_ids: HashMap<_, _> =
            info.iter().map(|(id, name, _, _)| (*name, *id)).collect();
        let authority_peer_ids = epoch_store
            .epoch_start_config()
            .epoch_start_state()
            .get_authority_names_to_peer_ids();
        let authority_info = authority_ids
            .into_iter()
            .map(|(name, id)| {
                let peer_id = *authority_peer_ids
                    .get(&name)
                    .expect("authority name should be in peer_ids");
                (name, (peer_id, id))
            })
            .collect();
        let nodes = info
            .iter()
            .map(|(id, _, pk, stake)| nodes::Node::<EncG> {
                id: *id,
                pk: pk.clone(),
                weight: (*stake).try_into().expect("stake should fit in u16"),
            })
            .collect();
        let (nodes, t) = match nodes::Nodes::new_reduced(
            nodes,
            committee
                .validity_threshold()
                .try_into()
                .expect("validity threshold should fit in u16"),
            protocol_config.random_beacon_reduction_allowed_delta(),
            protocol_config
                .random_beacon_reduction_lower_bound()
                .try_into()
                .expect("should fit u16"),
        ) {
            Ok((nodes, t)) => (nodes, t),
            Err(err) => {
                error!("random beacon: error while initializing Nodes: {err:?}");
                return None;
            }
        };
        let total_weight = nodes.total_weight();
        let num_nodes = nodes.num_nodes();
        let prefix_str = format!(
            "dkg {} {}",
            Hex::encode(epoch_store.get_chain_identifier().as_bytes()),
            committee.epoch()
        );

        // Create either a Party (for validators) or Observer (for observer nodes)
        let participant = if let Some(authority_key_pair) = authority_key_pair {
            // Validator with authority keys - create a Party
            let name: AuthorityName = authority_key_pair.public().into();
            let randomness_private_key = bls12381::Scalar::from_byte_array(
                authority_key_pair
                    .copy()
                    .private()
                    .as_bytes()
                    .try_into()
                    .expect("key length should match"),
            )
            .expect("should work to convert BLS key to Scalar");

            let party = match dkg_v1::Party::<PkG, EncG>::new(
                fastcrypto_tbls::ecies_v1::PrivateKey::<bls12381::G2Element>::from(
                    randomness_private_key,
                ),
                nodes.clone(),
                t,
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
            DkgParticipant::Party(Arc::new(party))
        } else {
            // Observer node without authority keys - create an Observer
            let observer = match dkg_v1::Observer::<PkG, EncG>::new(
                nodes,
                t,
                fastcrypto_tbls::random_oracle::RandomOracle::new(prefix_str.as_str()),
            ) {
                Ok(observer) => observer,
                Err(err) => {
                    error!("random beacon: error while initializing Observer: {err:?}");
                    return None;
                }
            };
            info!(
                "random beacon: Observer state initialized with total_weight={total_weight}, t={t}, num_nodes={num_nodes}, oracle initial_prefix={prefix_str:?}",
            );
            DkgParticipant::Observer(Arc::new(observer))
        };

        // Load existing data from store.
        let highest_completed_round = tables
            .randomness_highest_completed_round
            .get(&SINGLETON_KEY)
            .expect("typed_store should not fail");
        let mut rm = RandomnessManager {
            epoch_store: epoch_store_weak,
            epoch: committee.epoch(),
            consensus_adapter,
            network_handle: network_handle.clone(),
            authority_info,
            dkg_start_time: OnceCell::new(),
            participant,
            enqueued_messages: BTreeMap::new(),
            processed_messages: BTreeMap::new(),
            used_messages: OnceCell::new(),
            confirmations: BTreeMap::new(),
            dkg_output: OnceCell::new(),
            next_randomness_round: RandomnessRound(0),
            highest_completed_round: Arc::new(Mutex::new(highest_completed_round)),
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
            rm.dkg_output
                .set(Some(dkg_output.clone()))
                .expect("setting new OnceCell should succeed");
            network_handle.update_epoch(
                committee.epoch(),
                rm.authority_info.clone(),
                dkg_output,
                rm.participant.t(),
                highest_completed_round,
            );
        } else {
            info!(
                "random beacon: no existing DKG output found for epoch {}",
                committee.epoch()
            );

            // Load intermediate data.
            assert!(
                epoch_store.protocol_config().dkg_version() > 0,
                "BUG: DKG version 0 is deprecated"
            );
            rm.processed_messages.extend(
                tables
                    .dkg_processed_messages_v2
                    .safe_iter()
                    .map(|result| result.expect("typed_store should not fail")),
            );
            // For observer nodes, replay persisted messages through the observer
            // to restore its internal state after a restart.
            if let Some(observer) = rm.participant.as_observer() {
                let mut replayed = 0;
                for msg in rm.processed_messages.values() {
                    let raw_msg = msg.clone().unwrap_v1().message;
                    if let Err(e) = observer.process_message(raw_msg) {
                        debug!(
                            "random beacon: Observer error replaying DKG message on recovery: {e:?}"
                        );
                    } else {
                        replayed += 1;
                    }
                }
                if replayed > 0 {
                    info!(
                        "random beacon: Observer replayed {replayed} DKG messages from persistent storage"
                    );
                }
            }
            if let Some(used_messages) = tables
                .dkg_used_messages_v2
                .get(&SINGLETON_KEY)
                .expect("typed_store should not fail")
            {
                rm.used_messages
                    .set(used_messages.clone())
                    .expect("setting new OnceCell should succeed");
            }
            rm.confirmations.extend(
                tables
                    .dkg_confirmations_v2
                    .safe_iter()
                    .map(|result| result.expect("typed_store should not fail")),
            );
        }

        // Resume randomness generation from where we left off.
        // This must be loaded regardless of whether DKG has finished yet, since the
        // RandomnessEventLoop and commit-handling logic in AuthorityPerEpochStore both depend on
        // this state.
        rm.next_randomness_round = tables
            .randomness_next_round
            .get(&SINGLETON_KEY)
            .expect("typed_store should not fail")
            .unwrap_or(RandomnessRound(0));
        info!(
            "random beacon: starting from next_randomness_round={}",
            rm.next_randomness_round.0
        );
        let first_incomplete_round = highest_completed_round
            .map(|r| r + 1)
            .unwrap_or(RandomnessRound(0));
        if first_incomplete_round < rm.next_randomness_round {
            info!(
                "random beacon: resuming generation for randomness rounds from {} to {}",
                first_incomplete_round,
                rm.next_randomness_round - 1,
            );
            for r in first_incomplete_round.0..rm.next_randomness_round.0 {
                network_handle.send_partial_signatures(committee.epoch(), RandomnessRound(r));
            }
        }

        Some(rm)
    }

    /// Sends the initial dkg::Message to begin the randomness DKG protocol.
    pub async fn start_dkg(&mut self) -> SuiResult {
        if self.used_messages.initialized() || self.dkg_output.initialized() {
            // DKG already started (or completed or failed).
            return Ok(());
        }

        let _ = self.dkg_start_time.set(Instant::now());

        let epoch_store = self.epoch_store()?;

        // Observer nodes don't create DKG messages - they only consume messages and
        // confirmations from validators to complete DKG.
        if self.participant.as_observer().is_some() {
            info!("random beacon: Observer waiting to replay messages from consensus");
            return Ok(());
        }
        let dkg_version = epoch_store.protocol_config().dkg_version();
        info!("random beacon: starting DKG, version {dkg_version}");

        let msg = match self.participant.create_message(&mut rand::thread_rng()) {
            Ok(msg) => VersionedDkgMessage::V1(msg),
            Err(FastCryptoError::IgnoredMessage) => {
                info!(
                    "random beacon: no DKG Message for party id={:?} (zero weight)",
                    self.participant.party_id()
                );
                return Ok(());
            }
            Err(e) => {
                error!("random beacon: error while creating a DKG Message: {e:?}");
                return Ok(());
            }
        };

        info!("random beacon: created {msg:?} with dkg version {dkg_version}");
        let transaction = ConsensusTransaction::new_randomness_dkg_message(epoch_store.name, &msg);

        #[allow(unused_mut)]
        let mut fail_point_skip_sending = false;
        fail_point_if!("rb-dkg", || {
            // maybe skip sending in simtests
            fail_point_skip_sending = true;
        });
        if !fail_point_skip_sending {
            self.consensus_adapter
                .submit_to_consensus(&[transaction], &epoch_store)?;
        }

        epoch_store
            .metrics
            .epoch_random_beacon_dkg_message_time_ms
            .set(
                self.dkg_start_time
                    .get()
                    .unwrap() // already set above
                    .elapsed()
                    .as_millis() as i64,
            );
        Ok(())
    }

    /// Processes all received messages and advances the randomness DKG state machine when possible,
    /// sending out a dkg::Confirmation and generating final output.
    pub(crate) async fn advance_dkg(
        &mut self,
        consensus_output: &mut ConsensusCommitOutput,
        round: Round,
    ) -> SuiResult {
        let epoch_store = self.epoch_store()?;

        debug!(
            "random beacon: advancing DKG at consensus round {}, DKG output initialized: {}, used messages initialized: {}",
            round,
            self.dkg_output.initialized(),
            self.used_messages.initialized()
        );

        if !self.dkg_output.initialized() {
            if self.participant.as_party().is_some() {
                self.advance_dkg_validator(&epoch_store, consensus_output, round)
                    .await?;
            } else {
                self.advance_dkg_observer(&epoch_store, consensus_output, round)
                    .await;
            }
        }

        // If we ran out of time, mark DKG as failed.
        if !self.dkg_output.initialized()
            && round
                > epoch_store
                    .protocol_config()
                    .random_beacon_dkg_timeout_round()
                    .into()
        {
            error!(
                "random beacon: DKG timed out. Randomness disabled for this epoch. All randomness-using transactions will fail."
            );
            epoch_store.metrics.epoch_random_beacon_dkg_failed.set(1);
            self.dkg_output
                .set(None)
                .expect("checked above that `dkg_output` is uninitialized");
        }

        Ok(())
    }

    /// Advances DKG for validator nodes: processes enqueued messages, generates a confirmation,
    /// and completes DKG once enough confirmations are received.
    async fn advance_dkg_validator(
        &mut self,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        consensus_output: &mut ConsensusCommitOutput,
        round: Round,
    ) -> SuiResult {
        let party = self
            .participant
            .as_party()
            .expect("caller verified this is a validator")
            .clone();

        // Phase 1: Generate and send a Confirmation once we have enough messages.
        if !self.used_messages.initialized() {
            // Process all enqueued messages.
            let mut handles: FuturesUnordered<_> = std::mem::take(&mut self.enqueued_messages)
                .into_values()
                .collect();
            while let Some(res) = handles.next().await {
                if let Ok(Some(processed)) = res {
                    self.processed_messages
                        .insert(processed.sender(), processed.clone());
                    consensus_output.insert_dkg_processed_message(processed);
                }
            }

            // Attempt to merge processed messages and generate the Confirmation.
            let messages_to_merge: Vec<_> = self
                .processed_messages
                .values()
                .map(|vm| vm.clone().unwrap_v1())
                .collect();

            match party.merge(&messages_to_merge) {
                Ok((conf, used_msgs)) => {
                    let versioned_conf = VersionedDkgConfirmation::V1(conf.clone());
                    let versioned_used_msgs = VersionedUsedProcessedMessages::V1(used_msgs.clone());

                    info!(
                        "random beacon: sending DKG Confirmation with {} complaints",
                        versioned_conf.num_of_complaints()
                    );
                    if self.used_messages.set(versioned_used_msgs.clone()).is_err() {
                        error!("BUG: used_messages should only ever be set once");
                    }
                    consensus_output.insert_dkg_used_messages(versioned_used_msgs);

                    let transaction = ConsensusTransaction::new_randomness_dkg_confirmation(
                        epoch_store.name,
                        &versioned_conf,
                    );

                    #[allow(unused_mut)]
                    let mut fail_point_skip_sending = false;
                    fail_point_if!("rb-dkg", || {
                        // maybe skip sending in simtests
                        fail_point_skip_sending = true;
                    });
                    if !fail_point_skip_sending {
                        self.consensus_adapter
                            .submit_to_consensus(&[transaction], epoch_store)?;
                    }

                    let elapsed = self.dkg_start_time.get().map(|t| t.elapsed().as_millis());
                    if let Some(elapsed) = elapsed {
                        epoch_store
                            .metrics
                            .epoch_random_beacon_dkg_confirmation_time_ms
                            .set(elapsed as i64);
                    }
                }
                Err(FastCryptoError::NotEnoughInputs) => (), // wait for more input
                Err(e) => debug!("random beacon: error while merging DKG Messages: {e:?}"),
            }
        }

        // Phase 2: Complete DKG once we have the confirmation and enough confirmations from others.
        if !self.dkg_output.initialized() && self.used_messages.initialized() {
            let rng = &mut StdRng::from_rng(OsRng).expect("RNG construction should not fail");
            let used_messages = self
                .used_messages
                .get()
                .expect("checked above that `used_messages` is initialized");

            let VersionedUsedProcessedMessages::V1(msg) = used_messages else {
                panic!("BUG: invalid VersionedUsedProcessedMessages version")
            };

            let confirmations: Vec<_> = self
                .confirmations
                .values()
                .map(|vm| vm.unwrap_v1())
                .cloned()
                .collect();

            match party.complete(msg, &confirmations, rng) {
                Ok(output) => {
                    self.finalize_dkg(output, epoch_store, consensus_output, round, "DKG complete");
                }
                Err(FastCryptoError::NotEnoughInputs) => (), // wait for more input
                Err(e) => {
                    error!("random beacon: error while processing DKG Confirmations: {e:?}")
                }
            }
        }

        Ok(())
    }

    /// Advances DKG for observer nodes: processes enqueued messages, and once enough messages
    /// and confirmations from validators are collected, merges and completes DKG to obtain
    /// the VSS public key (without shares).
    async fn advance_dkg_observer(
        &mut self,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        consensus_output: &mut ConsensusCommitOutput,
        round: Round,
    ) {
        let observer = self
            .participant
            .as_observer()
            .expect("caller verified this is an observer")
            .clone();

        // Process all enqueued messages.
        let mut handles: FuturesUnordered<_> = std::mem::take(&mut self.enqueued_messages)
            .into_values()
            .collect();
        while let Some(res) = handles.next().await {
            if let Ok(Some(processed)) = res {
                self.processed_messages
                    .insert(processed.sender(), processed.clone());
                consensus_output.insert_dkg_processed_message(processed);
            }
        }

        // Extract raw messages from the persisted processed_messages store.
        let messages: Vec<_> = self
            .processed_messages
            .values()
            .map(|vm| vm.clone().unwrap_v1().message)
            .collect();

        let confirmations: Vec<_> = self
            .confirmations
            .values()
            .map(|vm| vm.unwrap_v1())
            .cloned()
            .collect();

        // Merge raw messages, then complete using validator confirmations.
        let filtered_messages = match observer.merge(messages) {
            Ok(msgs) => msgs,
            Err(FastCryptoError::NotEnoughInputs) => return, // wait for more messages
            Err(e) => {
                debug!("random beacon: Observer error while merging messages: {e:?}");
                return;
            }
        };

        match observer.complete::<StdRng>(&filtered_messages, &confirmations) {
            Ok(observer_output) => {
                let output = dkg_v1::Output {
                    nodes: observer_output.nodes,
                    vss_pk: observer_output.vss_pk,
                    shares: None,
                };
                self.finalize_dkg(
                    output,
                    epoch_store,
                    consensus_output,
                    round,
                    &format!(
                        "Observer DKG complete with {} confirmations",
                        confirmations.len()
                    ),
                );
            }
            Err(FastCryptoError::NotEnoughInputs) => (), // wait for more confirmations
            Err(e) => {
                debug!("random beacon: Observer error while completing DKG: {e:?}");
            }
        }
    }

    /// Adds a received VersionedDkgMessage to the randomness DKG state machine.
    /// Messages are enqueued for async processing and persisted later in `advance_dkg`.
    pub fn add_message(
        &mut self,
        authority: &AuthorityName,
        msg: VersionedDkgMessage,
    ) -> SuiResult {
        // message was received from other validators, so we need to ensure it uses a supported
        // version before we call other functions that assume the version is correct
        let dkg_version = self.epoch_store()?.protocol_config().dkg_version();
        if !msg.is_valid_version(dkg_version) {
            warn!("ignoring DKG Message from authority {authority:?} with unsupported version");
            return Ok(());
        }

        if self.used_messages.initialized() || self.dkg_output.initialized() {
            // We've already sent a `Confirmation`, so we can't add any more messages.
            return Ok(());
        }
        let Some((_, party_id)) = self.authority_info.get(authority) else {
            debug_fatal!(
                "random beacon: received DKG Message from unknown authority: {authority:?}"
            );
            return Ok(());
        };
        if *party_id != msg.sender() {
            warn!(
                "ignoring equivocating DKG Message from authority {authority:?} pretending to be PartyId {party_id:?}"
            );
            return Ok(());
        }
        if self.enqueued_messages.contains_key(&msg.sender())
            || self.processed_messages.contains_key(&msg.sender())
        {
            info!("ignoring duplicate DKG Message from authority {authority:?}");
            return Ok(());
        }

        match &self.participant {
            DkgParticipant::Observer(observer) => {
                // Observer nodes validate and process messages asynchronously, storing them
                // as ProcessedMessages (with empty shares/complaint) to reuse the same
                // persistence path as validators.
                let observer = Arc::clone(observer);
                self.enqueued_messages.insert(
                    msg.sender(),
                    tokio::task::spawn_blocking(move || {
                        let message = msg.unwrap_v1();
                        match observer.process_message(message.clone()) {
                            Ok(()) => {
                                Some(VersionedProcessedMessage::V1(dkg_v1::ProcessedMessage {
                                    message,
                                    shares: vec![],
                                    complaint: None,
                                }))
                            }
                            Err(err) => {
                                debug!(
                                    "random beacon: Observer error processing DKG Message: {err:?}"
                                );
                                None
                            }
                        }
                    }),
                );
            }
            DkgParticipant::Party(party) => {
                // Validator nodes process messages asynchronously in a background task.
                let party = Arc::clone(party);
                self.enqueued_messages.insert(
                    msg.sender(),
                    tokio::task::spawn_blocking(move || {
                        match party.process_message(msg.unwrap_v1(), &mut rand::thread_rng()) {
                            Ok(processed) => Some(VersionedProcessedMessage::V1(processed)),
                            Err(err) => {
                                debug!("random beacon: error processing DKG Message: {err:?}");
                                None
                            }
                        }
                    }),
                );
            }
        }
        Ok(())
    }

    /// Adds a received dkg::Confirmation to the randomness DKG state machine.
    pub(crate) fn add_confirmation(
        &mut self,
        output: &mut ConsensusCommitOutput,
        authority: &AuthorityName,
        conf: VersionedDkgConfirmation,
    ) -> SuiResult {
        // confirmation was received from other validators, so we need to ensure it uses a supported
        // version before we call other functions that assume the version is correct
        let dkg_version = self.epoch_store()?.protocol_config().dkg_version();
        if !conf.is_valid_version(dkg_version) {
            warn!(
                "ignoring DKG Confirmation from authority {authority:?} with unsupported version"
            );
            return Ok(());
        }

        if self.dkg_output.initialized() {
            // Once we have completed DKG, no more `Confirmation`s are needed.
            return Ok(());
        }
        let Some((_, party_id)) = self.authority_info.get(authority) else {
            error!(
                "random beacon: received DKG Confirmation from unknown authority: {authority:?}"
            );
            return Ok(());
        };
        if *party_id != conf.sender() {
            warn!(
                "ignoring equivocating DKG Confirmation from authority {authority:?} pretending to be PartyId {party_id:?}"
            );
            return Ok(());
        }
        self.confirmations.insert(conf.sender(), conf.clone());
        debug!(
            "random beacon: added DKG confirmation from party {} (total confirmations: {})",
            conf.sender(),
            self.confirmations.len()
        );
        output.insert_dkg_confirmation(conf);
        Ok(())
    }

    /// Reserves the next available round number for randomness generation if enough time has
    /// elapsed, or returns None if not yet ready (based on ProtocolConfig setting). Once the given
    /// batch is written, `generate_randomness` must be called to start the process. On restart,
    /// any reserved rounds for which the batch was written will automatically be resumed.
    pub(crate) fn reserve_next_randomness(
        &mut self,
        commit_timestamp: TimestampMs,
        output: &mut ConsensusCommitOutput,
    ) -> SuiResult<Option<RandomnessRound>> {
        let epoch_store = self.epoch_store()?;

        let last_round_timestamp = epoch_store
            .get_randomness_last_round_timestamp()
            .expect("read should not fail");

        if let Some(last_round_timestamp) = last_round_timestamp
            && commit_timestamp - last_round_timestamp
                < epoch_store
                    .protocol_config()
                    .random_beacon_min_round_interval_ms()
        {
            return Ok(None);
        }

        let randomness_round = self.next_randomness_round;
        self.next_randomness_round = self
            .next_randomness_round
            .checked_add(1)
            .expect("RandomnessRound should not overflow");

        output.reserve_next_randomness_round(self.next_randomness_round, commit_timestamp);

        Ok(Some(randomness_round))
    }

    /// Starts the process of generating the given RandomnessRound.
    /// Observer nodes skip this entirely as they don't hold private key shares.
    pub fn generate_randomness(&self, epoch: EpochId, randomness_round: RandomnessRound) {
        if self.participant.as_observer().is_some() {
            debug!(
                "random beacon: Observer skipping randomness generation for round {}",
                randomness_round.0
            );
            return;
        }
        self.network_handle
            .send_partial_signatures(epoch, randomness_round);
    }

    pub fn dkg_status(&self) -> DkgStatus {
        match self.dkg_output.get() {
            Some(Some(_)) => DkgStatus::Successful,
            Some(None) => DkgStatus::Failed,
            None => DkgStatus::Pending,
        }
    }

    /// Generates a new RandomnessReporter for reporting observed rounds to this RandomnessManager.
    pub fn reporter(&self) -> RandomnessReporter {
        RandomnessReporter {
            epoch_store: self.epoch_store.clone(),
            epoch: self.epoch,
            network_handle: self.network_handle.clone(),
            highest_completed_round: self.highest_completed_round.clone(),
        }
    }

    fn epoch_store(&self) -> SuiResult<Arc<AuthorityPerEpochStore>> {
        self.epoch_store
            .upgrade()
            .ok_or(SuiErrorKind::EpochEnded(self.epoch).into())
    }

    fn randomness_dkg_info_from_committee(
        committee: &Committee,
    ) -> Vec<(
        u16,
        AuthorityName,
        fastcrypto_tbls::ecies_v1::PublicKey<bls12381::G2Element>,
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
                (
                    index,
                    *name,
                    fastcrypto_tbls::ecies_v1::PublicKey::from(pk),
                    *stake,
                )
            })
            .collect()
    }
}

// Used by other components to notify the randomness system of observed randomness.
#[derive(Clone)]
pub struct RandomnessReporter {
    epoch_store: Weak<AuthorityPerEpochStore>,
    epoch: EpochId,
    network_handle: randomness::Handle,
    highest_completed_round: Arc<Mutex<Option<RandomnessRound>>>,
}

impl RandomnessReporter {
    /// Notifies the associated randomness manager that randomness for the given round has been
    /// durably committed in a checkpoint. This completes the process of generating randomness for
    /// the round.
    pub fn notify_randomness_in_checkpoint(&self, round: RandomnessRound) -> SuiResult {
        let epoch_store = self
            .epoch_store
            .upgrade()
            .ok_or(SuiErrorKind::EpochEnded(self.epoch))?;
        let mut highest_completed_round = self.highest_completed_round.lock();
        if Some(round) > *highest_completed_round {
            *highest_completed_round = Some(round);
            epoch_store
                .tables()?
                .randomness_highest_completed_round
                .insert(&SINGLETON_KEY, &round)?;
            self.network_handle
                .complete_round(epoch_store.committee().epoch(), round);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DkgStatus {
    Pending,
    Failed,
    Successful,
}

#[cfg(test)]
mod tests {
    use crate::{
        authority::{
            authority_per_epoch_store::{ExecutionIndices, ExecutionIndicesWithStatsV2},
            test_authority_builder::TestAuthorityBuilder,
        },
        checkpoints::CheckpointStore,
        consensus_adapter::{ConsensusAdapter, ConsensusAdapterMetrics, MockConsensusClient},
        epoch::randomness::*,
        mock_consensus::with_block_status,
    };
    use consensus_core::BlockStatus;
    use consensus_types::block::BlockRef;
    use std::num::NonZeroUsize;
    use sui_protocol_config::ProtocolConfig;
    use sui_protocol_config::{Chain, ProtocolVersion};
    use sui_types::messages_consensus::ConsensusTransactionKind;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_dkg_v1() {
        test_dkg(1).await;
    }

    async fn test_dkg(version: u64) {
        telemetry_subscribers::init_for_testing();

        let network_config =
            sui_swarm_config::network_config_builder::ConfigBuilder::new_with_temp_dir()
                .committee_size(NonZeroUsize::new(4).unwrap())
                .with_reference_gas_price(500)
                .build();

        let mut protocol_config =
            ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
        protocol_config.set_random_beacon_dkg_version_for_testing(version);

        let mut epoch_stores = Vec::new();
        let mut randomness_managers = Vec::new();
        let (tx_consensus, mut rx_consensus) = mpsc::channel(100);

        for validator in network_config.validator_configs.iter() {
            // Send consensus messages to channel.
            let mut mock_consensus_client = MockConsensusClient::new();
            let tx_consensus = tx_consensus.clone();
            mock_consensus_client
                .expect_submit()
                .withf(move |transactions: &[ConsensusTransaction], _epoch_store| {
                    tx_consensus.try_send(transactions.to_vec()).unwrap();
                    true
                })
                .returning(|_, _| {
                    Ok((
                        Vec::new(),
                        with_block_status(BlockStatus::Sequenced(BlockRef::MIN)),
                    ))
                });

            let state = TestAuthorityBuilder::new()
                .with_protocol_config(protocol_config.clone())
                .with_genesis_and_keypair(&network_config.genesis, validator.protocol_key_pair())
                .build()
                .await;
            let consensus_adapter = Arc::new(ConsensusAdapter::new(
                Arc::new(mock_consensus_client),
                CheckpointStore::new_for_tests(),
                state.name,
                100_000,
                100_000,
                ConsensusAdapterMetrics::new_test(),
            ));
            let epoch_store = state.epoch_store_for_testing();
            let randomness_manager = RandomnessManager::try_new(
                Arc::downgrade(&epoch_store),
                Box::new(consensus_adapter.clone()),
                sui_network::randomness::Handle::new_stub(),
                Some(validator.protocol_key_pair()),
            )
            .await
            .unwrap();

            epoch_stores.push(epoch_store);
            randomness_managers.push(randomness_manager);
        }

        // Generate and distribute Messages.
        let mut dkg_messages = Vec::new();
        for randomness_manager in randomness_managers.iter_mut() {
            randomness_manager.start_dkg().await.unwrap();

            let mut dkg_message = rx_consensus.recv().await.unwrap();
            assert!(dkg_message.len() == 1);
            match dkg_message.remove(0).kind {
                ConsensusTransactionKind::RandomnessDkgMessage(_, bytes) => {
                    let msg: VersionedDkgMessage = bcs::from_bytes(&bytes)
                        .expect("DKG message deserialization should not fail");
                    dkg_messages.push(msg);
                }
                _ => panic!("wrong type of message sent"),
            }
        }
        for i in 0..randomness_managers.len() {
            let mut output = ConsensusCommitOutput::new(0);
            output.record_consensus_commit_stats(ExecutionIndicesWithStatsV2 {
                index: ExecutionIndices {
                    last_committed_round: 0,
                    ..Default::default()
                },
                ..Default::default()
            });
            for (j, dkg_message) in dkg_messages.iter().cloned().enumerate() {
                randomness_managers[i]
                    .add_message(&epoch_stores[j].name, dkg_message)
                    .unwrap();
            }
            randomness_managers[i]
                .advance_dkg(&mut output, 0)
                .await
                .unwrap();
            let mut batch = epoch_stores[i].db_batch_for_test();
            output.write_to_batch(&epoch_stores[i], &mut batch).unwrap();
            batch.write().unwrap();
        }

        // Generate and distribute Confirmations.
        let mut dkg_confirmations = Vec::new();
        for _ in 0..randomness_managers.len() {
            let mut dkg_confirmation = rx_consensus.recv().await.unwrap();
            assert!(dkg_confirmation.len() == 1);
            match dkg_confirmation.remove(0).kind {
                ConsensusTransactionKind::RandomnessDkgConfirmation(_, bytes) => {
                    let msg: VersionedDkgConfirmation = bcs::from_bytes(&bytes)
                        .expect("DKG message deserialization should not fail");
                    dkg_confirmations.push(msg);
                }
                _ => panic!("wrong type of message sent"),
            }
        }
        for i in 0..randomness_managers.len() {
            let mut output = ConsensusCommitOutput::new(0);
            output.record_consensus_commit_stats(ExecutionIndicesWithStatsV2 {
                index: ExecutionIndices {
                    last_committed_round: 1,
                    ..Default::default()
                },
                ..Default::default()
            });
            for (j, dkg_confirmation) in dkg_confirmations.iter().cloned().enumerate() {
                randomness_managers[i]
                    .add_confirmation(&mut output, &epoch_stores[j].name, dkg_confirmation)
                    .unwrap();
            }
            randomness_managers[i]
                .advance_dkg(&mut output, 0)
                .await
                .unwrap();
            let mut batch = epoch_stores[i].db_batch_for_test();
            output.write_to_batch(&epoch_stores[i], &mut batch).unwrap();
            batch.write().unwrap();
        }

        // Verify DKG completed.
        for randomness_manager in &randomness_managers {
            assert_eq!(DkgStatus::Successful, randomness_manager.dkg_status());
        }
    }

    #[tokio::test]
    async fn test_dkg_expiration_v1() {
        test_dkg_expiration(1).await;
    }

    async fn test_dkg_expiration(version: u64) {
        telemetry_subscribers::init_for_testing();

        let network_config =
            sui_swarm_config::network_config_builder::ConfigBuilder::new_with_temp_dir()
                .committee_size(NonZeroUsize::new(4).unwrap())
                .with_reference_gas_price(500)
                .build();

        let mut epoch_stores = Vec::new();
        let mut randomness_managers = Vec::new();
        let (tx_consensus, mut rx_consensus) = mpsc::channel(100);

        let mut protocol_config =
            ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
        protocol_config.set_random_beacon_dkg_version_for_testing(version);

        for validator in network_config.validator_configs.iter() {
            // Send consensus messages to channel.
            let mut mock_consensus_client = MockConsensusClient::new();
            let tx_consensus = tx_consensus.clone();
            mock_consensus_client
                .expect_submit()
                .withf(move |transactions: &[ConsensusTransaction], _epoch_store| {
                    tx_consensus.try_send(transactions.to_vec()).unwrap();
                    true
                })
                .returning(|_, _| {
                    Ok((
                        Vec::new(),
                        with_block_status(consensus_core::BlockStatus::Sequenced(BlockRef::MIN)),
                    ))
                });

            let state = TestAuthorityBuilder::new()
                .with_protocol_config(protocol_config.clone())
                .with_genesis_and_keypair(&network_config.genesis, validator.protocol_key_pair())
                .build()
                .await;
            let consensus_adapter = Arc::new(ConsensusAdapter::new(
                Arc::new(mock_consensus_client),
                CheckpointStore::new_for_tests(),
                state.name,
                100_000,
                100_000,
                ConsensusAdapterMetrics::new_test(),
            ));
            let epoch_store = state.epoch_store_for_testing();
            let randomness_manager = RandomnessManager::try_new(
                Arc::downgrade(&epoch_store),
                Box::new(consensus_adapter.clone()),
                sui_network::randomness::Handle::new_stub(),
                Some(validator.protocol_key_pair()),
            )
            .await
            .unwrap();

            epoch_stores.push(epoch_store);
            randomness_managers.push(randomness_manager);
        }

        // Generate and distribute Messages.
        let mut dkg_messages = Vec::new();
        for randomness_manager in randomness_managers.iter_mut() {
            randomness_manager.start_dkg().await.unwrap();

            let mut dkg_message = rx_consensus.recv().await.unwrap();
            assert!(dkg_message.len() == 1);
            match dkg_message.remove(0).kind {
                ConsensusTransactionKind::RandomnessDkgMessage(_, bytes) => {
                    let msg: VersionedDkgMessage = bcs::from_bytes(&bytes)
                        .expect("DKG message deserialization should not fail");
                    dkg_messages.push(msg);
                }
                _ => panic!("wrong type of message sent"),
            }
        }
        for i in 0..randomness_managers.len() {
            let mut output = ConsensusCommitOutput::new(0);
            output.record_consensus_commit_stats(ExecutionIndicesWithStatsV2 {
                index: ExecutionIndices {
                    last_committed_round: 0,
                    ..Default::default()
                },
                ..Default::default()
            });
            for (j, dkg_message) in dkg_messages.iter().cloned().enumerate() {
                randomness_managers[i]
                    .add_message(&epoch_stores[j].name, dkg_message)
                    .unwrap();
            }
            randomness_managers[i]
                .advance_dkg(&mut output, u64::MAX)
                .await
                .unwrap();
            let mut batch = epoch_stores[i].db_batch_for_test();
            output.write_to_batch(&epoch_stores[i], &mut batch).unwrap();
            batch.write().unwrap();
        }

        // Verify DKG failed.
        for randomness_manager in &randomness_managers {
            assert_eq!(DkgStatus::Failed, randomness_manager.dkg_status());
        }
    }
}
