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
use std::time::Duration;
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
        let clients = committee
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
        let state = GetSigsState::new(action.approval_threshold(), self.committee.clone());
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
}

impl GetSigsState {
    fn new(validity_threshold: StakeUnit, committee: Arc<BridgeCommittee>) -> Self {
        Self {
            committee,
            total_bad_stake: 0,
            total_ok_stake: 0,
            sigs: BTreeMap::new(),
            validity_threshold,
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
    let preference = match action {
        BridgeAction::SuiToEthBridgeAction(_) => Some(BTreeSet::new()),
        BridgeAction::EthToSuiBridgeAction(_) => None,
        _ => {
            if action.chain_id().is_sui_chain() {
                None
            } else {
                Some(BTreeSet::new())
            }
        }
    };
    let (result, _) = quorum_map_then_reduce_with_timeout_and_prefs(
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
        );
        mock1.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[1])),
        );
        mock2.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[2])),
        );
        mock3.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[3])),
        );
        agg.request_committee_signatures(action.clone())
            .await
            .unwrap();

        // 1 out of 4 authorities returns error
        mock3.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Err(BridgeError::RestAPIError("".into())),
        );
        agg.request_committee_signatures(action.clone())
            .await
            .unwrap();

        // 2 out of 4 authorities returns error
        mock2.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Err(BridgeError::RestAPIError("".into())),
        );
        agg.request_committee_signatures(action.clone())
            .await
            .unwrap();

        // 3 out of 4 authorities returns error - good stake below valdiity threshold
        mock1.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Err(BridgeError::RestAPIError("".into())),
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
        );
        mock3.add_sui_event_response(
            sui_tx_digest,
            sui_tx_event_index,
            Ok(sign_action_with_key(&action, &secrets[3])),
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
}
