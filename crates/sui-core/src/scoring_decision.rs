// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityMetrics;
use crate::math::median;
use arc_swap::ArcSwap;
use narwhal_types::ReputationScores;
use std::collections::HashMap;
use std::sync::Arc;
use sui_types::base_types::AuthorityName;
use sui_types::committee::Committee;
use tracing::{debug, info, warn};

// TODO: migrate these values to config
const MAD_DIVISOR: f64 = 0.7;
const CUTOFF_VALUE: f64 = 2.4;

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
/// Once we have that value, if any authority's absolute deviation / ( MAD / C) < -K then it is deemed
/// to be a low-value outlier. The values of C and K can be tweaked to change the sensitivity to outliers.
/// They were chosen based on trial and error to produce reasonable results for score values in the
/// order of magnitude of 10s - 1000s. If you increase C and decrease K you will see more values
/// being included as outliers. As the scores get higher in value, outlier sensitivity tends to
/// decrease using this method.
///
/// If we find that we have rated enough validators as low scoring such that we no longer have
/// quorum with the remaining validators, then we either need to update this method's parameters,
/// our general approach to finding outliers, or our network is in a bad state. If we need to update
/// this code, we let it detect this and disable itself for safety reasons. If we have a bad network
/// state then in the interest of making debugging and investigation easier, disabling the scoring
/// mechanism will likely be helpful.
pub fn update_low_scoring_authorities(
    low_scoring_authorities: Arc<ArcSwap<HashMap<AuthorityName, u64>>>,
    committee: Arc<Committee>,
    reputation_scores: ReputationScores,
    metrics: &Arc<AuthorityMetrics>,
) {
    if !reputation_scores.final_of_schedule {
        return;
    }

    let mut final_low_scoring_map = HashMap::new();

    let mut score_list = vec![];
    let mut non_zero_scores = vec![];
    for val in reputation_scores.scores_per_authority.values() {
        score_list.push(*val as f64);
        if *val > 0 {
            non_zero_scores.push(*val as f64);
        }
    }

    let median_value = median(&non_zero_scores);
    let mut deviations = vec![];
    let mut abs_deviations = vec![];
    for (i, _) in score_list.clone().iter().enumerate() {
        deviations.push(score_list[i] - median_value);
        if score_list[i] > 0.0 {
            abs_deviations.push((score_list[i] - median_value).abs());
        }
    }

    // adjusted median absolute deviation
    let mad = median(&abs_deviations) / MAD_DIVISOR;
    let mut low_scoring = vec![];
    let mut rest = vec![];
    for (i, (a, _)) in reputation_scores.scores_per_authority.iter().enumerate() {
        let temp = deviations[i] / mad;
        if temp < -CUTOFF_VALUE {
            low_scoring.push(a);
        } else {
            rest.push(AuthorityName::from(a));
        }
    }

    // report new scores
    let len_low_scoring = low_scoring.len();
    metrics
        .consensus_handler_num_low_scoring_authorities
        .set(len_low_scoring as i64);

    reputation_scores
        .scores_per_authority
        .iter()
        .for_each(|(a, s)| {
            let name = AuthorityName::from(a);
            debug!("authority {} has score {}", name, s);
            metrics.consensus_handler_scores.observe(*s as f64);
        });

    info!("{:?} low scoring authorities calculated", len_low_scoring);

    for authority in low_scoring {
        let name = AuthorityName::from(authority);
        let score = *reputation_scores
            .scores_per_authority
            .get(authority)
            .unwrap();
        final_low_scoring_map.insert(name, score);
        debug!("low scoring authority {} has score {}", name, score);
    }

    // make sure the rest have at least quorum
    let remaining_stake = rest.iter().map(|a| committee.weight(a)).sum::<u64>();
    let quorum_threshold = committee.threshold::<true>();
    if remaining_stake < quorum_threshold {
        warn!(
            "too many low reputation-scoring authorities, temporarily disabling scoring mechanism"
        );

        low_scoring_authorities.swap(Arc::new(HashMap::new()));
        return;
    }

    low_scoring_authorities.swap(Arc::new(final_low_scoring_map));
}

