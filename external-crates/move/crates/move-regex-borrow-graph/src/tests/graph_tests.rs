// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{collections::*, regex::Regex};

// -------------------------------------------------------------------------------------------------
// Basic Graph Tests
// -------------------------------------------------------------------------------------------------

#[test]
fn new_graph_has_self_epsilon_edges() {
    let (g, refs) = Graph::<u8, char>::new([(0, 0u8, false)]).unwrap();
    for r in refs.values() {
        let edge = g.graph.edge_weight(*r, *r).unwrap();
        let regexes: Vec<_> = edge.regexes().collect();
        assert_eq!(regexes.len(), 1);
        assert!(regexes[0].is_epsilon());
    }
}

#[test]
fn release_removes_node_and_edges() {
    let (mut g, refs) = Graph::<u8, char>::new([(0, 0u8, false), (1, 1u8, false)]).unwrap();
    let r0 = refs[&0];
    let r1 = refs[&1];
    g.add_edge(r0, 0, Regex::label('a'), r1).unwrap();
    g.release(r0).unwrap();
    assert!(!g.graph.contains_node(r0));
    assert!(!g.graph.contains_edge(r0, r1));
}

#[test]
fn extend_by_epsilon_adds_only_self_edge() {
    let (mut g, refs) = Graph::<u8, char>::new([(0, 0u8, false)]).unwrap();
    let r = refs[&0];
    let new_r = g.extend_by_epsilon(1, [r], false).unwrap();
    g.check_invariants();
    let edge = g.graph.edge_weight(new_r, new_r).unwrap();
    let regexes: Vec<_> = edge.regexes().collect();
    assert_eq!(regexes.len(), 1);
    assert!(regexes[0].is_epsilon());
}

#[test]
fn extend_by_label_adds_label_path() {
    let (mut g, refs) = Graph::<u8, char>::new([(0, 0u8, false)]).unwrap();
    let r = refs[&0];
    let new_r = g.extend_by_label(1, [r], false, 'x').unwrap();
    g.check_invariants();

    let successors: Vec<_> = g.successors(r).unwrap().collect();
    assert!(successors.iter().any(|(_, s)| *s == new_r));
}
