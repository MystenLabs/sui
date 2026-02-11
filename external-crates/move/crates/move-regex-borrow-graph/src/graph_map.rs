// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use indexmap::IndexMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeIndex(u32);

#[derive(Debug, Clone)]
/// A simple graph implementation that uses two `IndexMap`s. One to store a node "weights" and one
/// To store edge "weights".
/// In the context of the borrow graph, the node weights will be the `Ref`, and the edge weights
/// will be an `Edge<Loc, Lbl>`.
pub struct GraphMap<N, E> {
    next: u32,
    node_weights: IndexMap<NodeIndex, N>,
    edge_weights: IndexMap<(NodeIndex, NodeIndex), E>,
}

impl<N, E> GraphMap<N, E> {
    /// Creates a  new graph with a given capacity for the nodes. This number is assumed to be
    /// the maximum number of canonical references at the end of a block
    pub fn new(canonical_reference_capacity: usize) -> Self {
        debug_assert!(canonical_reference_capacity < 512);
        Self {
            next: 0,
            node_weights: IndexMap::with_capacity(canonical_reference_capacity),
            edge_weights: IndexMap::with_capacity(canonical_reference_capacity * 3 / 2),
        }
    }

    /// Clear the graph of all nodes and edges.
    /// NOTE: Do not keep any `NodeIndex` values from before this call. They will be invalid and
    /// may panic if used (at the very least they will give the wrong nodes/edges)
    pub fn clear(&mut self) {
        self.next = 0;
        self.node_weights.clear();
        self.edge_weights.clear();
    }

    /// Recalculate the `next` field basedo on the current nodes. This should be called after
    /// canonicalization
    pub fn minimize(&mut self) {
        let mut max_next = 0;
        for index in self.node_weights.keys() {
            max_next = max_next.max(index.0.saturating_add(1));
        }
        self.next = max_next;
    }

    /// Returns the number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.node_weights.len()
    }

    /// Adds a node (with the given weight) to the graph and returns its index.
    pub fn add_node(&mut self, weight: N) -> NodeIndex {
        let index = NodeIndex(self.next);
        self.next = self.next.checked_add(1).expect("NodeIndex overflow");
        self.node_weights.insert(index, weight);
        index
    }

    /// Adds an edge (with the given weight) to the graph. The nodes must already exist.
    pub fn add_edge(&mut self, from: NodeIndex, weight: E, to: NodeIndex) {
        let prev = self.edge_weights.insert((from, to), weight);
        assert!(
            prev.is_none(),
            "Edge from {:?} to {:?} already exists",
            from,
            to
        );
    }

    /// Returns true iff the graph contains the specified node.
    pub fn contains_node(&self, index: NodeIndex) -> bool {
        self.node_weights.contains_key(&index)
    }

    /// Returns the weight of the specified node, or None if the node does not exist.
    pub fn node_weight(&self, index: NodeIndex) -> Option<&N> {
        self.node_weights.get(&index)
    }

    /// Returns a mutable reference to the weight of the specified node, or None if the node does
    /// not exist.
    pub fn node_weight_mut(&mut self, index: NodeIndex) -> Option<&mut N> {
        self.node_weights.get_mut(&index)
    }

    /// Returns true iff the graph contains an edge from `from` to `to`.
    pub fn contains_edge(&self, from: NodeIndex, to: NodeIndex) -> bool {
        self.edge_weights.contains_key(&(from, to))
    }

    /// Returns the weight of the edge from `from` to `to`, or None if the edge does not exist.
    pub fn edge_weight(&self, from: NodeIndex, to: NodeIndex) -> Option<&E> {
        self.edge_weights.get(&(from, to))
    }

    /// Returns a mutable reference to the weight of the edge from `from` to `to`, or None if the
    /// edge does not exist.
    pub fn edge_weight_mut(&mut self, from: NodeIndex, to: NodeIndex) -> Option<&mut E> {
        self.edge_weights.get_mut(&(from, to))
    }

    /// Removes the specified node and all edges to/from it. Searching for the edges to remove
    /// is O(E)
    /// Panics if the node does not exist.
    pub fn remove_node(&mut self, index: NodeIndex) {
        let node = self.node_weights.swap_remove(&index);
        assert!(node.is_some(), "Node {:?} does not exist", index);
        self.edge_weights
            .retain(|(p, s), _| *p != index && *s != index);
    }

    /// Returns an iterator over the outgoing edges from the specified node
    pub fn outgoing_edges(&self, index: NodeIndex) -> impl Iterator<Item = (&E, NodeIndex)> + '_ {
        self.edge_weights.iter().filter_map(
            move |((p, s), e)| {
                if *p == index { Some((e, *s)) } else { None }
            },
        )
    }

    /// Returns an iterator over the incoming edges to the specified node
    pub fn incoming_edges(&self, index: NodeIndex) -> impl Iterator<Item = (NodeIndex, &E)> + '_ {
        self.edge_weights.iter().filter_map(
            move |((p, s), e)| {
                if *s == index { Some((*p, e)) } else { None }
            },
        )
    }

    /// Returns an iterator over all edges in the graph, as (from, weight, to) triples.
    pub fn all_edges(&self) -> impl Iterator<Item = (NodeIndex, &E, NodeIndex)> + '_ {
        self.edge_weights.iter().map(|((p, s), e)| (*p, e, *s))
    }

    pub(crate) fn check_invariants(&self) {
        #[cfg(debug_assertions)]
        {
            // Check all edges point to nodes in the graph
            for (from, _weight, to) in self.all_edges() {
                debug_assert!(
                    self.contains_node(from),
                    "Edge from non-existent node: {:?}",
                    from
                );
                debug_assert!(
                    self.contains_node(to),
                    "Edge to non-existent node: {:?}",
                    to
                );
            }
            // Check that all node indices are less than `next`
            for index in self.node_weights.keys() {
                debug_assert!(
                    index.0 < self.next,
                    "NodeIndex {:?} out of bounds (next: {:?})",
                    index,
                    self.next
                );
            }
        }
    }
}
