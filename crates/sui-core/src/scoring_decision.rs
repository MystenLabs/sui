// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityMetrics;
use crate::math::median;
use arc_swap::ArcSwap;
use fastcrypto::traits::ToFromBytes;
use narwhal_config::{Committee, Stake};
use narwhal_crypto::PublicKey;
use narwhal_types::ReputationScores;
use std::collections::HashMap;
use std::sync::Arc;
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::AuthorityName;
use tracing::{info, warn};

/// Updates list of authorities that are deemed to have low reputation scores by consensus
/// these may be lagging behind the network, byzantine, or not reliably participating for any reason.
/// We want to ensure that the remaining set of validators once we exclude the low scoring authorities
/// is including enough stake for a quorum, at the very least. It is also possible that no authorities
/// are particularly low scoring, in which case this will result in storing an empty list.
///
/// Rather than using hardcoded cutoff score values, which will need consistent maintenance and create
/// a tight coupling between this code and the scoring code, we detect low-value outliers using
/// all the validator scores. The outlier detection method is by using the adjusted
/// median absolute deviation see https://en.wikipedia.org/wiki/Median_absolute_deviation for more
/// details. This calculates a the median of the data, then the absolute deviations from the median
/// for each authority, or the difference between the median and the score value. We then take the
/// median of those absolute deviations for each authority, which is called the median absolute deviation (MAD).
/// Once we have that value, if any authority's absolute deviation / ( MAD / MAD_DIVISOR) < -CUTOFF_VALUE
/// then it is deemed to be a low-value outlier. The values of MAD_DIVISOR and CUTOFF_VALUE can be
/// tweaked to change the sensitivity to outliers. They were chosen based on trial and error to
/// produce reasonable results for score values in the order of magnitude of 100s.
/// If you increase MAD_DIVISOR you decrease sensitivity to the spread of data and if you increase
/// CUTOFF_VALUE you will see less values being included as outliers. As the scores get higher in
/// value, outlier sensitivity tends to decrease using this method.
///
/// If we find that we have rated enough validators as low scoring such that we no longer have
/// quorum with the remaining validators, then we either need to update this method's parameters,
/// our general approach to finding outliers, or our network is in a bad state. If we need to update
/// this code, we let it detect this and disable itself for safety reasons. If we have a bad network
/// state then in the interest of making debugging and investigation easier, disabling the scoring
/// mechanism will likely be helpful.
pub fn update_low_scoring_authorities(
    low_scoring_authorities: Arc<ArcSwap<HashMap<AuthorityName, u64>>>,
    committee: &Committee,
    reputation_scores: ReputationScores,
    authority_names_to_hostnames: HashMap<AuthorityName, String>,
    metrics: &Arc<AuthorityMetrics>,
    protocol_config: &ProtocolConfig,
) {
    if protocol_config.scoring_decision_with_validity_cutoff() {
        update_low_scoring_authorities_with_no_disable_mechanism(
            low_scoring_authorities,
            committee,
            reputation_scores,
            authority_names_to_hostnames,
            metrics,
            protocol_config.scoring_decision_mad_divisor(),
            protocol_config.scoring_decision_cutoff_value(),
        );
    } else {
        // TODO remove this after the protocol version upgrade to 5
        static MAD_DIVISOR: f64 = 1.2;
        static CUTOFF_VALUE: f64 = 3.0;

        update_low_scoring_authorities_with_previous_disable_mechanism(
            low_scoring_authorities,
            committee,
            reputation_scores,
            authority_names_to_hostnames,
            metrics,
            MAD_DIVISOR,
            CUTOFF_VALUE,
        );
    }
}

