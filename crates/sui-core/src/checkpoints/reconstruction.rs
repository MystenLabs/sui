// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cmp::max;
use std::collections::{BTreeMap, HashMap, VecDeque};
use tracing::debug;

use sui_types::base_types::ExecutionDigests;
use sui_types::committee::StakeUnit;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages_checkpoint::{CheckpointProposalSummary, CheckpointSequenceNumber};
use sui_types::{
    base_types::AuthorityName,
    committee::Committee,
    messages::CertifiedTransaction,
    messages_checkpoint::CheckpointFragment,
    waypoint::{GlobalCheckpoint, WaypointError},
};

// TODO: This probably deserve a better name.
pub struct FragmentReconstruction {
    pub global: GlobalCheckpoint<AuthorityName, ExecutionDigests>,
    pub extra_transactions: BTreeMap<ExecutionDigests, CertifiedTransaction>,
}

// A structure that stores a set of spanning trees, and that supports addition
// of links to merge them, and construct ever growing components.
#[derive(Clone, Debug)]
pub enum SpanGraph {
    // A newly created span graph is always in Uninitialized state, and will be turned into
    // InProgress upon receiving the first fragment.
    Uninitialized,
    InProgress(InProgressSpanGraph),
    Completed(CompletedSpanGraph),
}

impl Default for SpanGraph {
    fn default() -> Self {
        Self::Uninitialized
    }
}

#[derive(Clone, Debug)]
pub struct InProgressSpanGraph {
    /// Each validator is a node in the span graph. This maps from each validator
    /// to the root of the tree it belongs to and the total stake of the tree.
    nodes: HashMap<AuthorityName, (AuthorityName, StakeUnit)>,

    /// The sequence number of the checkpoint being constructed.
    next_checkpoint: CheckpointSequenceNumber,

    /// Fragments that have been used so far.
    fragments_used: Vec<CheckpointFragment>,

    /// Proposals from each validator seen so far. This is needed to detect potential conflicting
    /// fragments.
    proposals_used: HashMap<AuthorityName, CheckpointProposalSummary>,

    /// The max stake we have seen so far in a span tree.
    max_weight_seen: StakeUnit,
}

impl InProgressSpanGraph {
    /// Add a link effectively merging two authorities into the same
    /// connected components. This is done by take the top node of the
    /// first and making it point to the top node of the second, and
    /// updating the total weight of the second.
    pub fn merge(
        &mut self,
        name1: &AuthorityName,
        name2: &AuthorityName,
    ) -> (AuthorityName, StakeUnit) {
        let top1 = self.top_node(name1).0;
        let top2 = self.top_node(name2).0;
        if top1 == top2 {
            // they are already merged
            return (top1, self.nodes[&top1].1);
        }

        // They are not merged, so merge now
        let new_weight = self.nodes[&top1].1 + self.nodes[&top2].1;
        self.nodes.get_mut(&top1).unwrap().0 = top2;
        self.nodes.get_mut(&top2).unwrap().1 = new_weight;
        debug_assert!(self.top_node(name1) == self.top_node(name2));
        (top2, new_weight)
    }

    /// Follow pointer until you get to a node that only point to itself
    /// and return the node name, and the weight of the tree that points
    /// indirectly to it.
    pub fn top_node(&self, name: &AuthorityName) -> (AuthorityName, StakeUnit) {
        let mut next_name = name;
        while self.nodes[next_name].0 != *next_name {
            next_name = &self.nodes[next_name].0
        }
        self.nodes[next_name]
    }
}

#[derive(Clone, Debug)]
pub struct CompletedSpanGraph {
    active_links: VecDeque<CheckpointFragment>,
}

impl SpanGraph {
    pub fn mew(
        committee: &Committee,
        next_checkpoint: CheckpointSequenceNumber,
        fragments: &[CheckpointFragment],
    ) -> Self {
        let mut span = Self::default();
        for frag in fragments {
            span.add_fragment_to_span(committee, next_checkpoint, frag);
            if span.is_completed() {
                break;
            }
        }
        span
    }

