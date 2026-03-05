// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::collections::*;

// -------------------------------------------------------------------------------------------------
// Basic Graph Tests
// -------------------------------------------------------------------------------------------------

#[test]
fn new_graph_has_self_epsilon_edges() {
    let (g, refs) = Graph::<u8, char>::new(1, [(0, 0u8, false)]).unwrap();
    for r in refs.values() {
        let r_idx = g.node(r).unwrap().node_index();
        let edge = g.graph.edge_weight(r_idx, r_idx).unwrap();
        let regexes: Vec<_> = edge.regexes().collect();
        assert_eq!(regexes.len(), 1);
        assert!(regexes[0].is_epsilon());
    }
}

#[test]
fn release_removes_node_and_edges() {
    let meter = &mut crate::meter::DummyMeter;
    let (mut g, refs) = Graph::<u8, char>::new(3, [(0, 0u8, false), (1, 1u8, false)]).unwrap();
    let r0 = refs[&0];
    let r1 = refs[&1];
    let r2 = g
        .extend_by_label(2, std::iter::once(r0), false, 'a', meter)
        .unwrap();
    let r0_node = g.node(&r0).unwrap();
    let r0_idx = r0_node.node_index();
    g.release(r0, meter).unwrap();
    assert!(!g.graph.contains_node(r0_idx));
    let r1_node = g.node(&r1).unwrap();
    assert!(g.graph.contains_node(r1_node.node_index()));
    let r2_node = g.node(&r2).unwrap();
    assert!(g.graph.contains_node(r2_node.node_index()));
    assert!(!g.graph.contains_edge(r0_idx, r2_node.node_index()));
}

#[test]
fn extend_by_epsilon_adds_only_self_edge() {
    let meter = &mut crate::meter::DummyMeter;
    let (mut g, refs) = Graph::<u8, char>::new(11, [(0, 0u8, false)]).unwrap();
    let r = refs[&0];
    let new_r = g.extend_by_epsilon(1, [r], false, meter).unwrap();
    g.check_invariants();
    let new_r_idx = g.node(&new_r).unwrap().node_index();
    let edge = g.graph.edge_weight(new_r_idx, new_r_idx).unwrap();
    let regexes: Vec<_> = edge.regexes().collect();
    assert_eq!(regexes.len(), 1);
    assert!(regexes[0].is_epsilon());
}

#[test]
fn extend_by_label_adds_label_path() {
    let meter = &mut crate::meter::DummyMeter;
    let (mut g, refs) = Graph::<u8, char>::new(2, [(0, 0u8, false)]).unwrap();
    let r = refs[&0];
    let new_r = g.extend_by_label(1, [r], false, 'x', meter).unwrap();
    g.check_invariants();

    let successors: Vec<_> = g.successors(r).unwrap().collect();
    assert!(successors.iter().any(|(_, s)| *s == new_r));
}