// TODO: remove after validators have upgraded to protocol version 5.
fn update_low_scoring_authorities_with_previous_disable_mechanism(
    low_scoring_authorities: Arc<ArcSwap<HashMap<AuthorityName, u64>>>,
    committee: &Committee,
    reputation_scores: ReputationScores,
    authority_names_to_hostnames: HashMap<AuthorityName, String>,
    metrics: &Arc<AuthorityMetrics>,
    mad_divisor: f64,
    cut_off_value: f64,
) {
    if !reputation_scores.final_of_schedule {
        return;
    }

    // Convert the narwhal authority ids to the corresponding AuthorityName in SUI
    // Also capture the stake so can calculate later is strong quorum is reached for the non-low scoring authorities.
    let scores_per_authority: HashMap<AuthorityName, (u64, Stake)> = reputation_scores
        .scores_per_authority
        .into_iter()
        .map(|(authority_id, score)| {
            let authority = committee.authority(&authority_id).unwrap();
            let name: AuthorityName = authority.protocol_key().into();

            // report the scores
            if let Some(hostname) = authority_names_to_hostnames.get(&name) {
                info!("authority {} has score {}", hostname, score);

                metrics
                    .consensus_handler_scores
                    .with_label_values(&[&format!("{:?}", hostname)])
                    .set(score as i64);
            }

            (name, (score, authority.stake()))
        })
        .collect();

    let mut final_low_scoring_map = HashMap::new();

    let mut score_list = vec![];
    let mut nonzero_scores = vec![];
    for (score, _stake) in scores_per_authority.values() {
        score_list.push(*score as f64);
        if score != &0_u64 {
            nonzero_scores.push(*score as f64);
        }
    }

    let median_value = median(&nonzero_scores);
    let mut deviations = vec![];
    let mut abs_deviations = vec![];
    for (i, _) in score_list.clone().iter().enumerate() {
        deviations.push(score_list[i] - median_value);
        if score_list[i] != 0.0 {
            abs_deviations.push((score_list[i] - median_value).abs());
        }
    }

    // adjusted median absolute deviation
    let mad = median(&abs_deviations) / mad_divisor;
    let mut low_scoring = vec![];
    let mut rest = vec![];
    for (i, (a, (score, stake))) in scores_per_authority.iter().enumerate() {
        let temp = deviations[i] / mad;
        if temp < -cut_off_value {
            low_scoring.push((a, *score));
        } else {
            rest.push((a, *stake));
        }
    }

    // report new scores
    let len_low_scoring = low_scoring.len();
    metrics
        .consensus_handler_num_low_scoring_authorities
        .set(len_low_scoring as i64);

    info!("{:?} low scoring authorities calculated", len_low_scoring);

    for (authority, score) in low_scoring {
        final_low_scoring_map.insert(*authority, score);
        if let Some(hostname) = authority_names_to_hostnames.get(authority) {
            info!("low scoring authority {} has score {}", hostname, score);
        }
    }

    // make sure the rest have at least quorum
    let remaining_stake = rest.into_iter().map(|(_, stake)| stake).sum::<Stake>();
    let quorum_threshold = committee.quorum_threshold();
    if remaining_stake < quorum_threshold {
        warn!(
            "too many low reputation-scoring authorities, temporarily disabling scoring mechanism"
        );

        low_scoring_authorities.swap(Arc::new(HashMap::new()));
        return;
    }

    low_scoring_authorities.swap(Arc::new(final_low_scoring_map));
}

