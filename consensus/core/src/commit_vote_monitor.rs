// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use consensus_config::AuthorityIndex;
use mysten_common::ZipDebugEqIteratorExt;
use parking_lot::Mutex;

use crate::{
    CommitIndex,
    block::{BlockAPI as _, VerifiedBlock},
    commit::{CommitVote, GENESIS_COMMIT_INDEX},
    context::Context,
};

// Is used to calculate the threshold for blocking blocks or triggering sync
// when the local commit index is lagging too far from the quorum commit index.
pub(crate) const COMMIT_LAG_MULTIPLIER: u32 = 5;

/// When a node is lagging behind peers in commits, we expect commit sync to be
/// triggered to catch up.
pub(crate) fn is_commit_lagging(
    context: &Context,
    local_commit_index: CommitIndex,
    quorum_commit_index: CommitIndex,
) -> bool {
    local_commit_index + context.parameters.commit_sync_batch_size * COMMIT_LAG_MULTIPLIER
        < quorum_commit_index
}

/// Monitors the progress of consensus commits across the network.
pub(crate) struct CommitVoteMonitor {
    context: Arc<Context>,
    // Highest commit index voted by each authority.
    highest_voted_commits: Mutex<Vec<CommitIndex>>,
}

impl CommitVoteMonitor {
    pub(crate) fn new(context: Arc<Context>) -> Self {
        let highest_voted_commits = Mutex::new(vec![0; context.committee.size()]);
        Self {
            context,
            highest_voted_commits,
        }
    }

    /// Keeps track of the highest commit voted by each authority.
    pub(crate) fn observe_block(&self, block: &VerifiedBlock) {
        let mut highest_voted_commits = self.highest_voted_commits.lock();
        for vote in block.commit_votes() {
            if vote.index > highest_voted_commits[block.author()] {
                highest_voted_commits[block.author()] = vote.index;
            }
        }
    }

    /// Credits commit votes claimed by a block streamed on `author`'s own
    /// authenticated subscription, before verification. Only `author`'s column moves —
    /// the same surface as `observe_block`, whose votes are equally the author's own
    /// claims (verification proves authorship; stream authentication plus the
    /// expected-author check in minimal decoding prove the same). Restores the
    /// full-form invariant that every streamed block feeds commit-sync targeting,
    /// which must hold especially when the block cannot be reconstructed yet.
    pub(crate) fn observe_stream_claim(&self, author: AuthorityIndex, votes: &[CommitVote]) {
        let mut highest_voted_commits = self.highest_voted_commits.lock();
        for vote in votes {
            if vote.index > highest_voted_commits[author] {
                highest_voted_commits[author] = vote.index;
            }
        }
    }

    // Finds the highest commit index certified by a quorum.
    // When an authority votes for commit index S, it is also voting for all commit indices 1 <= i < S.
    // So the quorum commit index is the smallest index S such that the sum of stakes of authorities
    // voting for commit indices >= S passes the quorum threshold.
    pub(crate) fn quorum_commit_index(&self) -> CommitIndex {
        let highest_voted_commits = self.highest_voted_commits.lock();
        let mut highest_voted_commits = highest_voted_commits
            .iter()
            .zip_debug_eq(self.context.committee.authorities())
            .map(|(commit_index, (_, a))| (*commit_index, a.stake))
            .collect::<Vec<_>>();
        // Sort by commit index then stake, in descending order.
        highest_voted_commits.sort_by(|a, b| a.cmp(b).reverse());
        let mut total_stake = 0;
        for (commit_index, stake) in highest_voted_commits {
            total_stake += stake;
            if total_stake >= self.context.committee.quorum_threshold() {
                return commit_index;
            }
        }
        GENESIS_COMMIT_INDEX
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use super::CommitVoteMonitor;
    use crate::{
        block::{TestBlock, VerifiedBlock},
        commit::{CommitDigest, CommitRef, CommitVote},
        context::Context,
    };

    #[tokio::test]
    async fn test_commit_vote_monitor() {
        let context = Arc::new(Context::new_for_test(4).0);
        let monitor = CommitVoteMonitor::new(context.clone());

        // Observe commit votes for indices 5, 6, 7, 8 from blocks.
        let blocks = (0..4)
            .map(|i| {
                VerifiedBlock::new_for_test(
                    TestBlock::new(10, i)
                        .set_commit_votes(vec![CommitRef::new(5 + i, CommitDigest::MIN)])
                        .build(),
                )
            })
            .collect::<Vec<_>>();
        for b in blocks {
            monitor.observe_block(&b);
        }

        // CommitIndex 6 is the highest index supported by a quorum.
        assert_eq!(monitor.quorum_commit_index(), 6);

        // Observe new blocks with new votes from authority 0 and 1.
        let blocks = (0..2)
            .map(|i| {
                VerifiedBlock::new_for_test(
                    TestBlock::new(11, i)
                        .set_commit_votes(vec![
                            CommitRef::new(6 + i, CommitDigest::MIN),
                            CommitRef::new(7 + i, CommitDigest::MIN),
                        ])
                        .build(),
                )
            })
            .collect::<Vec<_>>();
        for b in blocks {
            monitor.observe_block(&b);
        }

        // Highest commit index per authority should be 7, 8, 7, 8 now.
        assert_eq!(monitor.quorum_commit_index(), 7);
    }

    /// A stream claim moves only the claiming author's column: one peer cannot raise
    /// the quorum index without quorum stake behind it, and claims never regress.
    #[tokio::test]
    async fn stream_claim_bounded_to_own_column() {
        let context = Arc::new(Context::new_for_test(4).0);
        let monitor = CommitVoteMonitor::new(context.clone());
        let byzantine = context.committee.to_authority_index(3).unwrap();
        monitor.observe_stream_claim(byzantine, &[CommitVote::new(1_000_000, CommitDigest::MIN)]);
        assert_eq!(
            monitor.quorum_commit_index(),
            0,
            "one column must not move the quorum index"
        );
        // Two more columns at a modest index: quorum forms at the SMALLER value the
        // 2f+1 threshold actually supports, not at the inflated claim.
        for authority in [0u32, 1] {
            let block = VerifiedBlock::new_for_test(
                crate::block::TestBlock::new(1, authority)
                    .set_commit_votes(vec![CommitVote::new(7, CommitDigest::MIN)])
                    .build(),
            );
            monitor.observe_block(&block);
        }
        assert_eq!(monitor.quorum_commit_index(), 7);
        // Claims are monotone per column: a lower later claim does not regress it.
        monitor.observe_stream_claim(byzantine, &[CommitVote::new(3, CommitDigest::MIN)]);
        assert_eq!(monitor.quorum_commit_index(), 7);
    }
}
