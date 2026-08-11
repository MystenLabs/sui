// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::collections::*;
use crate::{references::Ref, regex::Extension};
use std::collections::{BTreeMap, BTreeSet};

type TestGraph = Graph<u8, char>;

// -------------------------------------------------------------------------------------------------
// Test Helpers
// -------------------------------------------------------------------------------------------------

fn canonicalize(graph: &mut TestGraph, refs: &[Ref]) -> Vec<Ref> {
    let remapping = refs
        .iter()
        .enumerate()
        .map(|(idx, r)| (*r, idx as u32))
        .collect::<BTreeMap<_, _>>();
    let canonical_refs = refs
        .iter()
        .map(|r| r.canonicalize(&remapping).unwrap())
        .collect();
    graph.canonicalize(&remapping).unwrap();
    canonical_refs
}

fn path_set(graph: &TestGraph, source: Ref, target: Ref) -> BTreeSet<(Vec<char>, bool)> {
    graph
        .borrowed_by(source, &mut crate::meter::DummyMeter)
        .unwrap()
        .remove(&target)
        .unwrap_or_default()
        .into_iter()
        .map(|path| (path.labels, path.ends_in_dot_star))
        .collect()
}

fn is_writable(graph: &TestGraph, r: Ref) -> bool {
    graph.is_mutable(r).unwrap()
        && graph
            .borrowed_by(r, &mut crate::meter::DummyMeter)
            .unwrap()
            .values()
            .all(|paths| paths.iter().all(|path| path.is_epsilon()))
}

// Constructs two mutable references related by the supplied extension:
// root --extension--> target.
fn two_node_graph(extension: Extension<char>) -> (TestGraph, Ref, Ref) {
    let meter = &mut crate::meter::DummyMeter;
    let (mut graph, refs) = Graph::<u8, char>::new(2, [(0, 0, true)]).unwrap();
    let root = refs[&0];
    let target = match extension {
        Extension::Epsilon => graph.extend_by_epsilon(0, [root], true, meter).unwrap(),
        Extension::Label(label) => graph
            .extend_by_label(0, [root], true, label, meter)
            .unwrap(),
        Extension::DotStar => graph
            .extend_by_dot_star_for_call(0, &BTreeSet::from([root]), vec![true], meter)
            .unwrap()[0],
    };
    let refs = canonicalize(&mut graph, &[root, target]);
    (graph, refs[0], refs[1])
}

// Constructs a three-reference chain with two labeled extensions:
// root --first--> middle --second--> leaf.
fn three_node_graph(first: char, second: char) -> (TestGraph, Ref, Ref, Ref) {
    let meter = &mut crate::meter::DummyMeter;
    let (mut graph, refs) = Graph::<u8, char>::new(3, [(0, 0, true)]).unwrap();
    let root = refs[&0];
    let middle = graph
        .extend_by_label(0, [root], true, first, meter)
        .unwrap();
    let leaf = graph
        .extend_by_label(0, [middle], true, second, meter)
        .unwrap();
    let refs = canonicalize(&mut graph, &[root, middle, leaf]);
    (graph, refs[0], refs[1], refs[2])
}

// -------------------------------------------------------------------------------------------------
// Construction Tests
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

// -------------------------------------------------------------------------------------------------
// Extension Tests
// -------------------------------------------------------------------------------------------------

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

// -------------------------------------------------------------------------------------------------
// Canonicalization and Refresh Tests
// -------------------------------------------------------------------------------------------------

#[test]
fn canonicalize_refresh_roundtrip_preserves_paths() {
    let meter = &mut crate::meter::DummyMeter;
    let (mut graph, refs) = Graph::<u8, char>::new(3, [(0, 0, true)]).unwrap();
    let root = refs[&0];
    let alias = graph.extend_by_epsilon(0, [root], true, meter).unwrap();
    let field = graph.extend_by_label(0, [alias], true, 'f', meter).unwrap();
    let canonical_refs = canonicalize(&mut graph, &[root, alias, field]);
    let before = path_set(&graph, canonical_refs[0], canonical_refs[2]);

    graph.refresh_refs().unwrap();
    let fresh_refs = canonical_refs
        .iter()
        .map(|r| r.refresh().unwrap())
        .collect::<Vec<_>>();
    let canonical_refs = canonicalize(&mut graph, &fresh_refs);

    assert_eq!(
        before,
        path_set(&graph, canonical_refs[0], canonical_refs[2])
    );
    assert!(graph.is_canonical());
}

