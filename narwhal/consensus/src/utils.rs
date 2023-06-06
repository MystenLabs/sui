// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::consensus::{ConsensusState, Dag};
use crate::metrics::ConsensusMetrics;
use config::{Authority, Committee};
use std::collections::HashSet;
use std::sync::Arc;
use tracing::debug;
use types::{Certificate, CertificateAPI, HeaderAPI, Round};

/// Order the past leaders that we didn't already commit.
pub fn order_leaders<'a, LeaderElector>(
    committee: &Committee,
    leader: &Certificate,
    state: &'a ConsensusState,
    get_leader: LeaderElector,
    metrics: Arc<ConsensusMetrics>,
) -> Vec<Certificate>
where
    LeaderElector: Fn(&Committee, Round, &'a Dag) -> (Authority, Option<&'a Certificate>),
{
    let mut to_commit = vec![leader.clone()];
    let mut leader = leader;
    assert_eq!(leader.round() % 2, 0);
    for r in (state.last_round.committed_round + 2..=leader.round() - 2)
        .rev()
        .step_by(2)
    {
        // Get the certificate proposed by the previous leader.
        let (prev_leader, authority) = match get_leader(committee, r, &state.dag) {
            (authority, Some(x)) => (x, authority),
            (authority, None) => {
                metrics
                    .leader_election
                    .with_label_values(&["not_found", authority.hostname()])
                    .inc();

                continue;
            }
        };

        // Check whether there is a path between the last two leaders.
        if linked(leader, prev_leader, &state.dag) {
            to_commit.push(prev_leader.clone());
            leader = prev_leader;
        } else {
            metrics
                .leader_election
                .with_label_values(&["no_path", authority.hostname()])
                .inc();
        }
    }

    // Now just report all the found leaders
    to_commit.iter().for_each(|certificate| {
        let authority = committee.authority(&certificate.origin()).unwrap();

        metrics
            .leader_election
            .with_label_values(&["committed", authority.hostname()])
            .inc();
    });

    to_commit
}

/// Checks if there is a path between two leaders.
fn linked(leader: &Certificate, prev_leader: &Certificate, dag: &Dag) -> bool {
    let mut parents = vec![leader];
    for r in (prev_leader.round()..leader.round()).rev() {
        parents = dag
            .get(&r)
            .expect("We should have the whole history by now")
            .values()
            .filter(|(digest, _)| {
                parents
                    .iter()
                    .any(|x| x.header().parents().contains(digest))
            })
            .map(|(_, certificate)| certificate)
            .collect();
    }
    parents.contains(&prev_leader)
}

/// Flatten the dag referenced by the input certificate. This is a classic depth-first search (pre-order):
/// <https://en.wikipedia.org/wiki/Tree_traversal#Pre-order>
pub fn order_dag(leader: &Certificate, state: &ConsensusState) -> Vec<Certificate> {
    debug!("Processing sub-dag of {:?}", leader);
    assert!(leader.round() > 0);
    let gc_round = leader.round().saturating_sub(state.gc_depth);

    let mut ordered = Vec::new();
    let mut already_ordered = HashSet::new();

    let mut buffer = vec![leader];
    while let Some(x) = buffer.pop() {
        debug!("Sequencing {:?}", x);
        ordered.push(x.clone());
        if x.round() == gc_round + 1 {
            // Do not try to order parents of the certificate, since they have been GC'ed.
            continue;
        }
        for parent in x.header().parents() {
            let (digest, certificate) = match state
                .dag
                .get(&(x.round() - 1))
                .and_then(|x| x.values().find(|(x, _)| x == parent))
            {
                Some(x) => x,
                None => panic!("Parent digest {parent:?} not found for {x:?}!"),
            };

            // We skip the certificate if we (1) already processed it or (2) we reached a round that we already
            // committed or will never commit for this authority.
            let mut skip = already_ordered.contains(&digest);
            skip |= state
                .last_committed
                .get(&certificate.origin())
                .map_or_else(|| false, |r| &certificate.round() <= r);
            if !skip {
                buffer.push(certificate);
                already_ordered.insert(digest);
            }
        }
    }

    // Ordering the output by round is not really necessary but it makes the commit sequence prettier.
    ordered.sort_by_key(|x| x.round());
    ordered
}

/// Calculates the GC round given a commit round and the gc_depth
pub fn gc_round(commit_round: Round, gc_depth: Round) -> Round {
    commit_round.saturating_sub(gc_depth)
}
