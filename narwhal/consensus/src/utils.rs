// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::consensus::ConsensusState;
use std::collections::HashSet;
use tracing::debug;
use types::{Certificate, CertificateAPI, HeaderAPI, Round};

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
        for (round, ancestor) in x.header().ancestors() {
            if round <= gc_round {
                continue;
            }
            let (digest, certificate) = match state
                .dag
                .get(&round)
                .and_then(|map| map.values().find(|(x, _)| x == &ancestor))
            {
                Some(x) => x,
                None => panic!("Ancestor {ancestor:?} at round  {round} not found for {x:?}!"),
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