// -------------------------------------------------------------------------------------------------
// Join Tests
// -------------------------------------------------------------------------------------------------

#[test]
fn join_identical_graph_is_unchanged_and_idempotent() {
    let (mut graph, _, _) = two_node_graph(Extension::Label('f'));
    let other = graph.clone();

    assert!(!graph.join(&other, &mut crate::meter::DummyMeter).unwrap());
    assert!(!graph.join(&other, &mut crate::meter::DummyMeter).unwrap());
}

#[test]
fn join_of_epsilon_paths_remains_writable() {
    let (mut graph, root, target) = two_node_graph(Extension::Epsilon);
    let (other, other_root, other_target) = two_node_graph(Extension::Epsilon);
    assert_eq!((root, target), (other_root, other_target));

    assert!(!graph.join(&other, &mut crate::meter::DummyMeter).unwrap());
    assert_eq!(
        path_set(&graph, root, target),
        BTreeSet::from([(vec![], false)])
    );
    assert!(is_writable(&graph, root));
    assert!(is_writable(&graph, target));
}

#[test]
fn join_of_epsilon_and_label_is_order_independent_and_not_writable() {
    let (epsilon, root, target) = two_node_graph(Extension::Epsilon);
    let (label, label_root, label_target) = two_node_graph(Extension::Label('f'));
    assert_eq!((root, target), (label_root, label_target));

    let mut epsilon_then_label = epsilon.clone();
    assert!(
        epsilon_then_label
            .join(&label, &mut crate::meter::DummyMeter)
            .unwrap()
    );
    let mut label_then_epsilon = label;
    assert!(
        label_then_epsilon
            .join(&epsilon, &mut crate::meter::DummyMeter)
            .unwrap()
    );

    let expected = BTreeSet::from([(vec![], false), (vec!['f'], false)]);
    assert_eq!(path_set(&epsilon_then_label, root, target), expected);
    assert_eq!(
        path_set(&epsilon_then_label, root, target),
        path_set(&label_then_epsilon, root, target)
    );
    assert!(!is_writable(&epsilon_then_label, root));
    assert!(is_writable(&epsilon_then_label, target));
}

#[test]
fn join_of_epsilon_and_dot_star_is_not_writable() {
    let (mut epsilon, root, target) = two_node_graph(Extension::Epsilon);
    let (dot_star, dot_star_root, dot_star_target) = two_node_graph(Extension::DotStar);
    assert_eq!((root, target), (dot_star_root, dot_star_target));

    assert!(
        epsilon
            .join(&dot_star, &mut crate::meter::DummyMeter)
            .unwrap()
    );
    assert_eq!(
        path_set(&epsilon, root, target),
        BTreeSet::from([(vec![], false), (vec![], true)])
    );
    assert!(!is_writable(&epsilon, root));
}

#[test]
fn join_is_associative_for_epsilon_label_and_dot_star() {
    let (epsilon, root, target) = two_node_graph(Extension::Epsilon);
    let (label, _, _) = two_node_graph(Extension::Label('f'));
    let (dot_star, _, _) = two_node_graph(Extension::DotStar);

    let mut left = epsilon.clone();
    left.join(&label, &mut crate::meter::DummyMeter).unwrap();
    left.join(&dot_star, &mut crate::meter::DummyMeter).unwrap();

    let mut right_branch = label;
    right_branch
        .join(&dot_star, &mut crate::meter::DummyMeter)
        .unwrap();
    let mut right = epsilon;
    right
        .join(&right_branch, &mut crate::meter::DummyMeter)
        .unwrap();

    assert_eq!(
        path_set(&left, root, target),
        path_set(&right, root, target)
    );
}

#[test]
fn join_ignores_edges_to_references_missing_from_one_branch() {
    let meter = &mut crate::meter::DummyMeter;
    let (mut smaller, refs) = Graph::<u8, char>::new(3, [(0, 0, true)]).unwrap();
    let root = refs[&0];
    let leaf = smaller
        .extend_by_label(0, [root], true, 'z', meter)
        .unwrap();
    let remapping = BTreeMap::from([(root, 0), (leaf, 2)]);
    let smaller_root = root.canonicalize(&remapping).unwrap();
    let smaller_leaf = leaf.canonicalize(&remapping).unwrap();
    smaller.canonicalize(&remapping).unwrap();

    let (larger, larger_root, _, larger_leaf) = three_node_graph('a', 'b');
    assert_eq!((smaller_root, smaller_leaf), (larger_root, larger_leaf));

    assert!(smaller.join(&larger, meter).unwrap());
    assert_eq!(
        path_set(&smaller, smaller_root, smaller_leaf),
        BTreeSet::from([(vec!['a', 'b'], false), (vec!['z'], false),])
    );
}

