// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use indexmap::IndexMap;

#[derive(Debug)]
/// An error marker for a case that should be impossible to reach if the graph is used correctly.
pub struct Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeIndex {
    generation: u32,
    id: u32,
}

#[derive(Debug, Clone)]
/// A simple graph implementation that uses a `BTreeMap` for node "weights" and an `IndexMap` for
/// edge "weights".
/// In the context of the borrow graph, the node weights will be the `Ref`, and the edge weights
/// will be an `Edge<Loc, Lbl>`.
pub struct GraphMap<N, E> {
    generation: u32,
    next: u32,
    node_weights: BTreeMap<NodeIndex, N>,
    edge_weights: IndexMap<(NodeIndex, NodeIndex), E>,
}

pub type EdgeEntry<'a, E> = indexmap::map::Entry<'a, (NodeIndex, NodeIndex), E>;

impl<N, E> GraphMap<N, E> {
    /// Creates a new graph with a given capacity for the nodes. This number is assumed to be
    /// the maximum number of canonical references at the end of a block
    pub fn new(canonical_reference_capacity: usize) -> Self {
        debug_assert!(canonical_reference_capacity < 512);
        Self {
            generation: 0,
            next: 0,
            node_weights: BTreeMap::new(),
            edge_weights: IndexMap::with_capacity(canonical_reference_capacity * 3 / 2),
        }
    }

    /// Clear the graph of all nodes and edges.
    /// NOTE: Do not keep any `NodeIndex` values from before this call. They will be invalid and
    /// may panic if used (at the very least they will give the wrong nodes/edges)
    pub fn clear(&mut self) -> Result<(), Error> {
        let Some(generation) = self.generation.checked_add(1) else {
            debug_assert!(false, "generation overflow");
            return Err(Error);
        };
        self.generation = generation;
        self.next = 0;
        self.node_weights.clear();
        self.edge_weights.clear();
        Ok(())
    }

    /// Recalculate the `next` field based on the current nodes. This should be called after
    /// canonicalization
    pub fn minimize(&mut self) -> Result<(), Error> {
        let Some(generation) = self.generation.checked_add(1) else {
            debug_assert!(false, "generation overflow");
            return Err(Error);
        };
        self.generation = generation;
        let mut max_next = 0;
        for index in self.node_weights.keys() {
            max_next = max_next.max(index.id.saturating_add(1));
        }
        self.next = max_next;
        Ok(())
    }

