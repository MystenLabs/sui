// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::graph_map::GraphMap;

const DEFAULT_CAPACITY: usize = 16;

// -------------------------------------------------------------------------------------------------
// Nod Tests
// -------------------------------------------------------------------------------------------------

#[test]
fn new_graph_is_empty() {
    let g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    assert_eq!(g.node_count(), 0);
}

#[test]
fn add_node_increments_count() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    g.add_node(42);
    assert_eq!(g.node_count(), 1);
    g.add_node(43);
    assert_eq!(g.node_count(), 2);
}

#[test]
fn contains_node_returns_true_for_existing_node() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n = g.add_node(42);
    assert!(g.contains_node(n));
}

#[test]
fn node_weight_returns_correct_value() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n = g.add_node(42);
    assert_eq!(g.node_weight(n), Some(&42));
}

#[test]
fn node_weight_mut_allows_modification() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n = g.add_node(42);
    *g.node_weight_mut(n).unwrap() = 100;
    assert_eq!(g.node_weight(n), Some(&100));
}

#[test]
fn remove_node_decrements_count() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n1 = g.add_node(1);
    let _n2 = g.add_node(2);
    assert_eq!(g.node_count(), 2);
    g.remove_node(n1);
    assert_eq!(g.node_count(), 1);
}

#[test]
fn remove_node_makes_contains_return_false() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n = g.add_node(42);
    assert!(g.contains_node(n));
    g.remove_node(n);
    assert!(!g.contains_node(n));
}

#[test]
#[should_panic(expected = "does not exist")]
fn removed_node_panics() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n = g.add_node(42);
    g.remove_node(n);
    // panics
    g.remove_node(n);
}

// -------------------------------------------------------------------------------------------------
// Edge Tests
// -------------------------------------------------------------------------------------------------

#[test]
fn add_edge_creates_edge() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n1 = g.add_node(1);
    let n2 = g.add_node(2);
    g.add_edge(n1, "edge", n2);
    assert!(g.contains_edge(n1, n2));
}

#[test]
fn contains_edge_returns_false_for_nonexistent_edge() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n1 = g.add_node(1);
    let n2 = g.add_node(2);
    assert!(!g.contains_edge(n1, n2));
}

#[test]
fn edge_is_directional() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n1 = g.add_node(1);
    let n2 = g.add_node(2);
    g.add_edge(n1, "forward", n2);
    assert!(g.contains_edge(n1, n2));
    assert!(!g.contains_edge(n2, n1));
}

#[test]
fn edge_weight_returns_correct_value() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n1 = g.add_node(1);
    let n2 = g.add_node(2);
    g.add_edge(n1, "my_edge", n2);
    assert_eq!(g.edge_weight(n1, n2), Some(&"my_edge"));
}

#[test]
fn edge_weight_returns_none_for_nonexistent_edge() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n1 = g.add_node(1);
    let n2 = g.add_node(2);
    assert_eq!(g.edge_weight(n1, n2), None);
}

#[test]
fn edge_weight_mut_allows_modification() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n1 = g.add_node(1);
    let n2 = g.add_node(2);
    g.add_edge(n1, "old", n2);
    *g.edge_weight_mut(n1, n2).unwrap() = "new";
    assert_eq!(g.edge_weight(n1, n2), Some(&"new"));
}

#[test]
#[should_panic(expected = "already exists")]
fn add_duplicate_edge_panics() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n1 = g.add_node(1);
    let n2 = g.add_node(2);
    g.add_edge(n1, "first", n2);
    g.add_edge(n1, "second", n2);
}

#[test]
fn self_edge_allowed() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n = g.add_node(1);
    g.add_edge(n, "self", n);
    assert!(g.contains_edge(n, n));
    assert_eq!(g.edge_weight(n, n), Some(&"self"));
}

#[test]
fn remove_node_removes_outgoing_edges() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n1 = g.add_node(1);
    let n2 = g.add_node(2);
    g.add_edge(n1, "out", n2);
    g.remove_node(n1);
    assert!(!g.contains_edge(n1, n2));
}

#[test]
fn remove_node_removes_incoming_edges() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n1 = g.add_node(1);
    let n2 = g.add_node(2);
    g.add_edge(n1, "in", n2);
    g.remove_node(n2);
    assert!(!g.contains_edge(n1, n2));
}

// -------------------------------------------------------------------------------------------------
// Edge Iteration Tests
// -------------------------------------------------------------------------------------------------