    /// Add a new fragment to the span graph and checks whether it can construct a connected
    /// component checkpoint with weight over 2/3 of stake. Note that the minimum
    /// prefix of links is used in this process.
    ///
    /// It is important to always use the minimum prefix since additional fragments
    /// may be added by the consensus, but only the prefix that constructs the
    /// checkpoint is safe to use. After that prefix different authorities will have
    /// different information to finalize the process:
    ///  - f+1 to 2f+1 honest authorities will be included in the prefix and can
    ///    immediately compute and sign the checkpoint.
    ///  - the remaining honest authorities will instead have to use other strategies
    ///    such as downloading the checkpoint, or using other links (off the consensus)
    ///    to compute it.
    pub fn add_fragment_to_span(
        &mut self,
        committee: &Committee,
        next_checkpoint: CheckpointSequenceNumber,
        frag: &CheckpointFragment,
    ) {
        if matches!(&self, Self::Uninitialized) {
            self.initialize(committee, next_checkpoint);
        }
        if let Self::InProgress(span) = self {
            debug!(
                next_cp_seq=span.next_checkpoint,
                frag_seq=frag.proposer.summary.sequence_number,
                proposer=?frag.proposer.authority(),
                other=?frag.other.authority(),
                "Trying to add a new checkpoint fragment to the span graph.",
            );

            // Ignore if the fragment is for a different checkpoint.
            if frag.proposer.summary.sequence_number != span.next_checkpoint {
                return;
            }

            // Check the checkpoint summary of the proposal is the same as the previous one.
            // Otherwise ignore the link.
            let n1 = frag.proposer.authority();
            if *span
                .proposals_used
                .entry(*n1)
                .or_insert_with(|| frag.proposer.summary.clone())
                != frag.proposer.summary
            {
                return;
            }

            let n2 = frag.other.authority();
            if *span
                .proposals_used
                .entry(*n2)
                .or_insert_with(|| frag.other.summary.clone())
                != frag.other.summary
            {
                return;
            }

            // Add to the links we will consider.
            span.fragments_used.push(frag.clone());

            // Merge the link.
            let (top_node, weight) = span.merge(n1, n2);
            span.max_weight_seen = max(span.max_weight_seen, weight);
            debug!(
                next_cp_seq=span.next_checkpoint,
                max_weight_seen=?span.max_weight_seen,
                "Checkpoint fragment added",
            );
            if weight >= committee.quorum_threshold() {
                // Get all links that are part of this component
                let active_links: VecDeque<_> = std::mem::take(&mut span.fragments_used)
                    .into_iter()
                    .filter(|frag| span.top_node(frag.proposer.authority()).0 == top_node)
                    .collect();

                debug!(
                    next_cp_seq = span.next_checkpoint,
                    "Checkpoint construction completed"
                );
                *self = Self::Completed(CompletedSpanGraph { active_links });
            }
        }
    }

    pub fn is_completed(&self) -> bool {
        matches!(self, Self::Completed(_))
    }

    pub fn construct_checkpoint(&self) -> SuiResult<FragmentReconstruction> {
        if let Self::Completed(span) = &self {
            let mut global = GlobalCheckpoint::new();
            let mut extra_transactions = BTreeMap::new();
            let mut active_links = span.active_links.clone();
            while let Some(link) = active_links.pop_front() {
                match global.insert(link.diff.clone()) {
                    Ok(_) | Err(WaypointError::NothingToDo) => {
                        extra_transactions.extend(link.certs.clone());
                    }
                    Err(WaypointError::CannotConnect) => {
                        // Reinsert the fragment at the end
                        active_links.push_back(link);
                    }
                    other => {
                        // This is bad news, we did not intend to fail here.
                        // We should have checked all conditions to avoid being
                        // in this situation. TODO: audit this.
                        panic!("Unexpected result: {:?}", other);
                        // Or: unreachable!();
                    }
                }
            }

            Ok(FragmentReconstruction {
                global,
                extra_transactions,
            })
        } else {
            Err(SuiError::from(
                "Not yet enough fragments to construct checkpoint",
            ))
        }
    }

    fn initialize(&mut self, committee: &Committee, next_checkpoint: CheckpointSequenceNumber) {
        let nodes: HashMap<AuthorityName, (AuthorityName, StakeUnit)> =
            committee.members().map(|(n, w)| (*n, (*n, *w))).collect();

        *self = Self::InProgress(InProgressSpanGraph {
            nodes,
            next_checkpoint,
            fragments_used: Vec::new(),
            proposals_used: HashMap::new(),
            max_weight_seen: 0,
        })
    }
}