fn update_low_scoring_authorities_with_no_disable_mechanism(
    low_scoring_authorities: Arc<ArcSwap<HashMap<AuthorityName, u64>>>,
    committee: &Committee,
    reputation_scores: ReputationScores,
    authority_names_to_hostnames: HashMap<AuthorityName, String>,
    metrics: &Arc<AuthorityMetrics>,
    mad_divisor: f64,
    cut_off_value: f64,
) {
    if !reputation_scores.final_of_schedule {
        return;
    }

    // Convert the narwhal authority ids to the corresponding AuthorityName in SUI
    // Also capture the stake so can calculate later is strong quorum is reached for the non-low scoring authorities.
    let scores_per_authority: HashMap<AuthorityName, (u64, Stake)> = reputation_scores
        .scores_per_authority
        .into_iter()
        .map(|(authority_id, score)| {
            let authority = committee.authority(&authority_id).unwrap();
            let name: AuthorityName = authority.protocol_key().into();

            // report the scores
            if let Some(hostname) = authority_names_to_hostnames.get(&name) {
                info!("authority {} has score {}", hostname, score);

                metrics
                    .consensus_handler_scores
                    .with_label_values(&[&format!("{:?}", hostname)])
                    .set(score as i64);
            }

            (name, (score, authority.stake()))
        })
        .collect();

    let mut final_low_scoring_map = HashMap::new();

    let mut score_list = vec![];
    let mut nonzero_scores = vec![];
    for (score, _stake) in scores_per_authority.values() {
        score_list.push(*score as f64);
        if score != &0_u64 {
            nonzero_scores.push(*score as f64);
        }
    }

    let median_value = median(&nonzero_scores);
    let mut deviations = vec![];
    let mut abs_deviations = vec![];
    for (i, _) in score_list.clone().iter().enumerate() {
        deviations.push(score_list[i] - median_value);
        if score_list[i] != 0.0 {
            abs_deviations.push((score_list[i] - median_value).abs());
        }
    }

    // adjusted median absolute deviation
    let mad = median(&abs_deviations) / mad_divisor;
    let mut low_scoring = vec![];
    for (i, (a, (score, _stake))) in scores_per_authority.iter().enumerate() {
        let temp = deviations[i] / mad;

        // We expect the methodology to include the zero scoring validators, but we explicitly
        // include them to make sure, as we know for sure that those have no contribution.
        if *score == 0 || temp < -cut_off_value {
            low_scoring.push((a, *score));
        }
    }

    // report new scores
    let len_low_scoring = low_scoring.len();
    info!("{:?} low scoring authorities calculated", len_low_scoring);

    // Do not disable the scoring mechanism when more than f validators are excluded. Just keep
    // marking low scoring authorities up to f.

    // sort the low scoring authorities in asc order
    low_scoring.sort_by_key(|(_, score)| *score);

    // take low scoring authorities while we haven't reached validity threshold (f+1)
    let mut total_stake = 0;
    for (authority, score) in low_scoring {
        total_stake += committee
            .authority_by_key(&PublicKey::from_bytes(authority.as_ref()).unwrap())
            .unwrap()
            .stake();

        let included = if !committee.reached_validity(total_stake) {
            final_low_scoring_map.insert(*authority, score);
            true
        } else {
            false
        };
        if let Some(hostname) = authority_names_to_hostnames.get(authority) {
            info!(
                "low scoring authority {} has score {}, included: {}",
                hostname, score, included
            );
        }
    }

    // Report the actual flagged final low scoring authorities
    metrics
        .consensus_handler_num_low_scoring_authorities
        .set(final_low_scoring_map.len() as i64);

    low_scoring_authorities.swap(Arc::new(final_low_scoring_map));
}

#[cfg(test)]
mod tests {
    #![allow(clippy::mutable_key_type)]
    use crate::authority::AuthorityMetrics;
    use crate::scoring_decision::update_low_scoring_authorities;
    use arc_swap::ArcSwap;
    use fastcrypto::traits::{InsecureDefault, KeyPair as _};
    use mysten_network::Multiaddr;
    use narwhal_config::Committee;
    use narwhal_config::{Authority, CommitteeBuilder};
    use narwhal_crypto::KeyPair;
    use narwhal_types::ReputationScores;
    use prometheus::Registry;
    use rand::rngs::{OsRng, StdRng};
    use rand::SeedableRng;
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;
    use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
    use sui_types::crypto::NetworkPublicKey;

    fn protocol_v4() -> ProtocolConfig {
        // There are no chain specific protocol config options at this version
        // so the chain is irrelevant
        ProtocolConfig::get_for_version(ProtocolVersion::new(4), Chain::Unknown)
    }

    fn protocol_v5() -> ProtocolConfig {
        // There are no chain specific protocol config options at this version
        // so the chain is irrelevant
        ProtocolConfig::get_for_version(ProtocolVersion::new(5), Chain::Unknown)
    }

