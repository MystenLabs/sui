// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anemo::PeerId;
use fastcrypto::encoding::{Encoding, Hex};
use fastcrypto::error::{FastCryptoError, FastCryptoResult};
use fastcrypto::groups::bls12381;
use fastcrypto::serde_helpers::ToFromByteArray;
use fastcrypto::traits::{KeyPair, ToFromBytes};
use fastcrypto_tbls::{dkg_v1, dkg_v1::Output, nodes, nodes::PartyId};
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

    pub fn as_v1(&self) -> Option<&dkg_v1::ProcessedMessage<PkG, EncG>> {
        if let VersionedProcessedMessage::V1(msg) = self {
            Some(msg)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VersionedUsedProcessedMessages {
    V0(), // deprecated
    V1(dkg_v1::UsedProcessedMessages<PkG, EncG>),
}

impl VersionedUsedProcessedMessages {
    pub fn as_v1(&self) -> Option<&dkg_v1::UsedProcessedMessages<PkG, EncG>> {
        if let VersionedUsedProcessedMessages::V1(msg) = self {
            Some(msg)
        } else {
            None
        }
    }
}

/// Distinguishes between an active DKG participant (validator) and a read-only observer (fullnode).
enum DkgRole {
    Party(dkg_v1::Party<PkG, EncG>),
    Observer(dkg_v1::Observer<PkG, EncG>),
}

impl DkgRole {
    /// Creates a new DkgRole. When `authority_key_pair` is Some, creates an active Party
    /// (validator). When None, creates a read-only Observer (fullnode).
    ///
    /// * `authority_key_pair` - The validator's BLS key pair used to derive the DKG private key.
    ///   When None, an Observer is created that can follow DKG but not produce shares.
    /// * `nodes` - The reduced set of DKG participants with their public keys and weights.
    /// * `t` - The threshold number of shares required to reconstruct randomness.
    /// * `random_oracle` - Epoch-specific oracle used to derive deterministic challenges during DKG.
    fn try_new(
        authority_key_pair: Option<&AuthorityKeyPair>,
        nodes: nodes::Nodes<EncG>,
        t: u16,
        random_oracle: fastcrypto_tbls::random_oracle::RandomOracle,
    ) -> Option<Self> {
        let total_weight = nodes.total_weight();
        let num_nodes = nodes.num_nodes();

        if let Some(authority_key_pair) = authority_key_pair {
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
                nodes,
                t,
                random_oracle,
                &mut rand::thread_rng(),
            ) {
                Ok(party) => party,
                Err(err) => {
                    error!("random beacon: error while initializing Party: {err:?}");
                    return None;
                }
            };
            let name: AuthorityName = authority_key_pair.public().into();
            info!(
                "random beacon: Party initialized with authority={name}, total_weight={total_weight}, t={t}, num_nodes={num_nodes}",
            );
            Some(DkgRole::Party(party))
        } else {
            let observer = match dkg_v1::Observer::<PkG, EncG>::new(nodes, t, random_oracle) {
                Ok(observer) => observer,
                Err(err) => {
                    error!("random beacon: error while initializing Observer: {err:?}");
                    return None;
                }
            };
            info!(
                "random beacon: Observer initialized with total_weight={total_weight}, t={t}, num_nodes={num_nodes}",
            );
            Some(DkgRole::Observer(observer))
        }
    }

    fn is_party(&self) -> bool {
        matches!(self, DkgRole::Party(_))
    }

    fn is_observer(&self) -> bool {
        matches!(self, DkgRole::Observer(_))
    }

    /// Processes a received DKG message according to the role.
    fn process_message(
        &self,
        message: VersionedDkgMessage,
    ) -> FastCryptoResult<VersionedProcessedMessage> {
        match self {
            DkgRole::Party(party) => {
                let processed =
                    party.process_message(message.unwrap_v1(), &mut rand::thread_rng())?;
                Ok(VersionedProcessedMessage::V1(processed))
            }
            DkgRole::Observer(observer) => {
                let raw_msg = message.unwrap_v1();
                observer.process_message(raw_msg.clone())?;
                Ok(VersionedProcessedMessage::V1(dkg_v1::ProcessedMessage {
                    message: raw_msg,
                    shares: vec![],
                    complaint: None,
                }))
            }
        }
    }

    /// Merges processed DKG messages. For Party, produces a confirmation and used messages.
    /// For Observer, produces only used messages, confirmation is None, as observer nodes do
    /// not have any voting rights.
    fn merge_messages(
        &self,
        messages: Vec<VersionedProcessedMessage>,
    ) -> FastCryptoResult<(
        Option<VersionedDkgConfirmation>,
        VersionedUsedProcessedMessages,
    )> {
        match self {
            DkgRole::Party(party) => {
                let (conf, msgs) = party.merge(
                    &messages
                        .into_iter()
                        .map(|vm| vm.unwrap_v1())
                        .collect::<Vec<_>>(),
                )?;
                Ok((
                    Some(VersionedDkgConfirmation::V1(conf)),
                    VersionedUsedProcessedMessages::V1(msgs),
                ))
            }
            DkgRole::Observer(observer) => {
                let raw_messages: Vec<_> = messages
                    .into_iter()
                    .map(|pm| pm.unwrap_v1().message)
                    .collect();
                let used = observer.merge(raw_messages)?;
                Ok((
                    None,
                    VersionedUsedProcessedMessages::V1(dkg_v1::UsedProcessedMessages(
                        used.into_iter()
                            .map(|m| dkg_v1::ProcessedMessage {
                                message: m,
                                shares: vec![],
                                complaint: None,
                            })
                            .collect(),
                    )),
                ))
            }
        }
    }

    /// Completes DKG from used messages and confirmations. The output contains the shared public key which can be used
    /// from there after to validate the randomness round signatures. For the observer case the output will contain the public
    /// key but no shares, as again the node does not participate in the voting process.
    fn complete_dkg<'a>(
        &self,
        used_messages: &VersionedUsedProcessedMessages,
        confirmations: impl Iterator<Item = &'a VersionedDkgConfirmation>,
    ) -> FastCryptoResult<Output<PkG, EncG>> {
        match self {
            DkgRole::Party(party) => {
                let rng = &mut StdRng::from_rng(OsRng).expect("RNG construction should not fail");
                let msg = used_messages
                    .as_v1()
                    .expect("expected V1 used processed messages");
                party.complete(
                    msg,
                    &confirmations
                        .map(|vm| vm.as_v1().expect("expected V1 confirmation"))
                        .cloned()
                        .collect::<Vec<_>>(),
                    rng,
                )
            }
            DkgRole::Observer(observer) => {
                let raw_messages: Vec<_> = used_messages
                    .as_v1()
                    .expect("expected V1 used processed messages")
                    .0
                    .iter()
                    .map(|pm| pm.message.clone())
                    .collect();
                let confirmations: Vec<_> = confirmations
                    .map(|c| c.as_v1().expect("expected V1 confirmation").clone())
                    .collect();
                observer.complete(&raw_messages, &confirmations)
            }
        }
    }
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
    role: Arc<DkgRole>,
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
        let random_oracle = fastcrypto_tbls::random_oracle::RandomOracle::new(&format!(
            "dkg {} {}",
            Hex::encode(epoch_store.get_chain_identifier().as_bytes()),
            committee.epoch()
        ));

        let role = Arc::new(DkgRole::try_new(
            authority_key_pair,
            nodes,
            t,
            random_oracle,
        )?);

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
            role,
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
            if let DkgRole::Party(party) = rm.role.as_ref() {
                network_handle.update_epoch(
                    committee.epoch(),
                    rm.authority_info.clone(),
                    dkg_output,
                    party.t(),
                    highest_completed_round,
                );
            }
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

        // Re-send partial signatures for incomplete rounds (validators only).
        if rm.role.is_party() {
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
        }

        Some(rm)
    }

    /// Sends the initial dkg::Message to begin the randomness DKG protocol.
    /// For observers, this is a no-op (observers don't send messages).
    pub async fn start_dkg(&mut self) -> SuiResult {
        let party = match self.role.as_ref() {
            DkgRole::Observer(_) => {
                info!("random beacon: observer started observing DKG");
                return Ok(());
            }
            DkgRole::Party(party) => party,
        };

        if self.used_messages.initialized() || self.dkg_output.initialized() {
            // DKG already started (or completed or failed).
            return Ok(());
        }

        let _ = self.dkg_start_time.set(Instant::now());

        let epoch_store = self.epoch_store()?;
        let dkg_version = epoch_store.protocol_config().dkg_version();
        info!("random beacon: starting DKG, version {dkg_version}");

        let msg = match VersionedDkgMessage::create(dkg_version, party) {
            Ok(msg) => msg,
            Err(FastCryptoError::IgnoredMessage) => {
                info!(
                    "random beacon: no DKG Message for party id={} (zero weight)",
                    party.id
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

        self.try_merge_messages(consensus_output, &epoch_store)
            .await?;
        self.try_complete_dkg(consensus_output, round, &epoch_store)?;

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

    /// Drains enqueued messages and attempts to merge them. For validators, a successful merge
    /// produces and broadcasts a DKG Confirmation. For observers, it just records the used messages.
    async fn try_merge_messages(
        &mut self,
        consensus_output: &mut ConsensusCommitOutput,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        if self.dkg_output.initialized() || self.used_messages.initialized() {
            return Ok(());
        }

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

        let messages: Vec<_> = self.processed_messages.values().cloned().collect();

        match self.role.merge_messages(messages) {
            Ok((conf, used_msgs)) => {
                if let Some(conf) = &conf {
                    info!(
                        "random beacon: sending DKG Confirmation with {} complaints",
                        conf.num_of_complaints()
                    );
                } else {
                    info!(
                        "random beacon: observer merged {} DKG messages",
                        used_msgs
                            .as_v1()
                            .expect("expected V1 used processed messages")
                            .0
                            .len()
                    );
                }
                if self.used_messages.set(used_msgs.clone()).is_err() {
                    error!("BUG: used_messages should only ever be set once");
                }
                consensus_output.insert_dkg_used_messages(used_msgs);

                if let Some(conf) = conf {
                    let transaction = ConsensusTransaction::new_randomness_dkg_confirmation(
                        epoch_store.name,
                        &conf,
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
            }
            Err(FastCryptoError::NotEnoughInputs) => (), // wait for more input
            Err(e) => debug!("random beacon: error while merging DKG Messages: {e:?}"),
        }

        Ok(())
    }

    /// Attempts to complete DKG once enough Confirmations have been collected. For validators,
    /// this produces the shared public key and private key shares. For observers, only the
    /// shared public key is derived.
    fn try_complete_dkg(
        &mut self,
        consensus_output: &mut ConsensusCommitOutput,
        round: Round,
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult {
        if !self.dkg_output.initialized() && self.used_messages.initialized() {
            let used_messages = self
                .used_messages
                .get()
                .expect("checked above that `used_messages` is initialized");
            let complete_result = self
                .role
                .complete_dkg(used_messages, self.confirmations.values());

            let epoch = epoch_store.committee().epoch();
            let num_confirmations = self.confirmations.len();
            let num_messages = self.processed_messages.len();

            match complete_result {
                Ok(output) => {
                    // Set the output now both internally and to consensus output
                    self.dkg_output
                        .set(Some(output.clone()))
                        .expect("checked above that `dkg_output` is uninitialized");
                    consensus_output.set_dkg_output(output.clone());

                    let epoch_elapsed = epoch_store.epoch_open_time.elapsed().as_millis();
                    epoch_store
                        .metrics
                        .epoch_random_beacon_dkg_epoch_start_completion_time_ms
                        .set(epoch_elapsed as i64);
                    epoch_store.metrics.epoch_random_beacon_dkg_failed.set(0);

                    match self.role.as_ref() {
                        DkgRole::Party(party) => {
                            let num_shares =
                                output.shares.as_ref().map_or(0, |shares| shares.len());
                            let elapsed =
                                self.dkg_start_time.get().map(|t| t.elapsed().as_millis());
                            info!(
                                "random beacon: DKG complete for Party epoch={epoch} commit_round={round} \
                                 num_messages={num_messages} num_confirmations={num_confirmations} \
                                 num_shares={num_shares} epoch_elapsed_ms={epoch_elapsed} dkg_elapsed_ms={elapsed:?}"
                            );
                            epoch_store
                                .metrics
                                .epoch_random_beacon_dkg_num_shares
                                .set(num_shares as i64);

                            if let Some(elapsed) = elapsed {
                                epoch_store
                                    .metrics
                                    .epoch_random_beacon_dkg_completion_time_ms
                                    .set(elapsed as i64);
                            }

                            self.network_handle.update_epoch(
                                epoch_store.committee().epoch(),
                                self.authority_info.clone(),
                                output,
                                party.t(),
                                None,
                            );
                        }
                        DkgRole::Observer(_) => {
                            info!(
                                "random beacon: DKG complete for Observer epoch={epoch} commit_round={round} \
                                 num_messages={num_messages} num_confirmations={num_confirmations} \
                                 epoch_elapsed_ms={epoch_elapsed}"
                            );
                        }
                    }
                }
                Err(FastCryptoError::NotEnoughInputs) => (), // wait for more input
                Err(e) => error!("random beacon: error while processing DKG Confirmations: {e:?}"),
            }
        }

        Ok(())
    }

    /// Adds a received VersionedDkgMessage to the randomness DKG state machine.
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

        let sender = msg.sender();
        let role = self.role.clone();
        // TODO: Could save some CPU by not processing messages if we already have enough to merge.
        let handle = tokio::task::spawn_blocking(move || match role.process_message(msg) {
            Ok(processed) => Some(processed),
            Err(err) => {
                debug!("random beacon: error while processing DKG Message: {err:?}");
                None
            }
        });
        self.enqueued_messages.insert(sender, handle);
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

    /// Starts the process of generating the given RandomnessRound (validators only).
    pub fn generate_randomness(&self, epoch: EpochId, randomness_round: RandomnessRound) {
        if self.role.is_party() {
            self.network_handle
                .send_partial_signatures(epoch, randomness_round);
        }
    }

    pub fn dkg_status(&self) -> DkgStatus {
        match self.dkg_output.get() {
            Some(Some(_)) => DkgStatus::Successful,
            Some(None) => DkgStatus::Failed,
            None => DkgStatus::Pending,
        }
    }

    /// Generates a new RandomnessReporter for reporting observed rounds to this RandomnessManager.
    /// Returns None for observers (they don't generate partial signatures).
    pub fn reporter(&self) -> Option<RandomnessReporter> {
        if self.role.is_observer() {
            return None;
        }
        Some(RandomnessReporter {
            epoch_store: self.epoch_store.clone(),
            epoch: self.epoch,
            network_handle: self.network_handle.clone(),
            highest_completed_round: self.highest_completed_round.clone(),
        })
    }

    #[cfg(test)]
    fn dkg_output(&self) -> Option<&dkg_v1::Output<PkG, EncG>> {
        self.dkg_output.get().and_then(|o| o.as_ref())
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

    use arc_swap::Guard;

    /// Test harness that sets up validators (and optionally an observer) with mock consensus,
    /// ready for DKG message exchange.
    struct DkgTestSetup {
        epoch_stores: Vec<Guard<Arc<AuthorityPerEpochStore>>>,
        randomness_managers: Vec<RandomnessManager>,
        rx_consensus: mpsc::Receiver<Vec<ConsensusTransaction>>,
        num_validators: usize,
    }

    impl DkgTestSetup {
        async fn new(include_observer: bool) -> Self {
            let network_config =
                sui_swarm_config::network_config_builder::ConfigBuilder::new_with_temp_dir()
                    .committee_size(NonZeroUsize::new(4).unwrap())
                    .with_reference_gas_price(500)
                    .build();

            let mut protocol_config =
                ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
            protocol_config.set_random_beacon_dkg_version_for_testing(1);

            let num_validators = network_config.validator_configs.len();
            let mut epoch_stores = Vec::new();
            let mut randomness_managers = Vec::new();
            let (tx_consensus, rx_consensus) = mpsc::channel(100);

            for validator in network_config.validator_configs.iter() {
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
                    .with_genesis_and_keypair(
                        &network_config.genesis,
                        validator.protocol_key_pair(),
                    )
                    .build()
                    .await;
                let consensus_adapter = Arc::new(ConsensusAdapter::new(
                    Arc::new(mock_consensus_client),
                    CheckpointStore::new_for_tests(),
                    state.name,
                    100_000,
                    100_000,
                    ConsensusAdapterMetrics::new_test(),
                    Arc::new(tokio::sync::Notify::new()),
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

            if include_observer {
                let observer_epoch_store = epoch_stores[0].clone();
                let mut mock_observer_consensus = MockConsensusClient::new();
                mock_observer_consensus
                    .expect_submit()
                    .returning(|_, _| panic!("observer should not submit to consensus"));
                let observer_adapter = Arc::new(ConsensusAdapter::new(
                    Arc::new(mock_observer_consensus),
                    CheckpointStore::new_for_tests(),
                    observer_epoch_store.name,
                    100_000,
                    100_000,
                    ConsensusAdapterMetrics::new_test(),
                    Arc::new(tokio::sync::Notify::new()),
                ));
                let observer_manager = RandomnessManager::try_new(
                    Arc::downgrade(&observer_epoch_store),
                    Box::new(observer_adapter),
                    sui_network::randomness::Handle::new_stub(),
                    None,
                )
                .await
                .unwrap();

                epoch_stores.push(observer_epoch_store.into());
                randomness_managers.push(observer_manager);
            }

            Self {
                epoch_stores,
                randomness_managers,
                rx_consensus,
                num_validators,
            }
        }

        /// Runs start_dkg on all managers and collects the DKG messages from validators.
        async fn start_dkg_and_collect_messages(&mut self) -> Vec<VersionedDkgMessage> {
            let mut dkg_messages = Vec::new();
            for randomness_manager in self.randomness_managers.iter_mut() {
                randomness_manager.start_dkg().await.unwrap();
            }
            for _ in 0..self.num_validators {
                let mut dkg_message = self.rx_consensus.recv().await.unwrap();
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
            dkg_messages
        }

        /// Distributes DKG messages to all managers and advances DKG at the given round.
        async fn distribute_messages_and_advance(
            &mut self,
            dkg_messages: &[VersionedDkgMessage],
            advance_round: Round,
        ) {
            for i in 0..self.randomness_managers.len() {
                let mut output = ConsensusCommitOutput::new(0);
                output.record_consensus_commit_stats(ExecutionIndicesWithStatsV2 {
                    index: ExecutionIndices {
                        last_committed_round: 0,
                        ..Default::default()
                    },
                    ..Default::default()
                });
                for (j, dkg_message) in dkg_messages.iter().cloned().enumerate() {
                    self.randomness_managers[i]
                        .add_message(&self.epoch_stores[j].name, dkg_message)
                        .unwrap();
                }
                self.randomness_managers[i]
                    .advance_dkg(&mut output, advance_round)
                    .await
                    .unwrap();
                let mut batch = self.epoch_stores[i].db_batch_for_test();
                output
                    .write_to_batch(&self.epoch_stores[i], &mut batch)
                    .unwrap();
                batch.write().unwrap();
            }
        }

        /// Collects DKG confirmations from validators and distributes them to all managers.
        async fn collect_and_distribute_confirmations(&mut self) {
            let mut dkg_confirmations = Vec::new();
            for _ in 0..self.num_validators {
                let mut dkg_confirmation = self.rx_consensus.recv().await.unwrap();
                assert!(dkg_confirmation.len() == 1);
                match dkg_confirmation.remove(0).kind {
                    ConsensusTransactionKind::RandomnessDkgConfirmation(_, bytes) => {
                        let msg: VersionedDkgConfirmation = bcs::from_bytes(&bytes)
                            .expect("DKG confirmation deserialization should not fail");
                        dkg_confirmations.push(msg);
                    }
                    _ => panic!("wrong type of message sent"),
                }
            }
            for i in 0..self.randomness_managers.len() {
                let mut output = ConsensusCommitOutput::new(0);
                output.record_consensus_commit_stats(ExecutionIndicesWithStatsV2 {
                    index: ExecutionIndices {
                        last_committed_round: 1,
                        ..Default::default()
                    },
                    ..Default::default()
                });
                for (j, dkg_confirmation) in dkg_confirmations.iter().cloned().enumerate() {
                    self.randomness_managers[i]
                        .add_confirmation(&mut output, &self.epoch_stores[j].name, dkg_confirmation)
                        .unwrap();
                }
                self.randomness_managers[i]
                    .advance_dkg(&mut output, 0)
                    .await
                    .unwrap();
                let mut batch = self.epoch_stores[i].db_batch_for_test();
                output
                    .write_to_batch(&self.epoch_stores[i], &mut batch)
                    .unwrap();
                batch.write().unwrap();
            }
        }
    }

    #[tokio::test]
    async fn test_dkg() {
        telemetry_subscribers::init_for_testing();

        let mut setup = DkgTestSetup::new(false).await;
        let dkg_messages = setup.start_dkg_and_collect_messages().await;
        setup
            .distribute_messages_and_advance(&dkg_messages, 0)
            .await;
        setup.collect_and_distribute_confirmations().await;

        for rm in &setup.randomness_managers {
            assert_eq!(DkgStatus::Successful, rm.dkg_status());
        }
    }

    #[tokio::test]
    async fn test_dkg_expiration() {
        telemetry_subscribers::init_for_testing();

        let mut setup = DkgTestSetup::new(false).await;
        let dkg_messages = setup.start_dkg_and_collect_messages().await;
        // Pass u64::MAX as round to trigger DKG timeout.
        setup
            .distribute_messages_and_advance(&dkg_messages, u64::MAX)
            .await;

        for rm in &setup.randomness_managers {
            assert_eq!(DkgStatus::Failed, rm.dkg_status());
        }
    }

    /// Verifies that an Observer completes DKG alongside validators and derives the same
    /// shared public key (vss_pk), but without receiving any private key shares.
    #[tokio::test]
    async fn test_dkg_observer() {
        telemetry_subscribers::init_for_testing();

        let mut setup = DkgTestSetup::new(true).await;
        let observer_idx = setup.randomness_managers.len() - 1;

        let dkg_messages = setup.start_dkg_and_collect_messages().await;
        setup
            .distribute_messages_and_advance(&dkg_messages, 0)
            .await;

        for rm in &setup.randomness_managers {
            assert_eq!(DkgStatus::Pending, rm.dkg_status());
        }

        setup.collect_and_distribute_confirmations().await;

        for rm in &setup.randomness_managers {
            assert_eq!(DkgStatus::Successful, rm.dkg_status());
        }

        // Verify the observer derived the same vss_pk as validators, but without shares.
        let observer_output = setup.randomness_managers[observer_idx]
            .dkg_output()
            .expect("observer should have DKG output");
        let validator_output = setup.randomness_managers[0]
            .dkg_output()
            .expect("validator should have DKG output");
        assert_eq!(observer_output.vss_pk, validator_output.vss_pk);
        assert!(observer_output.shares.is_none());

        for rm in &setup.randomness_managers[..setup.num_validators] {
            let output = rm.dkg_output().expect("validator should have DKG output");
            assert!(output.shares.is_some());
        }
    }

    /// Builds a minimal set of DKG Nodes from a network config's validator key pairs.
    fn build_dkg_nodes(
        network_config: &sui_swarm_config::network_config::NetworkConfig,
    ) -> (nodes::Nodes<EncG>, u16) {
        let dkg_nodes: Vec<_> = network_config
            .validator_configs
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let pk = bls12381::G2Element::from_byte_array(
                    v.protocol_key_pair()
                        .public()
                        .as_bytes()
                        .try_into()
                        .expect("key length should match"),
                )
                .expect("should work to convert BLS key to G2Element");
                nodes::Node::<EncG> {
                    id: i as u16,
                    pk: fastcrypto_tbls::ecies_v1::PublicKey::from(pk),
                    weight: 1,
                }
            })
            .collect();
        let num_nodes = dkg_nodes.len();
        // threshold = ceil(num_nodes / 3) works for a minimal committee
        let t = num_nodes.div_ceil(3) as u16;
        nodes::Nodes::new(dkg_nodes).map(|n| (n, t)).unwrap()
    }

    #[test]
    fn test_dkg_role_try_new_party() {
        let network_config =
            sui_swarm_config::network_config_builder::ConfigBuilder::new_with_temp_dir()
                .committee_size(NonZeroUsize::new(4).unwrap())
                .with_reference_gas_price(500)
                .build();

        let (nodes, t) = build_dkg_nodes(&network_config);
        let random_oracle =
            fastcrypto_tbls::random_oracle::RandomOracle::new("test_dkg_role_party");

        let role = DkgRole::try_new(
            Some(network_config.validator_configs[0].protocol_key_pair()),
            nodes,
            t,
            random_oracle,
        );
        assert!(role.is_some());
        assert!(role.unwrap().is_party());
    }

    #[test]
    fn test_dkg_role_try_new_observer() {
        let network_config =
            sui_swarm_config::network_config_builder::ConfigBuilder::new_with_temp_dir()
                .committee_size(NonZeroUsize::new(4).unwrap())
                .with_reference_gas_price(500)
                .build();

        let (nodes, t) = build_dkg_nodes(&network_config);
        let random_oracle =
            fastcrypto_tbls::random_oracle::RandomOracle::new("test_dkg_role_observer");

        let role = DkgRole::try_new(None, nodes, t, random_oracle);
        assert!(role.is_some());
        assert!(role.unwrap().is_observer());
    }
}
