// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! BridgeAuthorityAggregator aggregates signatures from BridgeCommittee.

use crate::client::bridge_client::BridgeClient;
use crate::crypto::BridgeAuthorityPublicKeyBytes;
use crate::crypto::BridgeAuthoritySignInfo;
use crate::error::{BridgeError, BridgeResult};
use crate::metrics::BridgeMetrics;
use crate::types::BridgeCommitteeValiditySignInfo;
use crate::types::{
    BridgeAction, BridgeCommittee, CertifiedBridgeAction, VerifiedCertifiedBridgeAction,
    VerifiedSignedBridgeAction,
};
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;
use sui_authority_aggregation::ReduceOutput;
use sui_authority_aggregation::{quorum_map_then_reduce_with_timeout_and_prefs, SigRequestPrefs};
use sui_types::base_types::ConciseableName;
use sui_types::committee::StakeUnit;
use sui_types::committee::TOTAL_VOTING_POWER;
use tracing::{error, info, warn};

const TOTAL_TIMEOUT_MS: u64 = 5_000;
const PREFETCH_TIMEOUT_MS: u64 = 1_500;
const RETRY_INTERVAL_MS: u64 = 500;

pub struct BridgeAuthorityAggregator {
    pub committee: Arc<BridgeCommittee>,
    pub clients: Arc<BTreeMap<BridgeAuthorityPublicKeyBytes, Arc<BridgeClient>>>,
    pub metrics: Arc<BridgeMetrics>,
    pub committee_keys_to_names: Arc<BTreeMap<BridgeAuthorityPublicKeyBytes, String>>,
}

