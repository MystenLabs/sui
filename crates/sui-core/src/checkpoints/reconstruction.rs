// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap, VecDeque};

use sui_types::base_types::ExecutionDigests;
use sui_types::committee::StakeUnit;
use sui_types::{
    base_types::AuthorityName,
    committee::Committee,
    error::SuiError,
    messages::CertifiedTransaction,
    messages_checkpoint::{CheckpointFragment, CheckpointSummary},
    waypoint::{GlobalCheckpoint, WaypointError},
};

pub struct FragmentReconstruction {
    pub committee: Committee,
    pub global: GlobalCheckpoint<AuthorityName, ExecutionDigests>,
    pub extra_transactions: BTreeMap<ExecutionDigests, CertifiedTransaction>,
}

impl FragmentReconstruction {
    /// Take an ordered list of fragments and attempts to construct a connected
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
    pub fn construct(
        seq: u64,
        committee: Committee,
        fragments: &[CheckpointFragment],
    ) -> Result<Option<FragmentReconstruction>, SuiError> {
        let mut span = SpanGraph::new(&committee);
        let mut fragments_used = Vec::new();
        let mut proposals: HashMap<AuthorityName, CheckpointSummary> = HashMap::new();
        let mut extra_transactions = BTreeMap::new();

        for frag in fragments {
            // Double check we have only been given waypoints for the correct sequence number
            debug_assert!(*frag.proposer.summary.sequence_number() == seq);

            // Check the checkpoint summary of the proposal is the same as the previous one.
            // Otherwise ignore the link.
            let n1 = frag.proposer.authority();
            if *proposals
                .entry(*n1)
                .or_insert_with(|| frag.proposer.summary.clone())
                != frag.proposer.summary
            {
                continue;
            }

            let n2 = frag.other.authority();
            if *proposals
                .entry(*n2)
                .or_insert_with(|| frag.other.summary.clone())
                != frag.other.summary
            {
                continue;
            }

            // Add to the links we will consider.
            fragments_used.push(frag);

            // Merge the link.
            let (top, weight) = span.merge(n1, n2);

            // We have found a connected component larger than the 2/3 threshold
            if weight >= committee.quorum_threshold() {
                // Get all links that are part of this component
                let mut active_links: VecDeque<_> = fragments_used
                    .into_iter()
                    .filter(|frag| span.top_node(frag.proposer.authority()).0 == top)
                    .collect();

                let mut global = GlobalCheckpoint::new();
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

                return Ok(Some(FragmentReconstruction {
                    global,
                    committee,
                    extra_transactions,
                }));
            }
        }

        // If we run out of candidates with no checkpoint, there is no
        // checkpoint yet.
        Ok(None)
    }
}

// A structure that stores a set of spanning trees, and that supports addition
// of links to merge them, and construct ever growing components.
struct SpanGraph {
    nodes: HashMap<AuthorityName, (AuthorityName, StakeUnit)>,
}

impl SpanGraph {
    /// Initialize the graph with each authority just pointing to itself.
    pub fn new(committee: &Committee) -> SpanGraph {
        let nodes: HashMap<AuthorityName, (AuthorityName, StakeUnit)> =
            committee.members().map(|(n, w)| (*n, (*n, *w))).collect();

        SpanGraph { nodes }
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
}