// -------------------------------------------------------------------------------------------------
// Release Tests
// -------------------------------------------------------------------------------------------------

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
fn release_preserves_paths_between_surviving_references() {
    let (mut graph, root, middle, leaf) = three_node_graph('a', 'b');
    let expected = BTreeSet::from([(vec!['a', 'b'], false)]);
    assert_eq!(path_set(&graph, root, leaf), expected);

    graph
        .release(middle, &mut crate::meter::DummyMeter)
        .unwrap();

    assert_eq!(path_set(&graph, root, leaf), expected);
    assert!(!is_writable(&graph, root));
    assert!(is_writable(&graph, leaf));
}

#[test]
fn release_of_epsilon_intermediate_preserves_field_path() {
    let meter = &mut crate::meter::DummyMeter;
    let (mut graph, refs) = Graph::<u8, char>::new(3, [(0, 0, true)]).unwrap();
    let root = refs[&0];
    let alias = graph.extend_by_epsilon(0, [root], true, meter).unwrap();
    let field = graph.extend_by_label(0, [alias], true, 'f', meter).unwrap();
    let refs = canonicalize(&mut graph, &[root, alias, field]);
    let (root, alias, field) = (refs[0], refs[1], refs[2]);

    graph.release(alias, meter).unwrap();

    assert_eq!(
        path_set(&graph, root, field),
        BTreeSet::from([(vec!['f'], false)])
    );
    assert!(!is_writable(&graph, root));
    assert!(is_writable(&graph, field));
}

#[test]
fn release_of_dot_star_intermediate_preserves_dot_star_path() {
    let meter = &mut crate::meter::DummyMeter;
    let (mut graph, refs) = Graph::<u8, char>::new(3, [(0, 0, true)]).unwrap();
    let root = refs[&0];
    let call_result = graph
        .extend_by_dot_star_for_call(0, &BTreeSet::from([root]), vec![true], meter)
        .unwrap()[0];
    let field = graph
        .extend_by_label(0, [call_result], true, 'f', meter)
        .unwrap();
    let refs = canonicalize(&mut graph, &[root, call_result, field]);
    let (root, call_result, field) = (refs[0], refs[1], refs[2]);

    graph.release(call_result, meter).unwrap();

    assert_eq!(
        path_set(&graph, root, field),
        BTreeSet::from([(vec![], true), (vec!['f'], false)])
    );
    assert!(!is_writable(&graph, root));
    assert!(is_writable(&graph, field));
}

// -------------------------------------------------------------------------------------------------
// Combined Join and Release Tests
// -------------------------------------------------------------------------------------------------

#[test]
fn releasing_before_or_after_join_preserves_the_same_paths() {
    let (left, root, middle, leaf) = three_node_graph('a', 'b');
    let (right, right_root, right_middle, right_leaf) = three_node_graph('c', 'd');
    assert_eq!((root, middle, leaf), (right_root, right_middle, right_leaf));

    let mut join_then_release = left.clone();
    join_then_release
        .join(&right, &mut crate::meter::DummyMeter)
        .unwrap();
    join_then_release
        .release(middle, &mut crate::meter::DummyMeter)
        .unwrap();

    let mut release_then_join_left = left;
    release_then_join_left
        .release(middle, &mut crate::meter::DummyMeter)
        .unwrap();
    let mut release_then_join_right = right;
    release_then_join_right
        .release(middle, &mut crate::meter::DummyMeter)
        .unwrap();
    release_then_join_left
        .join(&release_then_join_right, &mut crate::meter::DummyMeter)
        .unwrap();

    let expected = BTreeSet::from([(vec!['a', 'b'], false), (vec!['c', 'd'], false)]);
    assert_eq!(path_set(&join_then_release, root, leaf), expected);
    assert_eq!(
        path_set(&join_then_release, root, leaf),
        path_set(&release_then_join_left, root, leaf)
    );
}
