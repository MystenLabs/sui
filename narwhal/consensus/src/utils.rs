// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::consensus::{ConsensusState, Dag};
use config::Committee;
use dag::bft::Bft;
use tracing::debug;
use types::{Certificate, CertificateDigest, Round};

/// Order the past leaders that we didn't already commit.
pub fn order_leaders<'a, LeaderElector>(
    committee: &Committee,
    leader: &Certificate,
    state: &'a ConsensusState,
    get_leader: LeaderElector,
) -> Vec<Certificate>
where
    LeaderElector: Fn(&Committee, Round, &'a Dag) -> Option<&'a (CertificateDigest, Certificate)>,
{
    let mut to_commit = vec![leader.clone()];
    let mut leader = leader;
    assert_eq!(leader.round() % 2, 0);
    for r in (state.last_committed_round + 2..=leader.round() - 2)
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
fn linked(leader: &Certificate, prev_leader: &Certificate, dag: &Dag) -> bool {
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

/// Flatten the dag referenced by the input certificate. This is a classic breadth-first search:
/// https://en.wikipedia.org/wiki/Breadth-first_search
pub fn order_dag<'a>(
    gc_depth: Round,
    leader: &'a Certificate,
    state: &'a ConsensusState,
) -> Vec<Certificate> {
    debug!("Processing sub-dag of {:?}", leader);

    // This gives us the committable parents for a given certificate. The committable parents are the parents
    // that have not yet been included in a prior commit.
    let parents = |input_certificate: &&'a Certificate| {
        match input_certificate
            .round()
            .checked_sub(1)
            .and_then(|prior_round| state.dag.get(&prior_round))
        {
            Some(map) => map
                .values()
                .filter_map(|(digest, certificate)| {
                    let in_parent_set = input_certificate.header.parents.contains(digest);
                    // We skip the certificate if we reached a round that we already
                    // committed for this authority.
                    let committed_before = state
                        .last_committed
                        .get(&certificate.origin())
                        .map_or(false, |r| r == &certificate.round());
                    (in_parent_set && !committed_before).then_some(certificate)
                })
                .collect::<Vec<_>>(),
            // This is the genesis, or we have GC'ed up the input_certificate's round from
            // the dag
            None => vec![],
        }
        .into_iter()
    };
    let mut ordered: Vec<Certificate> = Bft::new(leader, parents).cloned().collect();

    // Ensure we do not commit garbage collected certificates.
    ordered.retain(|x| x.round() + gc_depth >= state.last_committed_round);

    // Since this is a BFT, no need to sort commits: they are in round order. This particular BFT is in
    // root-to-leaves order, though, and we'd like the reverse (older to newer blocks).
    ordered.reverse();

    ordered
}
