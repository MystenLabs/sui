// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::consensus::{ConsensusState, Dag};
use config::Committee;
use crypto::traits::VerifyingKey;
use std::collections::HashSet;
use tracing::debug;
use types::{Certificate, CertificateDigest, Round};

/// Order the past leaders that we didn't already commit.
pub fn order_leaders<'a, PublicKey: VerifyingKey, LeaderElector>(
    committee: &Committee<PublicKey>,
    leader: &Certificate<PublicKey>,
    state: &'a ConsensusState<PublicKey>,
    get_leader: LeaderElector,
) -> Vec<Certificate<PublicKey>>
where
    LeaderElector: Fn(
        &Committee<PublicKey>,
        Round,
        &'a Dag<PublicKey>,
    ) -> Option<&'a (CertificateDigest, Certificate<PublicKey>)>,
{
    let mut to_commit = vec![leader.clone()];
    let mut leader = leader;
    for r in (state.last_committed_round + 2..leader.round())
        .rev()
        .step_by(2)
    {
        // Get the certificate proposed by the previous leader.
        let (_, prev_leader) = match get_leader(committee, r, &state.dag) {
            Some(x) => x,
            None => continue,
        };

        // Check whether there is a path between the last two leaders.
        if linked(leader, prev_leader, &state.dag) {
            to_commit.push(prev_leader.clone());
            leader = prev_leader;
        }
    }
    to_commit
}

/// Checks if there is a path between two leaders.
fn linked<PublicKey: VerifyingKey>(
    leader: &Certificate<PublicKey>,
    prev_leader: &Certificate<PublicKey>,
    dag: &Dag<PublicKey>,
) -> bool {
    let mut parents = vec![leader];
    for r in (prev_leader.round()..leader.round()).rev() {
        parents = dag
            .get(&(r))
            .expect("We should have the whole history by now")
            .values()
            .filter(|(digest, _)| parents.iter().any(|x| x.header.parents.contains(digest)))
            .map(|(_, certificate)| certificate)
            .collect();
    }
    parents.contains(&prev_leader)
}

/// Flatten the dag referenced by the input certificate. This is a classic depth-first search (pre-order):
/// https://en.wikipedia.org/wiki/Tree_traversal#Pre-order
pub fn order_dag<PublicKey: VerifyingKey>(
    gc_depth: Round,
    leader: &Certificate<PublicKey>,
    state: &ConsensusState<PublicKey>,
) -> Vec<Certificate<PublicKey>> {
    debug!("Processing sub-dag of {:?}", leader);
    let mut ordered = Vec::new();
    let mut already_ordered = HashSet::new();

    let mut buffer = vec![leader];
    while let Some(x) = buffer.pop() {
        debug!("Sequencing {:?}", x);
        ordered.push(x.clone());
        for parent in &x.header.parents {
            let (digest, certificate) = match state
                .dag
                .get(&(x.round() - 1))
                .and_then(|x| x.values().find(|(x, _)| x == parent))
            {
                Some(x) => x,
                None => continue, // We already ordered or GC up to here.
            };

            // We skip the certificate if we (1) already processed it or (2) we reached a round that we already
            // committed for this authority.
            let mut skip = already_ordered.contains(&digest);
            skip |= state
                .last_committed
                .get(&certificate.origin())
                .map_or_else(|| false, |r| r == &certificate.round());
            if !skip {
                buffer.push(certificate);
                already_ordered.insert(digest);
            }
        }
    }

    // Ensure we do not commit garbage collected certificates.
    ordered.retain(|x| x.round() + gc_depth >= state.last_committed_round);

    // Ordering the output by round is not really necessary but it makes the commit sequence prettier.
    ordered.sort_by_key(|x| x.round());
    ordered
}