impl BridgeAuthorityAggregator {
    pub fn new(
        committee: Arc<BridgeCommittee>,
        metrics: Arc<BridgeMetrics>,
        committee_keys_to_names: Arc<BTreeMap<BridgeAuthorityPublicKeyBytes, String>>,
    ) -> Self {
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
            metrics,
            committee_keys_to_names,
        }
    }

    #[cfg(test)]
    pub fn new_for_testing(committee: Arc<BridgeCommittee>) -> Self {
        Self::new(
            committee,
            Arc::new(BridgeMetrics::new_for_testing()),
            Arc::new(BTreeMap::new()),
        )
    }

    pub async fn request_committee_signatures(
        &self,
        action: BridgeAction,
    ) -> BridgeResult<VerifiedCertifiedBridgeAction> {
        let state = GetSigsState::new(
            action.approval_threshold(),
            self.committee.clone(),
            self.metrics.clone(),
            self.committee_keys_to_names.clone(),
        );
        request_sign_bridge_action_into_certification(
            action,
            self.committee.clone(),
            self.clients.clone(),
            state,
            Duration::from_millis(PREFETCH_TIMEOUT_MS),
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
    metrics: Arc<BridgeMetrics>,
    committee_keys_to_names: Arc<BTreeMap<BridgeAuthorityPublicKeyBytes, String>>,
}

impl GetSigsState {
    fn new(
        validity_threshold: StakeUnit,
        committee: Arc<BridgeCommittee>,
        metrics: Arc<BridgeMetrics>,
        committee_keys_to_names: Arc<BTreeMap<BridgeAuthorityPublicKeyBytes, String>>,
    ) -> Self {
        Self {
            committee,
            total_bad_stake: 0,
            total_ok_stake: 0,
            sigs: BTreeMap::new(),
            validity_threshold,
            metrics,
            committee_keys_to_names,
        }
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
                self.add_ok_stake(stake, &name);
            }
            Entry::Occupied(_e) => {
                return Err(BridgeError::AuthoritySignatureDuplication(format!(
                    "Got signatures for the same authority twice: {}",
                    name.concise()
                )));
            }
        }
        if self.total_ok_stake >= self.validity_threshold {
            info!(
                "Got enough signatures from {} validators with total_ok_stake {}",
                self.sigs.len(),
                self.total_ok_stake
            );
            let signatures = self
                .sigs
                .iter()
                .map(|(k, v)| (k.clone(), v.signature.clone()))
                .collect::<BTreeMap<_, _>>();
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

    fn add_ok_stake(&mut self, ok_stake: StakeUnit, name: &BridgeAuthorityPublicKeyBytes) {
        if let Some(host_name) = self.committee_keys_to_names.get(name) {
            self.metrics
                .auth_agg_ok_responses
                .with_label_values(&[host_name])
                .inc();
        }
        self.total_ok_stake += ok_stake;
    }

    fn add_bad_stake(&mut self, bad_stake: StakeUnit, name: &BridgeAuthorityPublicKeyBytes) {
        if let Some(host_name) = self.committee_keys_to_names.get(name) {
            self.metrics
                .auth_agg_bad_responses
                .with_label_values(&[host_name])
                .inc();
        }
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
    prefetch_timeout: Duration,
) -> BridgeResult<VerifiedCertifiedBridgeAction> {
    // `preferences` is used as a trick here to influence the order of validators to be requested.
    // * if `Some(_)`, then we will request validators in the order of the voting power.
    // * if `None`, we still refer to voting power, but they are shuffled by randomness.
    // Because ethereum gas price is not negligible, when the signatures are to be verified on ethereum,
    // we pass in `Some` to make sure the validators with higher voting power are requested first
    // to save gas cost.
    let preference = match action {
        BridgeAction::SuiToEthBridgeAction(_) => Some(SigRequestPrefs {
            ordering_pref: BTreeSet::new(),
            prefetch_timeout,
        }),
        BridgeAction::EthToSuiBridgeAction(_) => None,
        _ => {
            if action.chain_id().is_sui_chain() {
                None
            } else {
                Some(SigRequestPrefs {
                    ordering_pref: BTreeSet::new(),
                    prefetch_timeout,
                })
            }
        }
    };
    let (result, _) = quorum_map_then_reduce_with_timeout_and_prefs(
        committee,
        clients,
        preference,
        state,
        |name, client| {
            Box::pin(async move {
                let start = std::time::Instant::now();
                let timeout = Duration::from_millis(TOTAL_TIMEOUT_MS);
                let retry_interval = Duration::from_millis(RETRY_INTERVAL_MS);
                while start.elapsed() < timeout {
                    match client.request_sign_bridge_action(action.clone()).await {
                        Ok(result) => {
                            return Ok(result);
                        }
                        // retryable errors
                        Err(BridgeError::TxNotFinalized) => {
                            warn!("Bridge authority {} observing transaction not yet finalized, retrying in {:?}", name.concise(), retry_interval);
                            tokio::time::sleep(retry_interval).await;
                        }
                        // non-retryable errors
                        Err(e) => {
                            return Err(e);
                        }
                    }
                }
                Err(BridgeError::TransientProviderError(format!("Bridge authority {} did not observe finalized transaction after {:?}", name.concise(), timeout)))
            })
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
                                state.add_bad_stake(stake, &name);
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Failed to get signature from {:?}. Error: {:?}",
                            name.concise(),
                            e
                        );
                        state.add_bad_stake(stake, &name);
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
        Duration::from_millis(TOTAL_TIMEOUT_MS),
    )
    .await
    .map_err(|state| {
        error!(
            "Failed to get enough signatures, bad stake: {}, blocklisted stake: {}, good stake: {}, validity threshold: {}",
            state.total_bad_stake,
            state.committee.total_blocklisted_stake(),
            state.total_ok_stake,
            state.validity_threshold,
        );
        BridgeError::AuthoritySignatureAggregationTooManyError(format!(
            "Failed to get enough signatures, bad stake: {}, blocklisted stake: {}, good stake: {}, validity threshold: {}",
            state.total_bad_stake,
            state.committee.total_blocklisted_stake(),
            state.total_ok_stake,
            state.validity_threshold,
        ))
    })?;
    Ok(result)
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

        let agg = BridgeAuthorityAggregator::new_for_testing(Arc::new(committee));
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
        let agg = BridgeAuthorityAggregator::new_for_testing(Arc::new(committee));
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
        let agg = BridgeAuthorityAggregator::new_for_testing(Arc::new(committee));
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

        let agg = BridgeAuthorityAggregator::new_for_testing(Arc::new(committee));

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
    async fn test_bridge_auth_agg_optimized() {
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

        // start servers - there is only one permutation of size 2 (1112, 2222) that will achieve quorum
        let (_handles, authorities, secrets) = get_test_authorities_and_run_mock_bridge_server(
            vec![666, 1000, 900, 900, 900, 900, 900, 1612, 2222],
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
            ],
        );

        let authorities_clone = authorities.clone();
        let committee = Arc::new(BridgeCommittee::new(authorities_clone).unwrap());

        let agg = BridgeAuthorityAggregator::new_for_testing(committee.clone());

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
            Some(Duration::from_millis(200)),
        );
        mock1.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[1])),
            Some(Duration::from_millis(200)),
        );
        mock2.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[2])),
            Some(Duration::from_millis(700)),
        );
        mock3.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[3])),
            Some(Duration::from_millis(700)),
        );
        mock4.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[4])),
            Some(Duration::from_millis(700)),
        );
        mock5.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[5])),
            Some(Duration::from_millis(700)),
        );
        mock6.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[6])),
            Some(Duration::from_millis(700)),
        );
        mock7.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[7])),
            Some(Duration::from_millis(900)),
        );
        mock8.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[8])),
            Some(Duration::from_millis(1_500)),
        );

        // we should receive all signatures in time, but only aggregate 2 authorities
        // to achieve quorum
        let metrics = Arc::new(BridgeMetrics::new_for_testing());
        let state = GetSigsState::new(
            action.approval_threshold(),
            committee.clone(),
            metrics.clone(),
            Arc::new(BTreeMap::new()),
        );
        let resp = request_sign_bridge_action_into_certification(
            action.clone(),
            agg.committee.clone(),
            agg.clients.clone(),
            state,
            Duration::from_millis(2_000),
        )
        .await
        .unwrap();
        let sig_keys = resp.auth_sig().signatures.keys().collect::<BTreeSet<_>>();
        assert_eq!(sig_keys.len(), 2);
        assert!(sig_keys.contains(&authorities[7].pubkey_bytes()));
        assert!(sig_keys.contains(&authorities[8].pubkey_bytes()));

        // we should receive all but the highest stake signatures in time, but still be able to
        // achieve quorum with 3 sigs
        let state = GetSigsState::new(
            action.approval_threshold(),
            committee.clone(),
            metrics.clone(),
            Arc::new(BTreeMap::new()),
        );
        let resp = request_sign_bridge_action_into_certification(
            action.clone(),
            agg.committee.clone(),
            agg.clients.clone(),
            state,
            Duration::from_millis(1_200),
        )
        .await
        .unwrap();
        let sig_keys = resp.auth_sig().signatures.keys().collect::<BTreeSet<_>>();
        assert_eq!(sig_keys.len(), 3);
        assert!(sig_keys.contains(&authorities[7].pubkey_bytes()));
        // this should not have come in time
        assert!(!sig_keys.contains(&authorities[8].pubkey_bytes()));

        // we should have fallen back to arrival order given that we timeout before we reach quorum
        let state = GetSigsState::new(
            action.approval_threshold(),
            committee.clone(),
            metrics.clone(),
            Arc::new(BTreeMap::new()),
        );
        let start = std::time::Instant::now();
        let resp = request_sign_bridge_action_into_certification(
            action.clone(),
            agg.committee.clone(),
            agg.clients.clone(),
            state,
            Duration::from_millis(500),
        )
        .await
        .unwrap();
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(700),
            "Expected to have to wait at least 700ms to fallback to arrival order and achieve quorum, but was {:?}",
            elapsed
        );
        let sig_keys = resp.auth_sig().signatures.keys().collect::<BTreeSet<_>>();
        assert_eq!(sig_keys.len(), 4);
        // These two do not make it on time initially, and then we should be able
        // to achieve quorum before these ultimately arrive
        assert!(!sig_keys.contains(&authorities[7].pubkey_bytes()));
        assert!(!sig_keys.contains(&authorities[8].pubkey_bytes()));
        // These were the first two to respond, and should be immediately
        // included once we fallback to arrival order
        assert!(sig_keys.contains(&authorities[0].pubkey_bytes()));
        assert!(sig_keys.contains(&authorities[1].pubkey_bytes()));
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

        let agg = BridgeAuthorityAggregator::new_for_testing(Arc::new(committee));

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
        let metrics = Arc::new(BridgeMetrics::new_for_testing());
        let mut state = GetSigsState::new(
            threshold,
            Arc::new(committee),
            metrics.clone(),
            Arc::new(BTreeMap::new()),
        );

        assert!(!state.is_too_many_error());
        let dummy = authorities[0].pubkey_bytes();
        // bad stake: 2500
        state.add_bad_stake(2500, &dummy);
        assert!(!state.is_too_many_error());

        // bad stake ; 5000
        state.add_bad_stake(2500, &dummy);
        assert!(!state.is_too_many_error());

        // bad stake : 6666
        state.add_bad_stake(1666, &dummy);
        assert!(!state.is_too_many_error());

        // bad stake : 6667 - too many errors
        state.add_bad_stake(1, &dummy);
        assert!(state.is_too_many_error());

        // Authority 0 is blocklisted, we lose 2500 stake
        authorities[0].is_blocklisted = true;
        let committee = BridgeCommittee::new(authorities.clone()).unwrap();
        let threshold = VALIDITY_THRESHOLD;
        let metrics = Arc::new(BridgeMetrics::new_for_testing());
        let mut state = GetSigsState::new(
            threshold,
            Arc::new(committee),
            metrics.clone(),
            Arc::new(BTreeMap::new()),
        );

        assert!(!state.is_too_many_error());

        // bad stake: 2500 + 2500
        state.add_bad_stake(2500, &dummy);
        assert!(!state.is_too_many_error());

        // bad stake: 5000 + 2500 - too many errors
        state.add_bad_stake(2500, &dummy);
        assert!(state.is_too_many_error());

        // Below we test `handle_verified_signed_action`
        authorities[0].is_blocklisted = false;
        authorities[1].voting_power = 1; // set authority's voting power to minimal
        authorities[2].voting_power = 4999;
        authorities[3].is_blocklisted = true; // blocklist authority 3
        let committee = BridgeCommittee::new(authorities.clone()).unwrap();
        let threshold = VALIDITY_THRESHOLD;
        let mut state = GetSigsState::new(
            threshold,
            Arc::new(committee.clone()),
            metrics.clone(),
            Arc::new(BTreeMap::new()),
        );

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
}