#[test]
fn outgoing_edges_returns_correct_edges() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n1 = g.add_node(1);
    let n2 = g.add_node(2);
    let n3 = g.add_node(3);
    g.add_edge(n1, "n1_to_n2", n2);
    g.add_edge(n1, "n1_to_n3", n3);
    g.add_edge(n2, "n2_to_n3", n3);

    let n1_outgoing: Vec<_> = g.outgoing_edges(n1).collect();
    assert_eq!(n1_outgoing.len(), 2);
    assert!(n1_outgoing.contains(&(&"n1_to_n2", n2)));
    assert!(n1_outgoing.contains(&(&"n1_to_n3", n3)));
}

#[test]
fn outgoing_edges_empty_for_node_with_no_outgoing() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n1 = g.add_node(1);
    let n2 = g.add_node(2);
    g.add_edge(n2, "n2_to_n1", n1);

    let outgoing: Vec<_> = g.outgoing_edges(n1).collect();
    assert!(outgoing.is_empty());
}

#[test]
fn incoming_edges_returns_correct_edges() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n1 = g.add_node(1);
    let n2 = g.add_node(2);
    let n3 = g.add_node(3);
    g.add_edge(n1, "n1_to_n3", n3);
    g.add_edge(n2, "n2_to_n3", n3);
    g.add_edge(n1, "n1_to_n2", n2);

    let incoming: Vec<_> = g.incoming_edges(n3).collect();
    assert_eq!(incoming.len(), 2);
    assert!(incoming.contains(&(n1, &"n1_to_n3")));
    assert!(incoming.contains(&(n2, &"n2_to_n3")));
}

#[test]
fn incoming_edges_empty_for_node_with_no_incoming() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n1 = g.add_node(1);
    let n2 = g.add_node(2);
    g.add_edge(n1, "n1_to_n2", n2);

    let incoming: Vec<_> = g.incoming_edges(n1).collect();
    assert!(incoming.is_empty());
}

#[test]
fn all_edges_returns_all_edges() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n1 = g.add_node(1);
    let n2 = g.add_node(2);
    let n3 = g.add_node(3);
    g.add_edge(n1, "n1_to_n2", n2);
    g.add_edge(n2, "n2_to_n3", n3);
    g.add_edge(n1, "n1_to_n3", n3);

    let all: Vec<_> = g.all_edges().collect();
    assert_eq!(all.len(), 3);
    assert!(all.contains(&(n1, &"n1_to_n2", n2)));
    assert!(all.contains(&(n2, &"n2_to_n3", n3)));
    assert!(all.contains(&(n1, &"n1_to_n3", n3)));
}

#[test]
fn all_edges_empty_for_graph_with_no_edges() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    g.add_node(1);
    g.add_node(2);

    let all: Vec<_> = g.all_edges().collect();
    assert!(all.is_empty());
}

// -------------------------------------------------------------------------------------------------
// Clear and Minimize Tests
// -------------------------------------------------------------------------------------------------

#[test]
fn clear_removes_all_nodes_and_edges() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n1 = g.add_node(1);
    let n2 = g.add_node(2);
    g.add_edge(n1, "edge", n2);

    g.clear();

    assert_eq!(g.node_count(), 0);
    assert!(!g.contains_node(n1));
    assert!(!g.contains_node(n2));
    assert!(!g.contains_edge(n1, n2));
}

#[test]
fn clear_resets_node_index_counter() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n1 = g.add_node(1);
    g.add_node(2);
    g.clear();

    let n3 = g.add_node(3);
    assert_eq!(n1, n3);
}

#[test]
fn add_node_returns_unique_indices_but_minimize_reuses() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n1 = g.add_node(1);
    let n2 = g.add_node(2);
    let n3 = g.add_node(3);
    assert_ne!(n1, n2);
    assert_ne!(n2, n3);
    assert_ne!(n1, n3);
    g.remove_node(n3);
    // unique even after removal
    let n4 = g.add_node(4);
    assert_ne!(n4, n1);
    assert_ne!(n4, n2);
    assert_ne!(n4, n3);
    g.remove_node(n4);
    // minimize and n3 will be reused
    g.minimize();
    let n5 = g.add_node(5);
    assert_eq!(n5, n3);
}
// -------------------------------------------------------------------------------------------------
// Check Invariants Tests
// -------------------------------------------------------------------------------------------------

#[test]
fn check_invariants_passes_for_valid_graph() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n1 = g.add_node(1);
    let n2 = g.add_node(2);
    g.add_edge(n1, "edge", n2);
    g.check_invariants();
}

#[test]
fn check_invariants_passes_for_empty_graph() {
    let g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    g.check_invariants();
}

