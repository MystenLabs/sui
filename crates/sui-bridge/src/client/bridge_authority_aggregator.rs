// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! BridgeAuthorityAggregator aggregates signatures from BridgeCommittee.

use crate::client::bridge_client::BridgeClient;
use crate::crypto::BridgeAuthorityPublicKeyBytes;
use crate::crypto::BridgeAuthoritySignInfo;
use crate::error::{BridgeError, BridgeResult};
use crate::types::BridgeCommitteeValiditySignInfo;
use crate::types::{
    BridgeAction, BridgeCommittee, CertifiedBridgeAction, VerifiedCertifiedBridgeAction,
    VerifiedSignedBridgeAction,
};
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sui_authority_aggregation::quorum_map_then_reduce_with_timeout_and_prefs;
use sui_authority_aggregation::ReduceOutput;
use sui_types::base_types::ConciseableName;
use sui_types::committee::StakeUnit;
use sui_types::committee::TOTAL_VOTING_POWER;
use tracing::{error, info, warn};

pub struct BridgeAuthorityAggregator {
    pub committee: Arc<BridgeCommittee>,
    pub clients: Arc<BTreeMap<BridgeAuthorityPublicKeyBytes, Arc<BridgeClient>>>,
}

impl BridgeAuthorityAggregator {
    pub fn new(committee: Arc<BridgeCommittee>) -> Self {
        let clients: BTreeMap<BridgeAuthorityPublicKeyBytes, Arc<BridgeClient>> = committee
            .members()
            .iter()
            .filter_map(|(name, authority)| {
                if authority.is_blocklisted {
                    warn!("Ignored blocklisted authority {:?} (stake: {}) when creating BridgeAuthorityAggregator", name.concise(), authority.voting_power);
                    return None;
                }
                // TODO: we could also record bad stakes here and use in signature aggregation
                match BridgeClient::new(
                    name.clone(),
                    committee.clone(),
                ) {
                    Ok(client) => Some((name.clone(), Arc::new(client))),
                    Err(e) => {
                        error!(
                            "Failed to create BridgeClient for {:?}: {:?}",
                            name.concise(),
                            e
                        );
                        None
                    }
                }
            })
            .collect::<BTreeMap<_, _>>();
        Self {
            committee,
            clients: Arc::new(clients),
        }
    }

    pub async fn request_committee_signatures(
        &self,
        action: BridgeAction,
    ) -> BridgeResult<VerifiedCertifiedBridgeAction> {
        // Decide whether we want to optimize for sigs
        let should_optimize = match action {
            BridgeAction::SuiToEthBridgeAction(_) => true,
            BridgeAction::EthToSuiBridgeAction(_) => false,
            _ => !action.chain_id().is_sui_chain(),
        };
        let state = if should_optimize {
            // We want to optimize for getting the minimal valid subset
            // We are willing to wait for ast most 2 seconds after getting enough signatures,
            // or the number of signatures is less than 3 + the minimal valid subset
            GetSigsState::new_with_best_effort(
                action.approval_threshold(),
                self.committee.clone(),
                Duration::from_secs(2),
                3,
            )
        } else {
            GetSigsState::new(action.approval_threshold(), self.committee.clone())
        };
        request_sign_bridge_action_into_certification(
            action,
            self.committee.clone(),
            self.clients.clone(),
            state,
        )
        .await
    }
}

#[derive(Debug)]
struct GetSigsState {
    total_bad_stake: StakeUnit,
    total_ok_stake: StakeUnit,
    sigs: BTreeMap<BridgeAuthorityPublicKeyBytes, BridgeAuthoritySignInfo>,
    validity_threshold: StakeUnit,
    committee: Arc<BridgeCommittee>,
    start_time: Instant,
    best_effort_config: Option<BestEffortConfig>,
    known_best_sigs: BTreeSet<BridgeAuthorityPublicKeyBytes>,
}

#[derive(Debug)]
struct BestEffortConfig {
    /// The instant when we get enough signatures and start best effort mode, used along with `best_effort_timeout`
    best_effort_start_time: Option<Instant>,
    /// Once we get enough signatures, how long are we willing to wait for the best effort validators
    best_effort_timeout: Duration,
    /// Once we get enough signatures, how many extra sigs can we tolerate for
    acceptable_extra_sigs: usize,
}

impl GetSigsState {
    fn new(validity_threshold: StakeUnit, committee: Arc<BridgeCommittee>) -> Self {
        Self {
            start_time: Instant::now(),
            committee,
            total_bad_stake: 0,
            total_ok_stake: 0,
            sigs: BTreeMap::new(),
            validity_threshold,
            best_effort_config: None,
            known_best_sigs: BTreeSet::new(),
        }
    }

