// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::consensus_types::AuthorityIndex;

pub(crate) trait ConsensusOutputAPI {
    fn reputation_score_sorted_desc(&self) -> Option<Vec<(AuthorityIndex, u64)>>;
}

impl ConsensusOutputAPI for narwhal_types::ConsensusOutput {
    fn reputation_score_sorted_desc(&self) -> Option<Vec<(AuthorityIndex, u64)>> {
        if !self.sub_dag.reputation_score.final_of_schedule {
            return None;
        }
        Some(
            self.sub_dag
                .reputation_score
                .authorities_by_score_desc()
                .into_iter()
                .map(|(id, score)| (id.0, score))
                .collect(),
        )
    }
}