    #[test]
    pub fn test_update_low_scoring_authorities() {
        let committee = generate_committee(4);
        let mut authorities = committee.authorities();
        let a1 = authorities.next().unwrap();
        let a2 = authorities.next().unwrap();
        let a3 = authorities.next().unwrap();
        let a4 = authorities.next().unwrap();

        let low_scoring = Arc::new(ArcSwap::from_pointee(HashMap::new()));

        let mut inner = HashMap::new();
        inner.insert(a1.protocol_key().into(), 50);
        let reputation_scores_1 = ReputationScores {
            scores_per_authority: Default::default(),
            final_of_schedule: false,
        };
        low_scoring.swap(Arc::new(inner));
        let peer_id_map = HashMap::new();

        let metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));

        // when final of schedule is false, calling update_low_scoring_authorities will not change the
        // low_scoring_authorities map
        update_low_scoring_authorities(
            low_scoring.clone(),
            &committee,
            reputation_scores_1,
            peer_id_map.clone(),
            &metrics,
            &protocol_v4(),
        );

        assert_eq!(
            *low_scoring.load().get(&a1.protocol_key().into()).unwrap(),
            50_u64
        );
        assert_eq!(low_scoring.load().len(), 1);

        // there is a clear low outlier in the scores, exclude it
        let mut scores = HashMap::new();
        scores.insert(a1.id(), 607_u64);
        scores.insert(a2.id(), 611_u64);
        scores.insert(a3.id(), 607_u64);
        scores.insert(a4.id(), 455_u64);
        let reputation_scores = ReputationScores {
            scores_per_authority: scores,
            final_of_schedule: true,
        };

        update_low_scoring_authorities(
            low_scoring.clone(),
            &committee,
            reputation_scores,
            peer_id_map.clone(),
            &metrics,
            &protocol_v4(),
        );
        assert_eq!(
            *low_scoring.load().get(&a4.protocol_key().into()).unwrap(),
            455_u64
        );
        assert_eq!(low_scoring.load().len(), 1);

        // one authority has score which is a bit lower, but should not be excluded
        let mut scores = HashMap::new();
        scores.insert(a1.id(), 607_u64);
        scores.insert(a2.id(), 751_u64);
        scores.insert(a3.id(), 707_u64);
        scores.insert(a4.id(), 650_u64);
        let reputation_scores = ReputationScores {
            scores_per_authority: scores,
            final_of_schedule: true,
        };

        update_low_scoring_authorities(
            low_scoring.clone(),
            &committee,
            reputation_scores.clone(),
            peer_id_map.clone(),
            &metrics,
            &protocol_v4(),
        );
        assert_eq!(low_scoring.load().len(), 0);

        // When with protocol 5
        update_low_scoring_authorities(
            low_scoring.clone(),
            &committee,
            reputation_scores,
            peer_id_map.clone(),
            &metrics,
            &protocol_v5(),
        );
        assert_eq!(low_scoring.load().len(), 1);

        // this set of scores has a high performing outlier, we don't exclude it
        let mut scores = HashMap::new();
        scores.insert(a1.id(), 900_u64);
        scores.insert(a2.id(), 257_u64);
        scores.insert(a3.id(), 240_u64);
        scores.insert(a4.id(), 200_u64);
        let reputation_scores = ReputationScores {
            scores_per_authority: scores,
            final_of_schedule: true,
        };

        update_low_scoring_authorities(
            low_scoring.clone(),
            &committee,
            reputation_scores.clone(),
            peer_id_map.clone(),
            &metrics,
            &protocol_v4(),
        );

        assert_eq!(low_scoring.load().len(), 0);

        // when with protocol 5
        update_low_scoring_authorities(
            low_scoring.clone(),
            &committee,
            reputation_scores,
            peer_id_map.clone(),
            &metrics,
            &protocol_v5(),
        );

        assert_eq!(low_scoring.load().len(), 1);

        // if more than the quorum is a low outlier, we don't exclude any authority
        let mut scores = HashMap::new();
        scores.insert(a1.id(), 450_u64);
        scores.insert(a2.id(), 490_u64);
        scores.insert(a3.id(), 10_u64);
        scores.insert(a4.id(), 0_u64);
        let reputation_scores = ReputationScores {
            scores_per_authority: scores,
            final_of_schedule: true,
        };

        update_low_scoring_authorities(
            low_scoring.clone(),
            &committee,
            reputation_scores.clone(),
            peer_id_map.clone(),
            &metrics,
            &protocol_v4(),
        );
        assert_eq!(low_scoring.load().len(), 0);

        // When protocol v5
        update_low_scoring_authorities(
            low_scoring.clone(),
            &committee,
            reputation_scores,
            peer_id_map.clone(),
            &metrics,
            &protocol_v5(),
        );
        assert_eq!(low_scoring.load().len(), 1);

        // test with large cluster
        let num_nodes = 50;
        let final_idx = num_nodes - 1;

        let committee = generate_committee(num_nodes);
        let authorities: Vec<Authority> = committee.authorities().cloned().collect();

        let low_scoring = Arc::new(ArcSwap::from_pointee(HashMap::new()));
        let mut scores = HashMap::new();
        // scores clustered between 100 - 110
        for (i, authority) in authorities.iter().enumerate().take(num_nodes - 1) {
            let score_add = i / 5;

            scores.insert(authority.id(), 100_u64 + (score_add as u64));
        }
        // the non-outlier
        let outlier_id = authorities[final_idx].id();
        scores.insert(outlier_id, 190_u64);

        let reputation_scores = ReputationScores {
            scores_per_authority: scores.clone(),
            final_of_schedule: true,
        };

        update_low_scoring_authorities(
            low_scoring.clone(),
            &committee,
            reputation_scores.clone(),
            peer_id_map.clone(),
            &metrics,
            &protocol_v4(),
        );

        assert_eq!(low_scoring.load().len(), 0);

        // When protocol v5
        update_low_scoring_authorities(
            low_scoring.clone(),
            &committee,
            reputation_scores,
            peer_id_map.clone(),
            &metrics,
            &protocol_v5(),
        );

        assert_eq!(low_scoring.load().len(), 10);

        // the outlier
        scores.insert(authorities[final_idx].id(), 40_u64);
        let reputation_scores = ReputationScores {
            scores_per_authority: scores,
            final_of_schedule: true,
        };
        update_low_scoring_authorities(
            low_scoring.clone(),
            &committee,
            reputation_scores.clone(),
            peer_id_map.clone(),
            &metrics,
            &protocol_v4(),
        );

        assert_eq!(
            *low_scoring
                .load()
                .get(&authorities[final_idx].protocol_key().into())
                .unwrap(),
            40_u64
        );

        // When protocol 5
        update_low_scoring_authorities(
            low_scoring.clone(),
            &committee,
            reputation_scores,
            peer_id_map,
            &metrics,
            &protocol_v4(),
        );

        assert_eq!(
            *low_scoring
                .load()
                .get(&authorities[final_idx].protocol_key().into())
                .unwrap(),
            40_u64
        );
        assert_eq!(low_scoring.load().len(), 1);
    }

    #[test]
    pub fn test_update_low_scoring_authorities_with_down_node() {
        let committee = generate_committee(8);
        let mut authorities = committee.authorities();
        let a1 = authorities.next().unwrap();
        let a2 = authorities.next().unwrap();
        let a3 = authorities.next().unwrap();
        let a4 = authorities.next().unwrap();
        let a5 = authorities.next().unwrap();
        let a6 = authorities.next().unwrap();
        let a7 = authorities.next().unwrap();
        let a8 = authorities.next().unwrap();

        let low_scoring = Arc::new(ArcSwap::from_pointee(HashMap::new()));

        let mut inner = HashMap::new();
        inner.insert(a1.protocol_key().into(), 50);

        low_scoring.swap(Arc::new(inner));

        let metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));

        // there is a low outlier in the non zero scores, exclude it as well as down nodes
        let mut scores = HashMap::new();
        scores.insert(a1.id(), 350_u64);
        scores.insert(a2.id(), 390_u64);
        scores.insert(a3.id(), 350_u64);
        scores.insert(a4.id(), 50_u64);
        scores.insert(a5.id(), 0_u64); // down node
        scores.insert(a6.id(), 300_u64);
        scores.insert(a7.id(), 340_u64);
        scores.insert(a8.id(), 310_u64);
        let reputation_scores = ReputationScores {
            scores_per_authority: scores,
            final_of_schedule: true,
        };

        update_low_scoring_authorities(
            low_scoring.clone(),
            &committee,
            reputation_scores.clone(),
            HashMap::new(),
            &metrics,
            &protocol_v4(),
        );
        assert_eq!(
            *low_scoring.load().get(&a4.protocol_key().into()).unwrap(),
            50_u64
        );
        assert_eq!(
            *low_scoring.load().get(&a5.protocol_key().into()).unwrap(),
            0_u64
        );
        assert_eq!(low_scoring.load().len(), 2);

        // When protocol v5
        update_low_scoring_authorities(
            low_scoring.clone(),
            &committee,
            reputation_scores,
            HashMap::new(),
            &metrics,
            &protocol_v4(),
        );
        assert_eq!(
            *low_scoring.load().get(&a4.protocol_key().into()).unwrap(),
            50_u64
        );
        assert_eq!(
            *low_scoring.load().get(&a5.protocol_key().into()).unwrap(),
            0_u64
        );
        assert_eq!(low_scoring.load().len(), 2);
    }

    #[test]
    pub fn test_limit_low_scoring_to_validity_threshold() {
        let committee = generate_committee(10);
        let mut authorities = committee.authorities();
        let a1 = authorities.next().unwrap();
        let a2 = authorities.next().unwrap();
        let a3 = authorities.next().unwrap();
        let a4 = authorities.next().unwrap();
        let a5 = authorities.next().unwrap();
        let a6 = authorities.next().unwrap();
        let a7 = authorities.next().unwrap();
        let a8 = authorities.next().unwrap();
        let a9 = authorities.next().unwrap();
        let a10 = authorities.next().unwrap();

        let low_scoring = Arc::new(ArcSwap::from_pointee(HashMap::new()));

        low_scoring.swap(Arc::new(HashMap::new()));

        let metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));

        // there is a low outlier in the non zero scores, exclude it as well as down nodes
        let mut scores = HashMap::new();
        scores.insert(a1.id(), 350_u64);
        scores.insert(a2.id(), 390_u64);
        scores.insert(a3.id(), 350_u64);
        scores.insert(a4.id(), 20_u64);
        scores.insert(a5.id(), 10_u64);
        scores.insert(a6.id(), 0_u64);
        scores.insert(a7.id(), 0_u64);
        scores.insert(a8.id(), 0_u64);
        scores.insert(a9.id(), 0_u64);
        scores.insert(a10.id(), 0_u64);
        let reputation_scores = ReputationScores {
            scores_per_authority: scores,
            final_of_schedule: true,
        };

        update_low_scoring_authorities(
            low_scoring.clone(),
            &committee,
            reputation_scores.clone(),
            HashMap::new(),
            &metrics,
            &protocol_v4(),
        );
        assert_eq!(low_scoring.load().len(), 0);

        // When protocol v5
        update_low_scoring_authorities(
            low_scoring.clone(),
            &committee,
            reputation_scores,
            HashMap::new(),
            &metrics,
            &protocol_v5(),
        );
        assert_eq!(low_scoring.load().len(), 3);
    }

    /// Generate a random committee for the given size. It's important to create the Authorities
    /// via the committee to ensure than an AuthorityIdentifier will be assigned, as this is dynamically
    /// calculated during committee creation.
    fn generate_committee(committee_size: usize) -> Arc<Committee> {
        let mut committee_builder = CommitteeBuilder::new(0);
        let mut rng = StdRng::from_rng(&mut OsRng).unwrap();

        for i in 0..committee_size {
            let pair = KeyPair::generate(&mut rng);
            let public_key = pair.public().clone();

            committee_builder = committee_builder.add_authority(
                public_key.clone(),
                1,
                Multiaddr::empty(),
                NetworkPublicKey::insecure_default(),
                i.to_string(),
            );
        }

        Arc::new(committee_builder.build())
    }

    #[test]
    pub fn test_update_low_scoring_authorities_with_large_score_variance() {
        // test with large cluster
        let num_nodes = 50;
        let final_idx = num_nodes - 1;

        let committee = generate_committee(num_nodes);
        let authorities: Vec<Authority> = committee.authorities().cloned().collect();

        let low_scoring = Arc::new(ArcSwap::from_pointee(HashMap::new()));
        let mut scores = HashMap::new();

        let metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));

        // scores clustered between 600 - 800
        for (i, authority) in authorities.iter().enumerate().take(num_nodes - 1) {
            let score_add = i * 5;

            scores.insert(
                authority.id(),
                600_u64 + (std::cmp::min(score_add as u64, 200)),
            );
        }
        // the outliers
        let outlier_id = authorities[final_idx].id();
        scores.insert(outlier_id, 550_u64);
        let outlier_id = authorities[final_idx - 1].id();
        scores.insert(outlier_id, 540_u64);
        let outlier_id = authorities[final_idx - 2].id();
        scores.insert(outlier_id, 0_u64);

        let reputation_scores = ReputationScores {
            scores_per_authority: scores.clone(),
            final_of_schedule: true,
        };

        let peer_id_map = HashMap::new();
        update_low_scoring_authorities(
            low_scoring.clone(),
            &committee,
            reputation_scores.clone(),
            peer_id_map.clone(),
            &metrics,
            &protocol_v4(),
        );

        assert_eq!(low_scoring.load().len(), 3);

        // When protocol v5
        update_low_scoring_authorities(
            low_scoring.clone(),
            &committee,
            reputation_scores,
            peer_id_map,
            &metrics,
            &protocol_v5(),
        );

        assert_eq!(low_scoring.load().len(), 12);
    }

    /// A test to use when need to tune the score parameters based on some score data retrieved by
    /// and external environment (ex devnet, testnet etc). A CSV file where the first line is the
    /// header with the host names, and every other line the scores for each authority on the
    /// instance of time. Example:
    /// "validator-0","validator-10","validator-11",...
    /// 184,185,144,...
    ///
    /// The method prints a matrix of the pruned hosts for each score "round"
    /// and the corresponding score. When a node has not been pruned for a round then the corresponding
    /// score will appear as "--".
    #[test]
    #[ignore]
    pub fn test_update_low_scoring_authorities_with_score_data() {
        use std::collections::{BTreeMap, HashMap};

        // read the file
        let (authority_host_names, all_authority_scores) = read_scores_csv("example.csv");

        let mut authority_names_to_hostnames = HashMap::new();

        // construct the committee
        let low_scoring = Arc::new(ArcSwap::from_pointee(HashMap::new()));
        let metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));
        let committee = generate_committee(authority_host_names.len());
        let mut authorities = committee.authorities();

        // associate authorities to host names
        for name in authority_host_names.iter().take(committee.size()) {
            let authority = authorities.next().unwrap();
            authority_names_to_hostnames.insert(authority.protocol_key().into(), name.clone());
        }

        let mut low_scoring_authorities_per_round: BTreeMap<String, BTreeMap<usize, Option<u64>>> =
            BTreeMap::new();

        // Now iterate over the scores
        for (index, authority_scores) in all_authority_scores.iter().enumerate() {
            let mut scores = HashMap::new();
            let mut authorities = committee.authorities();

            // create the scores map and just map them to some authorities
            for score in authority_scores.iter().take(committee.size()) {
                let authority = authorities.next().unwrap();
                scores.insert(authority.id(), *score);
            }

            let reputation_scores = ReputationScores {
                scores_per_authority: scores,
                final_of_schedule: true,
            };

            update_low_scoring_authorities(
                low_scoring.clone(),
                &committee,
                reputation_scores,
                authority_names_to_hostnames.clone(),
                &metrics,
                &protocol_v5(),
            );

            // snapshot the low scoring authorities
            for (name, host_name) in &authority_names_to_hostnames {
                if let Some(score) = low_scoring.load().get(name) {
                    low_scoring_authorities_per_round
                        .entry(host_name.clone())
                        .or_default()
                        .insert(index, Some(*score));
                } else {
                    // it's not a low scoring authority
                    low_scoring_authorities_per_round
                        .entry(host_name.clone())
                        .or_default()
                        .insert(index, None);
                }
            }
        }

        // Now print the authorities per "round"
        let mut total_low_scoring_authorities_per_round: BTreeMap<usize, usize> = BTreeMap::new();
        for (host_name, scores_per_round) in low_scoring_authorities_per_round {
            // quickly check if this host has been marked ever as low scoring - otherwise skip
            if scores_per_round.iter().any(|(_, value)| value.is_some()) {
                print!("{:30}", host_name);

                for (index, score) in scores_per_round {
                    if let Some(score) = score {
                        print!("{score:0>2}\t");
                        total_low_scoring_authorities_per_round
                            .entry(index)
                            .and_modify(|e| *e += 1)
                            .or_insert(1);
                    } else {
                        total_low_scoring_authorities_per_round
                            .entry(index)
                            .or_insert(0);
                        print!("--\t");
                    }
                }
                println!();
            }
        }

        // print the total low scoring authorities
        println!("---------------");
        print!("{:30}", "Total");
        for (_index, total) in total_low_scoring_authorities_per_round {
            print!("{total:0>2}\t");
        }
    }

    fn read_scores_csv(path: &str) -> (Vec<String>, Vec<Vec<u64>>) {
        use std::fs::File;
        use std::io::BufRead;
        use std::io::BufReader;

        let file = File::open(path).unwrap();
        let reader = BufReader::new(file);

        let mut headers = Vec::new();
        let mut scores = Vec::new();
        let mut scores_set: HashSet<u64> = HashSet::new();

        for line in reader.lines() {
            if headers.is_empty() {
                headers = line.unwrap().split(',').map(|s| s.to_string()).collect();
            } else {
                let l = line.unwrap();

                let s: Vec<u64> = l
                    .split(',')
                    .map(|s| s.parse::<f64>().unwrap() as u64)
                    .collect();

                if s.is_empty() {
                    continue;
                }

                if scores_set.insert(s.iter().sum()) {
                    scores.push(s);
                }
            }
        }

        (headers, scores)
    }
}