    /// Create a new state with best effort mode enabled
    fn new_with_best_effort(
        validity_threshold: StakeUnit,
        committee: Arc<BridgeCommittee>,
        best_effort_timeout: Duration,
        acceptable_extra_sigs: usize,
    ) -> Self {
        Self {
            start_time: Instant::now(),
            committee,
            total_bad_stake: 0,
            total_ok_stake: 0,
            sigs: BTreeMap::new(),
            validity_threshold,
            best_effort_config: Some(BestEffortConfig {
                best_effort_start_time: None,
                best_effort_timeout,
                acceptable_extra_sigs,
            }),
            known_best_sigs: BTreeSet::new(),
        }
    }

    fn get_best_sigs(&self, action: BridgeAction) -> Option<VerifiedCertifiedBridgeAction> {
        if self.known_best_sigs.is_empty() {
            return None;
        }
        let signatures = self
            .known_best_sigs
            .iter()
            .map(|name| (name.clone(), self.sigs.get(name).unwrap().signature.clone()))
            .collect::<BTreeMap<_, _>>();
        let sig_info = BridgeCommitteeValiditySignInfo { signatures };
        let certified_action: sui_types::message_envelope::Envelope<
            BridgeAction,
            BridgeCommitteeValiditySignInfo,
        > = CertifiedBridgeAction::new_from_data_and_sig(action, sig_info);
        Some(VerifiedCertifiedBridgeAction::new_from_verified(
            certified_action,
        ))
    }

    fn handle_verified_signed_action(
        &mut self,
        name: BridgeAuthorityPublicKeyBytes,
        stake: StakeUnit,
        signed_action: VerifiedSignedBridgeAction,
    ) -> BridgeResult<Option<VerifiedCertifiedBridgeAction>> {
        info!("Got signatures from {}, stake: {}", name.concise(), stake);
        if !self.committee.is_active_member(&name) {
            return Err(BridgeError::InvalidBridgeAuthority(name));
        }

        // safeguard here to assert passed in stake matches the stake in committee
        // unwrap safe: if name is an active member then it must be in committee set
        assert_eq!(stake, self.committee.member(&name).unwrap().voting_power);

        match self.sigs.entry(name.clone()) {
            Entry::Vacant(e) => {
                e.insert(signed_action.auth_sig().clone());
                self.total_ok_stake += stake;
            }
            Entry::Occupied(_e) => {
                return Err(BridgeError::AuthoritySignatureDuplication(format!(
                    "Got signatures for the same authority twice: {}",
                    name.concise()
                )));
            }
        }

        if self.total_ok_stake >= self.validity_threshold {
            let signatures = if let Some(best_effort_config) = &mut self.best_effort_config {
                let mut should_update_best_sigs_and_try_harder = false;
                // Decide whether we are happy or we continue with best effort
                let minimal_validity_subset_size = self
                    .committee
                    .minimal_validity_subset_size(self.validity_threshold);
                if self.sigs.len() == *minimal_validity_subset_size {
                    info!(
                        "Got enough signatures from minimal validity subset from {} validators with total_ok_stake {} within {}ms",
                        self.sigs.len(),
                        self.total_ok_stake,
                        self.start_time.elapsed().as_millis()
                    );
                } else if let Some(best_effort_start_time) =
                    best_effort_config.best_effort_start_time
                {
                    if best_effort_start_time.elapsed() > best_effort_config.best_effort_timeout {
                        info!(
                            "Got enough signatures from {} validators with total_ok_stake {} within {}ms (best effort timeout {}ms reached)",
                            self.sigs.len(),
                            self.total_ok_stake,
                            self.start_time.elapsed().as_millis(),
                            best_effort_config.best_effort_timeout.as_millis()
                        );
                    } else {
                        // We have enough signatures, but we can do a bit better. Keep grinding.
                        should_update_best_sigs_and_try_harder = true;
                    }
                } else if self.sigs.len()
                    <= *minimal_validity_subset_size + best_effort_config.acceptable_extra_sigs
                {
                    info!(
                        "Got enough signatures from {} validators with total_ok_stake {} within {}ms (with {} extra signatures)",
                        self.sigs.len(),
                        self.total_ok_stake,
                        self.start_time.elapsed().as_millis(),
                        self.sigs.len() - *minimal_validity_subset_size,
                    );
                } else {
                    // We have enough signatures, but we can do a bit better. Keep grinding.
                    // From now on we are in best effort mode
                    if best_effort_config.best_effort_start_time.is_none() {
                        info!(
                            "Starting best effort {}ms: got enough signatures from {} validators with total_ok_stake {} within {}ms",
                            best_effort_config.best_effort_timeout.as_millis(),
                            self.sigs.len(),
                            self.total_ok_stake,
                            self.start_time.elapsed().as_millis(),
                        );
                        best_effort_config.best_effort_start_time = Some(Instant::now());
                    }
                    should_update_best_sigs_and_try_harder = true;
                }
                // Sort by voting power descending, get the top ones
                let mut sigs = self
                    .sigs
                    .iter()
                    .map(
                        // Unwrap safe: the key must below to the committee
                        |(name, sig_info)| {
                            (
                                name.clone(),
                                sig_info.clone(),
                                self.committee.member(name).unwrap().voting_power,
                            )
                        },
                    )
                    .collect::<Vec<_>>();
                sigs.sort_by_key(|k| std::cmp::Reverse(k.2));

                let mut total_power = 0;
                let sig = sigs
                    .into_iter()
                    .take_while(|(_, _, voting_power)| {
                        let should_take = total_power < self.validity_threshold;
                        total_power += voting_power;
                        should_take
                    })
                    .map(|(key, v, _)| (key.clone(), v.signature.clone()))
                    .collect::<BTreeMap<_, _>>();
                if should_update_best_sigs_and_try_harder {
                    self.known_best_sigs = sig.keys().cloned().collect();
                    return Ok(None);
                }
                sig
            } else {
                info!(
                    "Got enough signatures from {} validators with total_ok_stake {} within {}ms",
                    self.sigs.len(),
                    self.total_ok_stake,
                    self.start_time.elapsed().as_millis()
                );
                self.sigs
                    .iter()
                    .map(|(k, v)| (k.clone(), v.signature.clone()))
                    .collect()
            };

            // When we reach here, we have enough signatures and we are happy with them.
            let sig_info = BridgeCommitteeValiditySignInfo { signatures };
            let certified_action: sui_types::message_envelope::Envelope<
                BridgeAction,
                BridgeCommitteeValiditySignInfo,
            > = CertifiedBridgeAction::new_from_data_and_sig(
                signed_action.into_inner().into_data(),
                sig_info,
            );
            // `BridgeClient` already verified individual signatures
            Ok(Some(VerifiedCertifiedBridgeAction::new_from_verified(
                certified_action,
            )))
        } else {
            Ok(None)
        }
    }