#[test]
pub fn test_update_low_scoring_authorities() {
    #![allow(clippy::mutable_key_type)]
    use fastcrypto::traits::KeyPair;
    use prometheus::Registry;
    use std::collections::BTreeMap;
    use std::collections::HashMap;
    use sui_types::crypto::{get_key_pair, AuthorityKeyPair};

    let (_, sec1): (_, AuthorityKeyPair) = get_key_pair();
    let (_, sec2): (_, AuthorityKeyPair) = get_key_pair();
    let (_, sec3): (_, AuthorityKeyPair) = get_key_pair();
    let (_, sec4): (_, AuthorityKeyPair) = get_key_pair();
    let a1: AuthorityName = sec1.public().into();
    let a2: AuthorityName = sec2.public().into();
    let a3: AuthorityName = sec3.public().into();
    let a4: AuthorityName = sec4.public().into();

    let mut authorities = BTreeMap::new();
    authorities.insert(a1, 1);
    authorities.insert(a2, 1);
    authorities.insert(a3, 1);
    authorities.insert(a4, 1);
    let committee = Arc::new(Committee::new(0, authorities));

    let low_scoring = Arc::new(ArcSwap::new(Arc::new(HashMap::new())));

    let mut inner = HashMap::new();
    inner.insert(a1, 50);
    let reputation_scores_1 = ReputationScores {
        scores_per_authority: Default::default(),
        final_of_schedule: false,
    };
    low_scoring.swap(Arc::new(inner));

    let metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));

    // when final of schedule is false, calling update_low_scoring_authorities will not change the
    // low_scoring_authorities map
    update_low_scoring_authorities(
        low_scoring.clone(),
        committee.clone(),
        reputation_scores_1,
        &metrics,
    );
    assert_eq!(*low_scoring.load().get(&a1).unwrap(), 50_u64);
    assert_eq!(low_scoring.load().len(), 1);

    // there is a clear low outlier in the scores, exclude it
    let mut scores = HashMap::new();
    scores.insert(sec1.public().clone(), 45_u64);
    scores.insert(sec2.public().clone(), 49_u64);
    scores.insert(sec3.public().clone(), 55_u64);
    scores.insert(sec4.public().clone(), 25_u64);
    let reputation_scores = ReputationScores {
        scores_per_authority: scores,
        final_of_schedule: true,
    };

    update_low_scoring_authorities(
        low_scoring.clone(),
        committee.clone(),
        reputation_scores,
        &metrics,
    );
    assert_eq!(*low_scoring.load().get(&a4).unwrap(), 25_u64);
    assert_eq!(low_scoring.load().len(), 1);

    // a4 has score of 30 which is a bit lower, but not an outlier, so it should not be excluded
    let mut scores = HashMap::new();
    scores.insert(sec1.public().clone(), 45_u64);
    scores.insert(sec2.public().clone(), 49_u64);
    scores.insert(sec3.public().clone(), 55_u64);
    scores.insert(sec4.public().clone(), 30_u64);
    let reputation_scores = ReputationScores {
        scores_per_authority: scores,
        final_of_schedule: true,
    };

    update_low_scoring_authorities(
        low_scoring.clone(),
        committee.clone(),
        reputation_scores,
        &metrics,
    );
    assert_eq!(low_scoring.load().len(), 0);

    // this set of scores has a high performing outlier, we don't exclude it
    let mut scores = HashMap::new();
    scores.insert(sec1.public().clone(), 45_u64);
    scores.insert(sec2.public().clone(), 49_u64);
    scores.insert(sec3.public().clone(), 55_u64);
    scores.insert(sec4.public().clone(), 80_u64);
    let reputation_scores = ReputationScores {
        scores_per_authority: scores,
        final_of_schedule: true,
    };

    update_low_scoring_authorities(
        low_scoring.clone(),
        committee.clone(),
        reputation_scores,
        &metrics,
    );
    assert_eq!(low_scoring.load().len(), 0);

    // if more than the quorum is a low outlier, we don't exclude any authority
    let mut scores = HashMap::new();
    scores.insert(sec1.public().clone(), 45_u64);
    scores.insert(sec2.public().clone(), 49_u64);
    scores.insert(sec3.public().clone(), 16_u64);
    scores.insert(sec4.public().clone(), 25_u64);
    let reputation_scores = ReputationScores {
        scores_per_authority: scores,
        final_of_schedule: true,
    };

    update_low_scoring_authorities(
        low_scoring.clone(),
        committee.clone(),
        reputation_scores,
        &metrics,
    );
    assert_eq!(low_scoring.load().len(), 0);

    // the computation can handle score values at larger scale
    let mut scores = HashMap::new();
    scores.insert(sec1.public().clone(), 2300_u64);
    scores.insert(sec2.public().clone(), 3000_u64);
    scores.insert(sec3.public().clone(), 900_u64);
    scores.insert(sec4.public().clone(), 1900_u64);
    let reputation_scores = ReputationScores {
        scores_per_authority: scores,
        final_of_schedule: true,
    };

    update_low_scoring_authorities(
        low_scoring.clone(),
        committee.clone(),
        reputation_scores,
        &metrics,
    );
    assert_eq!(low_scoring.load().len(), 0);

    // the computation can handle score values scaled up
    let mut scores = HashMap::new();
    scores.insert(sec1.public().clone(), 2300_u64);
    scores.insert(sec2.public().clone(), 3000_u64);
    scores.insert(sec3.public().clone(), 210_u64);
    scores.insert(sec4.public().clone(), 1900_u64);
    let reputation_scores = ReputationScores {
        scores_per_authority: scores,
        final_of_schedule: true,
    };

    update_low_scoring_authorities(low_scoring.clone(), committee, reputation_scores, &metrics);
    assert_eq!(*low_scoring.load().get(&a3).unwrap(), 210_u64);
    assert_eq!(low_scoring.load().len(), 1);

    // test with large cluster
    let mut secs = Vec::new();
    let mut authority_names = Vec::new();
    let mut authorities = BTreeMap::new();
    let num_nodes = 50;
    let final_idx = num_nodes - 1;

    for _i in 0..num_nodes {
        let (_, sec1): (_, AuthorityKeyPair) = get_key_pair();
        let a: AuthorityName = sec1.public().into();
        secs.push(sec1);
        authority_names.push(a);
        authorities.insert(a, 1);
    }

    let committee = Arc::new(Committee::new(0, authorities));
    let low_scoring = Arc::new(ArcSwap::new(Arc::new(HashMap::new())));
    let mut scores = HashMap::new();
    // scores clustered between 100 - 110
    for i in 0..num_nodes - 1 {
        let score_add = i / 5;
        scores.insert(
            secs[i as usize].public().clone(),
            100_u64 + (score_add as u64),
        );
    }
    // the outlier
    scores.insert(secs[final_idx].public().clone(), 70_u64);

    let reputation_scores = ReputationScores {
        scores_per_authority: scores,
        final_of_schedule: true,
    };

    update_low_scoring_authorities(low_scoring.clone(), committee, reputation_scores, &metrics);
    assert_eq!(
        *low_scoring.load().get(&authority_names[final_idx]).unwrap(),
        70_u64
    );
    assert_eq!(low_scoring.load().len(), 1);
}