    /// Returns the number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.node_weights.len()
    }

    /// Adds a node (with the given weight) to the graph and returns its index.
    pub fn add_node(&mut self, weight: N) -> Result<NodeIndex, Error> {
        let index = NodeIndex {
            generation: self.generation,
            id: self.next,
        };
        let Some(next) = self.next.checked_add(1) else {
            debug_assert!(false, "NodeIndex id overflow");
            return Err(Error);
        };
        self.next = next;
        let prev = self.node_weights.insert(index, weight);
        if prev.is_some() {
            debug_assert!(false, "NodeIndex {:?} already exists", index);
            return Err(Error);
        }
        Ok(index)
    }

    /// Adds an edge (with the given weight) to the graph. The nodes must already exist.
    pub fn add_edge(&mut self, from: NodeIndex, weight: E, to: NodeIndex) -> Result<(), Error> {
        if !self.contains_node(from) {
            debug_assert!(false, "Cannot add edge from unbound node: {:?}", from);
            return Err(Error);
        }
        if !self.contains_node(to) {
            debug_assert!(false, "Cannot add edge to unbound node: {:?}", to);
            return Err(Error);
        }
        let prev = self.edge_weights.insert((from, to), weight);
        if prev.is_some() {
            debug_assert!(false, "Edge from {:?} to {:?} already exists", from, to);
            return Err(Error);
        }
        Ok(())
    }

    /// Returns true iff the graph contains the specified node.
    pub fn contains_node(&self, index: NodeIndex) -> bool {
        self.node_weights.contains_key(&index)
    }

    /// Returns the weight of the specified node, or None if the node does not exist.
    #[allow(unused)]
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
    #[allow(unused)]
    pub fn edge_weight_mut(&mut self, from: NodeIndex, to: NodeIndex) -> Option<&mut E> {
        self.edge_weights.get_mut(&(from, to))
    }

    /// Returns a mutable entry to the weight of the edge from `from` to `to`, or None if the
    /// edge does not exist.
    pub fn edge_weight_entry(&mut self, from: NodeIndex, to: NodeIndex) -> EdgeEntry<'_, E> {
        self.edge_weights.entry((from, to))
    }

    /// Removes the specified node and all edges to/from it. Searching for the edges to remove
    /// is O(E)
    pub fn remove_node(&mut self, index: NodeIndex) -> Result<(), Error> {
        let node = self.node_weights.remove(&index);
        if node.is_none() {
            debug_assert!(false, "Node {:?} does not exist", index);
            return Err(Error);
        }
        self.edge_weights
            .retain(|(p, s), _| *p != index && *s != index);
        Ok(())
    }

    /// Returns an iterator over the outgoing edges from the specified node
    pub fn outgoing_edges_idx(
        &self,
        index: NodeIndex,
    ) -> impl Iterator<Item = (&E, NodeIndex)> + '_ {
        self.edge_weights.iter().filter_map(
            move |((p, s), e)| {
                if *p == index { Some((e, *s)) } else { None }
            },
        )
    }

    /// Returns an iterator over the outgoing edges (with the node weight) from the specified node.
    /// Validates all edge targets up front, returning an error if any target node is missing.
    pub fn outgoing_edges(
        &self,
        index: NodeIndex,
    ) -> Result<impl Iterator<Item = (&E, &N)> + '_, Error> {
        for ((p, s), _) in &self.edge_weights {
            if *p == index && !self.node_weights.contains_key(s) {
                debug_assert!(false, "Edge to non-existent node: {:?}", s);
                return Err(Error);
            }
        }
        Ok(self.edge_weights.iter().filter_map(move |((p, s), e)| {
            if *p == index {
                Some((e, self.node_weights.get(s).unwrap()))
            } else {
                None
            }
        }))
    }

    /// Returns an iterator over the incoming edges to the specified node
    pub fn incoming_edges_idx(
        &self,
        index: NodeIndex,
    ) -> impl Iterator<Item = (NodeIndex, &E)> + '_ {
        self.edge_weights.iter().filter_map(
            move |((p, s), e)| {
                if *s == index { Some((*p, e)) } else { None }
            },
        )
    }

    /// Returns an iterator over the incoming edges (with the node weight) to the specified node.
    /// Validates all edge sources up front, returning an error if any source node is missing.
    pub fn incoming_edges(
        &self,
        index: NodeIndex,
    ) -> Result<impl Iterator<Item = (&N, &E)> + '_, Error> {
        for ((p, s), _) in &self.edge_weights {
            if *s == index && !self.node_weights.contains_key(p) {
                debug_assert!(false, "Edge from non-existent node: {:?}", p);
                return Err(Error);
            }
        }
        Ok(self.edge_weights.iter().filter_map(move |((p, s), e)| {
            if *s == index {
                Some((self.node_weights.get(p).unwrap(), e))
            } else {
                None
            }
        }))
    }

    /// Returns an iterator over all edges in the graph, as (from, weight, to) triples.
    pub fn all_edges_idx(&self) -> impl Iterator<Item = (NodeIndex, &E, NodeIndex)> + '_ {
        self.edge_weights.iter().map(|((p, s), e)| (*p, e, *s))
    }

    /// Returns an iterator over all edges in the graph, as (from, weight, to) triples with
    /// node weights.
    /// Validates all edge endpoints up front, returning an error if any node is missing.
    pub fn all_edges(&self) -> Result<impl Iterator<Item = (&N, &E, &N)> + '_, Error> {
        for ((p, s), _) in &self.edge_weights {
            if !self.node_weights.contains_key(p) {
                debug_assert!(false, "Edge from non-existent node: {:?}", p);
                return Err(Error);
            }
            if !self.node_weights.contains_key(s) {
                debug_assert!(false, "Edge to non-existent node: {:?}", s);
                return Err(Error);
            }
        }
        Ok(self.edge_weights.iter().map(|((p, s), e)| {
            (
                self.node_weights.get(p).unwrap(),
                e,
                self.node_weights.get(s).unwrap(),
            )
        }))
    }

    pub(crate) fn check_invariants(&self) {
        #[cfg(debug_assertions)]
        {
            // Check all edges point to nodes in the graph
            for (from, _weight, to) in self.all_edges_idx() {
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
                    index.id < self.next,
                    "NodeIndex {:?} out of bounds (next: {:?})",
                    index,
                    self.next
                );
                debug_assert!(
                    index.generation <= self.generation,
                    "NodeIndex {:?} has future generation (current: {:?})",
                    index,
                    self.generation
                );
            }
        }
    }
}