    fn add_bad_stake(&mut self, bad_stake: StakeUnit) {
        self.total_bad_stake += bad_stake;
    }

    fn is_too_many_error(&self) -> bool {
        TOTAL_VOTING_POWER - self.total_bad_stake - self.committee.total_blocklisted_stake()
            < self.validity_threshold
    }
}

async fn request_sign_bridge_action_into_certification(
    action: BridgeAction,
    committee: Arc<BridgeCommittee>,
    clients: Arc<BTreeMap<BridgeAuthorityPublicKeyBytes, Arc<BridgeClient>>>,
    state: GetSigsState,
) -> BridgeResult<VerifiedCertifiedBridgeAction> {
    // `preferences` is used as a trick here to influence the order of validators to be requested.
    // * if `Some(_)`, then we will request validators in the order of the voting power.
    // * if `None`, we still refer to voting power, but they are shuffled by randomness.
    // Because ethereum gas price is not negligible, when the signatures are to be verified on ethereum,
    // we pass in `Some` to make sure the validators with higher voting power are requested first
    // to save gas cost.
    let preference = if state.best_effort_config.is_some() {
        Some(BTreeSet::new())
    } else {
        None
    };
    let action_clone = action.clone();
    let result = quorum_map_then_reduce_with_timeout_and_prefs(
        committee,
        clients,
        preference.as_ref(),
        state,
        |_name, client| {
            Box::pin(async move { client.request_sign_bridge_action(action.clone()).await })
        },
        |mut state, name, stake, result| {
            Box::pin(async move {
                match result {
                    Ok(verified_signed_action) => {
                        match state.handle_verified_signed_action(
                            name.clone(),
                            stake,
                            verified_signed_action,
                        ) {
                            Ok(Some(certified_action)) => {
                                return ReduceOutput::Success(certified_action)
                            }
                            Ok(None) => (),
                            Err(e) => {
                                error!(
                                    "Failed to handle verified signed action from {}: {:?}",
                                    name.concise(),
                                    e
                                );
                                state.add_bad_stake(stake);
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Failed to get signature from {:?}. Error: {:?}",
                            name.concise(),
                            e
                        );
                        state.add_bad_stake(stake);
                    }
                };

                // If bad stake (including blocklisted stake) is too high to reach validity threshold, return error
                if state.is_too_many_error() {
                    ReduceOutput::Failed(state)
                } else {
                    ReduceOutput::Continue(state)
                }
            })
        },
        // A herustic timeout, we expect the signing to finish within 5 seconds
        Duration::from_secs(5),
    )
    .await;
    if result.is_err() {
        let state = result.unwrap_err();
        if let Some(certified_action) = state.get_best_sigs(action_clone) {
            info!(
                "Got enough signatures (best known sigs) from {} validators with total_ok_stake {} within {}ms",
                state.known_best_sigs.len(),
                state.total_ok_stake,
                state.start_time.elapsed().as_millis(),
            );
            return Ok(certified_action);
        }
        error!(
            "Failed to get enough signatures, bad stake: {}, blocklisted stake: {}, good stake: {}, validity threshold: {}",
            state.total_bad_stake,
            state.committee.total_blocklisted_stake(),
            state.total_ok_stake,
            state.validity_threshold,
        );
        return Err(BridgeError::AuthoritySignatureAggregationTooManyError(format!(
            "Failed to get enough signatures, bad stake: {}, blocklisted stake: {}, good stake: {}, validity threshold: {}",
            state.total_bad_stake,
            state.committee.total_blocklisted_stake(),
            state.total_ok_stake,
            state.validity_threshold,
        )));
    };
    Ok(result.unwrap().0)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use fastcrypto::traits::ToFromBytes;
    use sui_types::committee::VALIDITY_THRESHOLD;
    use sui_types::digests::TransactionDigest;

    use crate::crypto::BridgeAuthorityPublicKey;
    use crate::server::mock_handler::BridgeRequestMockHandler;

    use super::*;
    use crate::test_utils::{
        get_test_authorities_and_run_mock_bridge_server, get_test_authority_and_key,
        get_test_sui_to_eth_bridge_action, sign_action_with_key,
    };
    use crate::types::BridgeCommittee;

    #[tokio::test]
    async fn test_bridge_auth_agg_construction() {
        telemetry_subscribers::init_for_testing();

        let mut authorities = vec![];
        for _i in 0..4 {
            let (authority, _, _) = get_test_authority_and_key(2500, 12345);
            authorities.push(authority);
        }
        let committee = BridgeCommittee::new(authorities.clone()).unwrap();

        let agg = BridgeAuthorityAggregator::new(Arc::new(committee));
        assert_eq!(
            agg.clients.keys().cloned().collect::<BTreeSet<_>>(),
            BTreeSet::from_iter(vec![
                authorities[0].pubkey_bytes(),
                authorities[1].pubkey_bytes(),
                authorities[2].pubkey_bytes(),
                authorities[3].pubkey_bytes()
            ])
        );

        // authority 2 is blocklisted
        authorities[2].is_blocklisted = true;
        let committee = BridgeCommittee::new(authorities.clone()).unwrap();
        let agg = BridgeAuthorityAggregator::new(Arc::new(committee));
        assert_eq!(
            agg.clients.keys().cloned().collect::<BTreeSet<_>>(),
            BTreeSet::from_iter(vec![
                authorities[0].pubkey_bytes(),
                authorities[1].pubkey_bytes(),
                authorities[3].pubkey_bytes()
            ])
        );

        // authority 3 has bad url
        authorities[3].base_url = "".into();
        let committee = BridgeCommittee::new(authorities.clone()).unwrap();
        let agg = BridgeAuthorityAggregator::new(Arc::new(committee));
        assert_eq!(
            agg.clients.keys().cloned().collect::<BTreeSet<_>>(),
            BTreeSet::from_iter(vec![
                authorities[0].pubkey_bytes(),
                authorities[1].pubkey_bytes(),
                authorities[3].pubkey_bytes()
            ])
        );
    }

    #[tokio::test]
    async fn test_bridge_auth_agg_ok() {
        telemetry_subscribers::init_for_testing();

        let mock0 = BridgeRequestMockHandler::new();
        let mock1 = BridgeRequestMockHandler::new();
        let mock2 = BridgeRequestMockHandler::new();
        let mock3 = BridgeRequestMockHandler::new();

        // start servers
        let (_handles, authorities, secrets) = get_test_authorities_and_run_mock_bridge_server(
            vec![2500, 2500, 2500, 2500],
            vec![mock0.clone(), mock1.clone(), mock2.clone(), mock3.clone()],
        );

        let committee = BridgeCommittee::new(authorities).unwrap();

        let agg = BridgeAuthorityAggregator::new(Arc::new(committee));

        let sui_tx_digest = TransactionDigest::random();
        let sui_tx_event_index = 0;
        let nonce = 0;
        let amount = 1000;
        let action = get_test_sui_to_eth_bridge_action(
            Some(sui_tx_digest),
            Some(sui_tx_event_index),
            Some(nonce),
            Some(amount),
            None,
            None,
            None,
        );

        // All authorities return signatures
        mock0.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[0])),
            None,
        );
        mock1.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[1])),
            None,
        );
        mock2.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[2])),
            None,
        );
        mock3.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[3])),
            None,
        );
        agg.request_committee_signatures(action.clone())
            .await
            .unwrap();

        // 1 out of 4 authorities returns error
        mock3.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Err(BridgeError::RestAPIError("".into())),
            None,
        );
        agg.request_committee_signatures(action.clone())
            .await
            .unwrap();

        // 2 out of 4 authorities returns error
        mock2.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Err(BridgeError::RestAPIError("".into())),
            None,
        );
        agg.request_committee_signatures(action.clone())
            .await
            .unwrap();

        // 3 out of 4 authorities returns error - good stake below valdiity threshold
        mock1.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Err(BridgeError::RestAPIError("".into())),
            None,
        );
        let err = agg
            .request_committee_signatures(action.clone())
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            BridgeError::AuthoritySignatureAggregationTooManyError(_)
        ));
    }

    #[tokio::test]
    async fn test_bridge_auth_agg_more_cases() {
        telemetry_subscribers::init_for_testing();

        let mock0 = BridgeRequestMockHandler::new();
        let mock1 = BridgeRequestMockHandler::new();
        let mock2 = BridgeRequestMockHandler::new();
        let mock3 = BridgeRequestMockHandler::new();

        // start servers
        let (_handles, mut authorities, secrets) = get_test_authorities_and_run_mock_bridge_server(
            vec![2500, 2500, 2500, 2500],
            vec![mock0.clone(), mock1.clone(), mock2.clone(), mock3.clone()],
        );
        // 0 and 1 are blocklisted
        authorities[0].is_blocklisted = true;
        authorities[1].is_blocklisted = true;

        let committee = BridgeCommittee::new(authorities.clone()).unwrap();

        let agg = BridgeAuthorityAggregator::new(Arc::new(committee));

        let sui_tx_digest = TransactionDigest::random();
        let sui_tx_event_index = 0;
        let nonce = 0;
        let amount = 1000;
        let action = get_test_sui_to_eth_bridge_action(
            Some(sui_tx_digest),
            Some(sui_tx_event_index),
            Some(nonce),
            Some(amount),
            None,
            None,
            None,
        );

        // Only mock authority 2 and 3 to return signatures, such that if BridgeAuthorityAggregator
        // requests to authority 0 and 1 (which should not happen) it will panic.
        mock2.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[2])),
            None,
        );
        mock3.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[3])),
            None,
        );
        let certified = agg
            .request_committee_signatures(action.clone())
            .await
            .unwrap();
        let signers = certified
            .auth_sig()
            .signatures
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            signers,
            BTreeSet::from_iter(vec![
                authorities[2].pubkey_bytes(),
                authorities[3].pubkey_bytes()
            ])
        );

        // if mock 3 returns error, then it won't reach validity threshold
        mock3.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Err(BridgeError::RestAPIError("".into())),
            None,
        );
        let err = agg
            .request_committee_signatures(action.clone())
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            BridgeError::AuthoritySignatureAggregationTooManyError(_)
        ));

        // if mock 3 returns duplicated signature (by authority 2), `BridgeClient` will catch this
        mock3.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[2])),
            None,
        );
        let err = agg
            .request_committee_signatures(action.clone())
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            BridgeError::AuthoritySignatureAggregationTooManyError(_)
        ));
    }

    #[test]
    fn test_get_sigs_state() {
        telemetry_subscribers::init_for_testing();

        let mut authorities = vec![];
        let mut secrets = vec![];
        for _i in 0..4 {
            let (authority, _, secret) = get_test_authority_and_key(2500, 12345);
            authorities.push(authority);
            secrets.push(secret);
        }

        let committee = BridgeCommittee::new(authorities.clone()).unwrap();

        let threshold = VALIDITY_THRESHOLD;
        let mut state = GetSigsState::new(threshold, Arc::new(committee));

        assert!(!state.is_too_many_error());

        // bad stake: 2500
        state.add_bad_stake(2500);
        assert!(!state.is_too_many_error());

        // bad stake ; 5000
        state.add_bad_stake(2500);
        assert!(!state.is_too_many_error());

        // bad stake : 6666
        state.add_bad_stake(1666);
        assert!(!state.is_too_many_error());

        // bad stake : 6667 - too many errors
        state.add_bad_stake(1);
        assert!(state.is_too_many_error());

        // Authority 0 is blocklisted, we lose 2500 stake
        authorities[0].is_blocklisted = true;
        let committee = BridgeCommittee::new(authorities.clone()).unwrap();
        let threshold = VALIDITY_THRESHOLD;
        let mut state = GetSigsState::new(threshold, Arc::new(committee));

        assert!(!state.is_too_many_error());

        // bad stake: 2500 + 2500
        state.add_bad_stake(2500);
        assert!(!state.is_too_many_error());

        // bad stake: 5000 + 2500 - too many errors
        state.add_bad_stake(2500);
        assert!(state.is_too_many_error());

        // Below we test `handle_verified_signed_action`
        authorities[0].is_blocklisted = false;
        authorities[1].voting_power = 1; // set authority's voting power to minimal
        authorities[2].voting_power = 4999;
        authorities[3].is_blocklisted = true; // blocklist authority 3
        let committee = BridgeCommittee::new(authorities.clone()).unwrap();
        let threshold = VALIDITY_THRESHOLD;
        let mut state = GetSigsState::new(threshold, Arc::new(committee.clone()));

        let sui_tx_digest = TransactionDigest::random();
        let sui_tx_event_index = 0;
        let nonce = 0;
        let amount = 1000;
        let action = get_test_sui_to_eth_bridge_action(
            Some(sui_tx_digest),
            Some(sui_tx_event_index),
            Some(nonce),
            Some(amount),
            None,
            None,
            None,
        );

        let sig_0 = sign_action_with_key(&action, &secrets[0]);
        // returns Ok(None)
        assert!(state
            .handle_verified_signed_action(
                authorities[0].pubkey_bytes().clone(),
                authorities[0].voting_power,
                VerifiedSignedBridgeAction::new_from_verified(sig_0.clone())
            )
            .unwrap()
            .is_none());
        assert_eq!(state.total_ok_stake, 2500);

        // Handling a sig from an already signed authority would fail
        let new_sig_0 = sign_action_with_key(&action, &secrets[0]);
        // returns Err(BridgeError::AuthoritySignatureDuplication)
        let err = state
            .handle_verified_signed_action(
                authorities[0].pubkey_bytes().clone(),
                authorities[0].voting_power,
                VerifiedSignedBridgeAction::new_from_verified(new_sig_0.clone()),
            )
            .unwrap_err();
        assert!(matches!(err, BridgeError::AuthoritySignatureDuplication(_)));
        assert_eq!(state.total_ok_stake, 2500);

        // Handling a sig from an authority not in committee would fail
        let (unknown_authority, _, kp) = get_test_authority_and_key(2500, 12345);
        let unknown_sig = sign_action_with_key(&action, &kp);
        // returns Err(BridgeError::InvalidBridgeAuthority)
        let err = state
            .handle_verified_signed_action(
                unknown_authority.pubkey_bytes().clone(),
                authorities[0].voting_power,
                VerifiedSignedBridgeAction::new_from_verified(unknown_sig.clone()),
            )
            .unwrap_err();
        assert!(matches!(err, BridgeError::InvalidBridgeAuthority(_)));
        assert_eq!(state.total_ok_stake, 2500);

        // Handling a blocklisted authority would fail
        let sig_3 = sign_action_with_key(&action, &secrets[3]);
        // returns Err(BridgeError::InvalidBridgeAuthority)
        let err = state
            .handle_verified_signed_action(
                authorities[3].pubkey_bytes().clone(),
                authorities[3].voting_power,
                VerifiedSignedBridgeAction::new_from_verified(sig_3.clone()),
            )
            .unwrap_err();
        assert!(matches!(err, BridgeError::InvalidBridgeAuthority(_)));
        assert_eq!(state.total_ok_stake, 2500);

        // Collect signtuare from authority 1 (voting power = 1)
        let sig_1 = sign_action_with_key(&action, &secrets[1]);
        // returns Ok(None)
        assert!(state
            .handle_verified_signed_action(
                authorities[1].pubkey_bytes().clone(),
                authorities[1].voting_power,
                VerifiedSignedBridgeAction::new_from_verified(sig_1.clone())
            )
            .unwrap()
            .is_none());
        assert_eq!(state.total_ok_stake, 2501);

        // Collect signature from authority 2 - reach validity threshold
        let sig_2 = sign_action_with_key(&action, &secrets[2]);
        // returns Ok(None)
        let certificate = state
            .handle_verified_signed_action(
                authorities[2].pubkey_bytes().clone(),
                authorities[2].voting_power,
                VerifiedSignedBridgeAction::new_from_verified(sig_2.clone()),
            )
            .unwrap()
            .unwrap();
        assert_eq!(state.total_ok_stake, 7500);

        assert_eq!(certificate.data(), &action);
        let signers = certificate
            .auth_sig()
            .signatures
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            signers,
            BTreeSet::from_iter(vec![
                authorities[0].pubkey_bytes(),
                authorities[1].pubkey_bytes(),
                authorities[2].pubkey_bytes()
            ])
        );

        for (pubkey, sig) in &certificate.auth_sig().signatures {
            let sign_info = BridgeAuthoritySignInfo {
                authority_pub_key: BridgeAuthorityPublicKey::from_bytes(pubkey.as_ref()).unwrap(),
                signature: sig.clone(),
            };
            assert!(sign_info.verify(&action, &committee).is_ok());
        }
    }

    #[tokio::test]
    async fn test_bridge_auth_agg_with_best_effort_config() {
        telemetry_subscribers::init_for_testing();

        let mock0 = BridgeRequestMockHandler::new();
        let mock1 = BridgeRequestMockHandler::new();
        let mock2 = BridgeRequestMockHandler::new();
        let mock3 = BridgeRequestMockHandler::new();
        let mock4 = BridgeRequestMockHandler::new();
        let mock5 = BridgeRequestMockHandler::new();
        let mock6 = BridgeRequestMockHandler::new();
        let mock7 = BridgeRequestMockHandler::new();
        let mock8 = BridgeRequestMockHandler::new();
        let mock9 = BridgeRequestMockHandler::new();

        // start servers - there is only one permutation of size 2 (1112, 2222) that will achieve quorum
        let (_handles, authorities, secrets) = get_test_authorities_and_run_mock_bridge_server(
            vec![333, 666, 666, 999, 1000, 1000, 1000, 1002, 1112, 2222],
            vec![
                mock0.clone(),
                mock1.clone(),
                mock2.clone(),
                mock3.clone(),
                mock4.clone(),
                mock5.clone(),
                mock6.clone(),
                mock7.clone(),
                mock8.clone(),
                mock9.clone(),
            ],
        );

        let committee = Arc::new(BridgeCommittee::new(authorities.clone()).unwrap());

        let agg = BridgeAuthorityAggregator::new(committee.clone());

        let sui_tx_digest = TransactionDigest::random();
        let sui_tx_event_index = 0;
        let nonce = 0;
        let amount = 1000;
        let action = get_test_sui_to_eth_bridge_action(
            Some(sui_tx_digest),
            Some(sui_tx_event_index),
            Some(nonce),
            Some(amount),
            None,
            None,
            None,
        );

        // All authorities return signatures
        mock0.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[0])),
            Some(Duration::from_millis(300)),
        );
        mock1.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[1])),
            Some(Duration::from_millis(100)),
        );
        mock2.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[2])),
            Some(Duration::from_millis(100)),
        );
        mock3.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[3])),
            Some(Duration::from_millis(100)),
        );
        mock4.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[4])),
            Some(Duration::from_millis(100)),
        );
        mock5.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[5])),
            Some(Duration::from_millis(100)),
        );
        mock6.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[6])),
            Some(Duration::from_millis(100)),
        );
        mock7.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[7])),
            Some(Duration::from_millis(100)),
        );
        mock8.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[8])),
            Some(Duration::from_millis(1000)), // <- delay
        );
        mock9.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[9])),
            Some(Duration::from_millis(1000)), // <- delay
        );

        // Unoptimized case: 8 and 9 are delayed, they should not be included in the certificate
        let state = GetSigsState::new(action.approval_threshold(), committee.clone());
        let resp = request_sign_bridge_action_into_certification(
            action.clone(),
            agg.committee.clone(),
            agg.clients.clone(),
            state,
        )
        .await
        .unwrap();

        let sig_keys = resp.auth_sig().signatures.keys().collect::<BTreeSet<_>>();
        assert!(!sig_keys.contains(&authorities[8].pubkey_bytes()));
        assert!(!sig_keys.contains(&authorities[9].pubkey_bytes()));
        let total_stake = resp
            .auth_sig()
            .signatures
            .keys()
            .map(|k| committee.active_stake(k))
            .sum::<StakeUnit>();
        assert!(total_stake >= action.approval_threshold());

        // optimized case: 8 and 9 timeout, but we can use 4 (2+2) sigs
        let state = GetSigsState::new_with_best_effort(
            action.approval_threshold(),
            committee.clone(),
            Duration::from_millis(10),
            2,
        );
        let resp = request_sign_bridge_action_into_certification(
            action.clone(),
            agg.committee.clone(),
            agg.clients.clone(),
            state,
        )
        .await
        .unwrap();

        let sig_keys = resp.auth_sig().signatures.keys().collect::<BTreeSet<_>>();
        assert_eq!(sig_keys.len(), 4);
        assert!(!sig_keys.contains(&authorities[8].pubkey_bytes()));
        assert!(!sig_keys.contains(&authorities[9].pubkey_bytes()));

        // optimized case: 8 and 9 are delayed but we wait for longer
        let state = GetSigsState::new_with_best_effort(
            action.approval_threshold(),
            committee.clone(),
            Duration::from_millis(2000),
            0,
        );
        let resp = request_sign_bridge_action_into_certification(
            action.clone(),
            agg.committee.clone(),
            agg.clients.clone(),
            state,
        )
        .await
        .unwrap();

        let sig_keys = resp.auth_sig().signatures.keys().collect::<BTreeSet<_>>();
        assert_eq!(sig_keys.len(), 2);
        let total_stake = resp
            .auth_sig()
            .signatures
            .keys()
            .map(|k| committee.active_stake(k))
            .sum::<StakeUnit>();
        assert_eq!(total_stake, 3334);

        // optimized case: we are willing to wait for 10ms to get the perfect 2 sig set, otherwise we are ok with any set
        let state = GetSigsState::new_with_best_effort(
            action.approval_threshold(),
            committee.clone(),
            Duration::from_millis(10),
            0,
        );
        let resp = request_sign_bridge_action_into_certification(
            action.clone(),
            agg.committee.clone(),
            agg.clients.clone(),
            state,
        )
        .await
        .unwrap();

        // At this point we should have collected all signatuers other than 8 and 9
        // Verify that we only take the minimal set which is 7, 6, 5, 4
        let sig_keys = resp.auth_sig().signatures.keys().collect::<BTreeSet<_>>();
        assert_eq!(sig_keys.len(), 4);

        // optimzied case but does not pan out, the ideal case won't ever happen. In this case
        // default to best knwown sigs set.
        mock8.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[8])),
            Some(Duration::from_millis(10000)), // <- timeout in mapper, we don't get this sig
        );
        mock9.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[9])),
            Some(Duration::from_millis(10000)), // <- timeout in mapper, we don't get this sig
        );
        let state = GetSigsState::new_with_best_effort(
            action.approval_threshold(),
            committee.clone(),
            Duration::from_millis(10000), // mapper times out before this
            0,
        );
        let resp = request_sign_bridge_action_into_certification(
            action.clone(),
            agg.committee.clone(),
            agg.clients.clone(),
            state,
        )
        .await
        .unwrap();
        // At this point we should have collected all signatuers other than 8 and 9
        // Verify that we only take the minimal set which is 7, 6, 5, 4
        let sig_keys = resp.auth_sig().signatures.keys().collect::<BTreeSet<_>>();
        assert_eq!(sig_keys.len(), 4);
    }

    #[tokio::test]
    async fn test_bridge_auth_agg_with_best_effort_config_use_best_known_sigs() {
        telemetry_subscribers::init_for_testing();

        let mock0 = BridgeRequestMockHandler::new();
        let mock1 = BridgeRequestMockHandler::new();
        let mock2 = BridgeRequestMockHandler::new();
        let mock3 = BridgeRequestMockHandler::new();
        let mock4 = BridgeRequestMockHandler::new();

        let (_handles, authorities, secrets) = get_test_authorities_and_run_mock_bridge_server(
            vec![1, 1, 3332, 3333, 3333],
            vec![
                mock0.clone(),
                mock1.clone(),
                mock2.clone(),
                mock3.clone(),
                mock4.clone(),
            ],
        );

        let committee = Arc::new(BridgeCommittee::new(authorities.clone()).unwrap());

        let agg = BridgeAuthorityAggregator::new(committee.clone());

        let sui_tx_digest = TransactionDigest::random();
        let sui_tx_event_index = 0;
        let nonce = 0;
        let amount = 1000;
        let action = get_test_sui_to_eth_bridge_action(
            Some(sui_tx_digest),
            Some(sui_tx_event_index),
            Some(nonce),
            Some(amount),
            None,
            None,
            None,
        );

        // All authorities return signatures
        mock0.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[0])),
            None,
        );
        mock1.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[1])),
            None,
        );
        mock2.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[2])),
            None,
        );
        mock3.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Err(BridgeError::Generic("sowhat".into())),
            None,
        );
        mock4.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Err(BridgeError::Generic("sowhat".into())),
            None,
        );

        let state = GetSigsState::new_with_best_effort(
            action.approval_threshold(),
            committee.clone(),
            Duration::from_millis(10000),
            0,
        );
        let resp = request_sign_bridge_action_into_certification(
            action.clone(),
            agg.committee.clone(),
            agg.clients.clone(),
            state,
        )
        .await
        .unwrap();

        // It has to be  {1, 1, 3331}
        let sig_keys = resp.auth_sig().signatures.keys().collect::<BTreeSet<_>>();
        assert_eq!(sig_keys.len(), 3);
        let total_stake = resp
            .auth_sig()
            .signatures
            .keys()
            .map(|k| committee.active_stake(k))
            .sum::<u64>();
        assert_eq!(total_stake, 3334);
    }
}