#[test]
pub fn test_update_low_scoring_authorities_with_down_node() {
    #![allow(clippy::mutable_key_type)]
    use fastcrypto::traits::KeyPair;
    use prometheus::Registry;
    use std::collections::BTreeMap;
    use std::collections::HashMap;
    use sui_types::crypto::{get_key_pair, AuthorityKeyPair};

    let (_, sec1): (_, AuthorityKeyPair) = get_key_pair();
    let (_, sec2): (_, AuthorityKeyPair) = get_key_pair();
    let (_, sec3): (_, AuthorityKeyPair) = get_key_pair();
    let (_, sec4): (_, AuthorityKeyPair) = get_key_pair();
    let (_, sec5): (_, AuthorityKeyPair) = get_key_pair();
    let (_, sec6): (_, AuthorityKeyPair) = get_key_pair();
    let (_, sec7): (_, AuthorityKeyPair) = get_key_pair();
    let (_, sec8): (_, AuthorityKeyPair) = get_key_pair();
    let a1: AuthorityName = sec1.public().into();
    let a2: AuthorityName = sec2.public().into();
    let a3: AuthorityName = sec3.public().into();
    let a4: AuthorityName = sec4.public().into();
    let a5: AuthorityName = sec5.public().into();
    let a6: AuthorityName = sec6.public().into();
    let a7: AuthorityName = sec7.public().into();
    let a8: AuthorityName = sec8.public().into();

    let mut authorities = BTreeMap::new();
    authorities.insert(a1, 1);
    authorities.insert(a2, 1);
    authorities.insert(a3, 1);
    authorities.insert(a4, 1);
    authorities.insert(a5, 1);
    authorities.insert(a6, 1);
    authorities.insert(a7, 1);
    authorities.insert(a8, 1);
    let committee = Arc::new(Committee::new(0, authorities));

    let low_scoring = Arc::new(ArcSwap::new(Arc::new(HashMap::new())));

    let mut inner = HashMap::new();
    inner.insert(a1, 50);

    low_scoring.swap(Arc::new(inner));

    let metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));

    // there is a low outlier in the non zero scores, exclude it as well as down nodes
    let mut scores = HashMap::new();
    scores.insert(sec1.public().clone(), 45_u64);
    scores.insert(sec2.public().clone(), 49_u64);
    scores.insert(sec3.public().clone(), 55_u64);
    scores.insert(sec4.public().clone(), 35_u64);
    scores.insert(sec5.public().clone(), 0_u64); // down node
    scores.insert(sec6.public().clone(), 50_u64);
    scores.insert(sec7.public().clone(), 54_u64);
    scores.insert(sec8.public().clone(), 51_u64);
    let reputation_scores = ReputationScores {
        scores_per_authority: scores,
        final_of_schedule: true,
    };

    update_low_scoring_authorities(low_scoring.clone(), committee, reputation_scores, &metrics);
    assert_eq!(*low_scoring.load().get(&a4).unwrap(), 35_u64);
    assert_eq!(*low_scoring.load().get(&a5).unwrap(), 0_u64);
    assert_eq!(low_scoring.load().len(), 2);
}
