// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::{collections::HashMap, sync::Arc};

use arc_swap::ArcSwap;
use consensus_config::Committee as ConsensusCommittee;
use sui_types::{
    base_types::AuthorityName, committee::Committee, messages_consensus::AuthorityIndex,
};
use tracing::debug;

use crate::authority::AuthorityMetrics;

/// Updates list of authorities that are deemed to have low reputation scores by consensus
/// these may be lagging behind the network, byzantine, or not reliably participating for any reason.
/// The algorithm is flagging as low scoring authorities all the validators that have the lowest scores
/// up to the defined protocol_config.consensus_bad_nodes_stake_threshold. This is done to align the
/// submission side with the consensus leader election schedule. Practically we don't want to submit
/// transactions for sequencing to validators that have low scores and are not part of the leader
/// schedule since the chances of getting them sequenced are lower.
pub(crate) fn update_low_scoring_authorities(
    low_scoring_authorities: Arc<ArcSwap<HashMap<AuthorityName, u64>>>,
    sui_committee: &Committee,
    consensus_committee: &ConsensusCommittee,
    reputation_score_sorted_desc: Option<Vec<(AuthorityIndex, u64)>>,
    metrics: &Arc<AuthorityMetrics>,
    consensus_bad_nodes_stake_threshold: u64,
) {
    assert!((0..=33).contains(&consensus_bad_nodes_stake_threshold), "The bad_nodes_stake_threshold should be in range [0 - 33], out of bounds parameter detected {}", consensus_bad_nodes_stake_threshold);

    let Some(reputation_scores) = reputation_score_sorted_desc else {
        return;
    };

    // We order the authorities by score ascending order in the exact same way as the reputation
    // scores do - so we keep complete alignment between implementations
    let scores_per_authority_order_asc: Vec<_> = reputation_scores
        .into_iter()
        .rev() // we reverse so we get them in asc order
        .collect();

    let mut final_low_scoring_map = HashMap::new();
    let mut total_stake = 0;
    for (index, score) in scores_per_authority_order_asc {
        let authority_name = sui_committee.authority_by_index(index).unwrap();
        let authority_index = consensus_committee
            .to_authority_index(index as usize)
            .unwrap();
        let consensus_authority = consensus_committee.authority(authority_index);
        let hostname = &consensus_authority.hostname;
        let stake = consensus_authority.stake;
        total_stake += stake;

        let included = if total_stake
            <= consensus_bad_nodes_stake_threshold * consensus_committee.total_stake() / 100
        {
            final_low_scoring_map.insert(*authority_name, score);
            true
        } else {
            false
        };

        if !hostname.is_empty() {
            debug!(
                "authority {} has score {}, is low scoring: {}",
                hostname, score, included
            );

            metrics
                .consensus_handler_scores
                .with_label_values(&[hostname])
                .set(score as i64);
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
    use std::{collections::HashMap, sync::Arc};

    use arc_swap::ArcSwap;
    use consensus_config::{local_committee_and_keys, Committee as ConsensusCommittee};
    use prometheus::Registry;
    use sui_types::{committee::Committee, crypto::AuthorityPublicKeyBytes};

    use crate::{authority::AuthorityMetrics, scoring_decision::update_low_scoring_authorities};

    #[test]
    #[cfg_attr(msim, ignore)]
    pub fn test_update_low_scoring_authorities() {
        // GIVEN
        // Total stake is 8 for this committee and every authority has equal stake = 1
        let (sui_committee, consensus_committee) = generate_committees(8);

        let low_scoring = Arc::new(ArcSwap::from_pointee(HashMap::new()));
        let metrics = Arc::new(AuthorityMetrics::new(&Registry::new()));

        // there is a low outlier in the non zero scores, exclude it as well as down nodes
        let authorities_by_score_desc = vec![
            (1, 390_u64),
            (0, 350_u64),
            (6, 340_u64),
            (7, 310_u64),
            (5, 300_u64),
            (3, 50_u64),
            (2, 50_u64),
            (4, 0_u64), // down node
        ];

        // WHEN
        let consensus_bad_nodes_stake_threshold = 33; // 33 * 8 / 100 = 2 low scoring validator

        update_low_scoring_authorities(
            low_scoring.clone(),
            &sui_committee,
            &consensus_committee,
            Some(authorities_by_score_desc.clone()),
            &metrics,
            consensus_bad_nodes_stake_threshold,
        );

        // THEN
        assert_eq!(low_scoring.load().len(), 2);
        assert_eq!(
            *low_scoring
                .load()
                // authority 2 is 2nd to the last in authorities_by_score_desc
                .get(sui_committee.authority_by_index(2).unwrap())
                .unwrap(),
            50
        );
        assert_eq!(
            *low_scoring
                .load()
                // authority 4 is the last in authorities_by_score_desc
                .get(sui_committee.authority_by_index(4).unwrap())
                .unwrap(),
            0
        );

        // WHEN setting the threshold to lower
        let consensus_bad_nodes_stake_threshold = 20; // 20 * 8 / 100 = 1 low scoring validator
        update_low_scoring_authorities(
            low_scoring.clone(),
            &sui_committee,
            &consensus_committee,
            Some(authorities_by_score_desc.clone()),
            &metrics,
            consensus_bad_nodes_stake_threshold,
        );

        // THEN
        assert_eq!(low_scoring.load().len(), 1);
        assert_eq!(
            *low_scoring
                .load()
                .get(sui_committee.authority_by_index(4).unwrap())
                .unwrap(),
            0
        );
    }

    /// Generate a pair of Sui and consensus committees for the given size.
    fn generate_committees(committee_size: usize) -> (Committee, ConsensusCommittee) {
        let (consensus_committee, _) = local_committee_and_keys(0, vec![1; committee_size]);

        let public_keys = consensus_committee
            .authorities()
            .map(|(_i, authority)| authority.authority_key.inner())
            .collect::<Vec<_>>();
        let sui_authorities = public_keys
            .iter()
            .map(|key| (AuthorityPublicKeyBytes::from(*key), 1))
            .collect::<Vec<_>>();
        let sui_committee = Committee::new_for_testing_with_normalized_voting_power(
            0,
            sui_authorities.iter().cloned().collect(),
        );

        (sui_committee, consensus_committee)
    }
}