// -------------------------------------------------------------------------------------------------
// Complex Graph Scenarios
// -------------------------------------------------------------------------------------------------

#[test]
fn diamond_graph_structure() {
    let mut g: GraphMap<&str, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let top = g.add_node("top");
    let left = g.add_node("left");
    let right = g.add_node("right");
    let bottom = g.add_node("bottom");

    g.add_edge(top, "top_to_left", left);
    g.add_edge(top, "top_to_right", right);
    g.add_edge(left, "left_to_bottom", bottom);
    g.add_edge(right, "right_to_bottom", bottom);

    assert_eq!(g.node_count(), 4);

    let top_outgoing: Vec<_> = g.outgoing_edges(top).collect();
    assert_eq!(top_outgoing.len(), 2);

    let bottom_incoming: Vec<_> = g.incoming_edges(bottom).collect();
    assert_eq!(bottom_incoming.len(), 2);

    g.check_invariants();
}

#[test]
fn list_graph_structure() {
    let mut g: GraphMap<u32, u32> = GraphMap::new(DEFAULT_CAPACITY);
    let mut nodes = Vec::new();
    for i in 0..5 {
        nodes.push(g.add_node(i));
    }
    for i in 0..4 {
        g.add_edge(nodes[i], i as u32, nodes[i + 1]);
    }

    assert_eq!(g.node_count(), 5);

    for i in 0..4 {
        assert!(g.contains_edge(nodes[i], nodes[i + 1]));
        assert!(!g.contains_edge(nodes[i + 1], nodes[i]));
    }

    g.check_invariants();
}

#[test]
fn multiple_self_loops() {
    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);
    let n1 = g.add_node(1);
    let n2 = g.add_node(2);
    g.add_edge(n1, "self", n1);
    g.add_edge(n2, "self", n2);
    g.add_edge(n1, "n1_to_n2", n2);

    assert!(g.contains_edge(n1, n1));
    assert!(g.contains_edge(n2, n2));
    assert!(g.contains_edge(n1, n2));

    let n1_outgoing: Vec<_> = g.outgoing_edges(n1).collect();
    assert_eq!(n1_outgoing.len(), 2);

    g.check_invariants();
}

#[test]
fn edge_iterators_consistent_and_all_contained() {
    use crate::graph_map::NodeIndex;
    use std::collections::BTreeSet;

    let mut g: GraphMap<u32, &str> = GraphMap::new(DEFAULT_CAPACITY);

    // Build an interesting graph: diamond with self-loops and back-edges
    let n1 = g.add_node(1);
    let n2 = g.add_node(2);
    let n3 = g.add_node(3);
    let n4 = g.add_node(4);
    let n5 = g.add_node(5);

    let nodes = [n1, n2, n3, n4, n5];

    // Diamond: n1 -> n2, n1 -> n3, n2 -> n4, n3 -> n4
    g.add_edge(n1, "n1_n2", n2);
    g.add_edge(n1, "n1_n3", n3);
    g.add_edge(n2, "n2_n4", n4);
    g.add_edge(n3, "n3_n4", n4);
    // Self-loops
    g.add_edge(n1, "n1_self", n1);
    g.add_edge(n4, "n4_self", n4);
    // Extra edges
    g.add_edge(n4, "n4_n5", n5);
    g.add_edge(n2, "n2_n3", n3);

    // Collect edges from all_edges
    let all_edges_set: BTreeSet<(NodeIndex, &str, NodeIndex)> =
        g.all_edges().map(|(from, e, to)| (from, *e, to)).collect();

    // Collect edges by iterating outgoing_edges for each node
    let outgoing_set = nodes
        .iter()
        .copied()
        .flat_map(|from| g.outgoing_edges(from).map(move |(e, to)| (from, *e, to)))
        .collect();

    // Collect edges by iterating incoming_edges for each node
    let incoming_set = nodes
        .iter()
        .copied()
        .flat_map(|to| g.incoming_edges(to).map(move |(from, e)| (from, *e, to)))
        .collect();

    // All three methods should produce the same set of edges
    assert_eq!(all_edges_set, outgoing_set);
    assert_eq!(all_edges_set, incoming_set);

    // Verify all nodes are contained
    for &node in &nodes {
        assert!(g.contains_node(node));
        assert!(g.node_weight(node).is_some());
    }

    // Verify all edges are contained and have weights
    for &(from, e, to) in &all_edges_set {
        assert!(g.contains_edge(from, to));
        assert_eq!(*g.edge_weight(from, to).unwrap(), e);
    }

    g.check_invariants();
}
