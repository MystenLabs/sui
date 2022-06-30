// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use sui_types::messages_checkpoint::SignedCheckpointSummary;
use sui_types::{
    base_types::{AuthorityName, ExecutionDigests},
    messages_checkpoint::{
        CheckpointContents, CheckpointFragment, CheckpointSequenceNumber, CheckpointSummary,
    },
    waypoint::WaypointDiff,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointProposal {
    /// Name of the authority
    pub signed_summary: SignedCheckpointSummary,
    /// The transactions included in the proposal.
    /// TODO: only include a commitment by default.
    pub transactions: CheckpointContents,
}

impl CheckpointProposal {
    /// Create a proposal for a checkpoint at a particular height
    /// This contains a sequence number, waypoint and a list of
    /// proposed transactions.
    /// TODO: Add an identifier for the proposer, probably
    ///       an AuthorityName.
    pub fn new(proposal: SignedCheckpointSummary, transactions: CheckpointContents) -> Self {
        CheckpointProposal {
            signed_summary: proposal,
            transactions,
        }
    }

    /// Returns the sequence number of this proposal
    pub fn sequence_number(&self) -> &CheckpointSequenceNumber {
        self.signed_summary.summary.sequence_number()
    }

    // Iterate over all transaction/effects
    pub fn transactions(&self) -> impl Iterator<Item = &ExecutionDigests> {
        self.transactions.iter()
    }

    // Get the inner checkpoint
    pub fn checkpoint(&self) -> &CheckpointSummary {
        &self.signed_summary.summary
    }

    // Get the authority name
    pub fn name(&self) -> &AuthorityName {
        self.signed_summary.authority()
    }

    /// Construct a Diff structure between this proposal and another
    /// proposal. A diff structure has to contain keys. The diff represents
    /// the elements that each proposal need to be augmented by to
    /// contain the same elements.
    ///
    /// TODO: down the line we can include other methods to get diffs
    /// line MerkleTrees or IBLT filters that do not require O(n) download
    /// of both proposals.
    pub fn fragment_with(&self, other_proposal: &CheckpointProposal) -> CheckpointFragment {
        let all_elements = self
            .transactions()
            .chain(other_proposal.transactions.iter())
            .collect::<HashSet<_>>();

        let my_transactions = self.transactions().collect();
        let iter_missing_me = all_elements.difference(&my_transactions).map(|x| **x);
        let other_transactions = other_proposal.transactions().collect();
        let iter_missing_other = all_elements.difference(&other_transactions).map(|x| **x);

        let diff = WaypointDiff::new(
            *self.name(),
            *self.checkpoint().waypoint.clone(),
            iter_missing_me,
            *other_proposal.name(),
            *other_proposal.checkpoint().waypoint.clone(),
            iter_missing_other,
        );

        CheckpointFragment {
            proposer: self.signed_summary.clone(),
            other: other_proposal.signed_summary.clone(),
            diff,
            certs: BTreeMap::new(),
        }
    }
}
