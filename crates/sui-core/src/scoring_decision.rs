// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::{collections::HashMap, sync::Arc};

use consensus_config::Committee as ConsensusCommittee;
use sui_types::{
    base_types::AuthorityName, committee::Committee, messages_consensus::AuthorityIndex,
};
use tracing::debug;

use crate::authority::AuthorityMetrics;

/// Identifies the authorities deemed to have low reputation scores by consensus — lagging behind,
/// byzantine, or not reliably participating. Flags authorities with the lowest scores up to
/// `consensus_bad_nodes_stake_threshold` (percent of total stake). Emits per-authority score
/// metrics and a count gauge.
/// The list of lower-scoring authorities are returned for testing.
pub(crate) fn update_low_scoring_authorities_metrics(
    sui_committee: &Committee,
    consensus_committee: &ConsensusCommittee,
    reputation_score_sorted_desc: Option<Vec<(AuthorityIndex, u64)>>,
    metrics: &Arc<AuthorityMetrics>,
    consensus_bad_nodes_stake_threshold: u64,
) -> HashMap<AuthorityName, u64> {
    assert!(
        (0..=33).contains(&consensus_bad_nodes_stake_threshold),
        "The bad_nodes_stake_threshold should be in range [0 - 33], out of bounds parameter detected {}",
        consensus_bad_nodes_stake_threshold
    );

    let mut final_low_scoring_map = HashMap::new();
    let Some(reputation_scores) = reputation_score_sorted_desc else {
        return final_low_scoring_map;
    };

    // Iterate authorities by score ascending (reverse of the supplied descending order) so the
    // stake budget fills starting from the worst-scoring validators.
    let scores_per_authority_order_asc = reputation_scores.into_iter().rev();

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
    metrics
        .consensus_handler_num_low_scoring_authorities
        .set(final_low_scoring_map.len() as i64);
    final_low_scoring_map
}

#[cfg(test)]
mod tests {
    #![allow(clippy::mutable_key_type)]
    use std::sync::Arc;

    use consensus_config::{Committee as ConsensusCommittee, local_committee_and_keys};
    use prometheus::Registry;
    use sui_types::{committee::Committee, crypto::AuthorityPublicKeyBytes};

    use crate::{
        authority::AuthorityMetrics, scoring_decision::update_low_scoring_authorities_metrics,
    };

    #[test]
    #[cfg_attr(msim, ignore)]
    pub fn test_update_low_scoring_authorities_metrics() {
        // GIVEN
        // Total stake is 8 for this committee and every authority has equal stake = 1
        let (sui_committee, consensus_committee) = generate_committees(8);

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

        let low_scoring = update_low_scoring_authorities_metrics(
            &sui_committee,
            &consensus_committee,
            Some(authorities_by_score_desc.clone()),
            &metrics,
            consensus_bad_nodes_stake_threshold,
        );

        // THEN
        assert_eq!(low_scoring.len(), 2);
        assert_eq!(
            *low_scoring
                // authority 2 is 2nd to the last in authorities_by_score_desc
                .get(sui_committee.authority_by_index(2).unwrap())
                .unwrap(),
            50
        );
        assert_eq!(
            *low_scoring
                // authority 4 is the last in authorities_by_score_desc
                .get(sui_committee.authority_by_index(4).unwrap())
                .unwrap(),
            0
        );

        // WHEN setting the threshold to lower
        let consensus_bad_nodes_stake_threshold = 20; // 20 * 8 / 100 = 1 low scoring validator
        let low_scoring = update_low_scoring_authorities_metrics(
            &sui_committee,
            &consensus_committee,
            Some(authorities_by_score_desc.clone()),
            &metrics,
            consensus_bad_nodes_stake_threshold,
        );

        // THEN
        assert_eq!(low_scoring.len(), 1);
        assert_eq!(
            *low_scoring
                .get(sui_committee.authority_by_index(4).unwrap())
                .unwrap(),
            0
        );
    }

    /// Generate a pair of Sui and consensus committees for the given size.
    fn generate_committees(committee_size: usize) -> (Committee, ConsensusCommittee) {
        let (consensus_committee, _) = local_committee_and_keys(0, vec![1; committee_size]);

        let sui_authorities = consensus_committee
            .authorities()
            .map(|(_i, authority)| {
                let bytes: [u8; 96] = authority
                    .authority_name
                    .to_bytes()
                    .try_into()
                    .expect("Authority name should be 96 bytes");
                (AuthorityPublicKeyBytes::new(bytes), 1)
            })
            .collect::<Vec<_>>();
        let sui_committee = Committee::new_for_testing_with_normalized_voting_power(
            0,
            sui_authorities.iter().cloned().collect(),
        );

        (sui_committee, consensus_committee)
    }
}
