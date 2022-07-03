// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use arc_swap::ArcSwap;
use itertools::Itertools;
use once_cell::sync::OnceCell;
use std::{
    ops::Deref,
    sync::{Arc, Weak},
};

pub mod bft;
pub mod node_dag;

/// Reference-counted pointers to a Node
#[derive(Debug)]
pub struct NodeRef<T>(Arc<Node<T>>);

// reimplemented to avoid a clone bound on T
impl<T> Clone for NodeRef<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> std::hash::Hash for NodeRef<T> {
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        (Arc::as_ptr(&self.0)).hash(state)
    }
}

impl<T> PartialEq<NodeRef<T>> for NodeRef<T> {
    fn eq(&self, other: &NodeRef<T>) -> bool {
        Arc::as_ptr(&self.0) == Arc::as_ptr(&other.0)
    }
}

impl<T> Eq for NodeRef<T> {}

// The NodeRef is just a wrapper around a smart pointer (only here to define reference equality
// when inserting in a collection).
impl<T> Deref for NodeRef<T> {
    type Target = Arc<Node<T>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> From<Arc<Node<T>>> for NodeRef<T> {
    fn from(pointer: Arc<Node<T>>) -> Self {
        NodeRef(pointer)
    }
}

impl<T> From<Node<T>> for NodeRef<T> {
    fn from(node: Node<T>) -> Self {
        NodeRef::from_pointee(node)
    }
}

impl<T> NodeRef<T> {
    /// Returns a NodeRef pointing at the Node passed as argument
    ///
    /// # Example
    ///
    /// ```
    /// use dag::{ Node, NodeRef };
    ///
    /// let node = Node::new_leaf(1, false);
    /// // Note the 2 derefs: one for the newtype, one for the Arc
    /// assert_eq!(Node::new_leaf(1, false), **NodeRef::from_pointee(node));
    /// ```
    pub fn from_pointee(val: Node<T>) -> Self {
        Arc::new(val).into()
    }
}

/// Non reference-counted pointers to a Node
pub type WeakNodeRef<T> = Weak<Node<T>>;

/// The Dag node, aka vertex.
#[derive(Debug)]
pub struct Node<T> {
    /// The antecedents of the Node, aka the edges of the DAG in association list form.
    parents: ArcSwap<Vec<NodeRef<T>>>,
    /// Whether the node is "empty" in some sense: the nodes have a value payload on top of the connections they form.
    /// An "empty" node can be reclaimed in ways that preserve the connectedness of the graph.
    compressible: OnceCell<()>,
    /// The value payload of the node
    value: T,
}

impl<T: PartialEq> PartialEq for Node<T> {
    fn eq(&self, other: &Self) -> bool {
        *self.parents.load() == *other.parents.load()
            && self.is_compressible() == other.is_compressible()
            && self.value.eq(&other.value)
    }
}

impl<T: Eq> Eq for Node<T> {}

impl<T> Node<T> {
    /// Create a new DAG leaf node that contains the given value.
    ///
    /// # Example
    ///
    /// ```
    /// use dag::{ Node, NodeRef };
    ///
    /// let node = Node::new_leaf(1, false);
    /// ```
    pub fn new_leaf(value: T, compressible: bool) -> Self {
        Self::new(value, compressible, Vec::default())
    }

    /// Create a new DAG inner node that contains the given value and points to the given parents.
    pub fn new(value: T, compressible: bool, parents: Vec<NodeRef<T>>) -> Self {
        let once_cell = {
            let cell = OnceCell::new();
            if compressible {
                let _ = cell.set(());
            }
            cell
        };
        Self {
            parents: ArcSwap::from_pointee(parents),
            compressible: once_cell,
            value,
        }
    }

    /// Return the value payload of the node
    ///
    /// # Example
    ///
    /// ```
    /// use dag::Node;
    ///
    /// let node = Node::new_leaf(1, false);
    /// assert_eq!(*node.value(), 1);
    /// ```
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Is the node parent-less?
    ///
    /// # Examples
    ///
    /// ```
    /// use dag::Node;
    ///
    /// let node = Node::new_leaf(1, false);
    /// assert_eq!(node.is_leaf(), true);
    /// ```
    pub fn is_leaf(&self) -> bool {
        self.parents.load().is_empty()
    }

    /// Is the node compressible?
    ///
    /// # Examples
    ///
    /// ```
    /// use dag::Node;
    ///
    /// let node = Node::new_leaf(1, true);
    /// assert_eq!(node.is_compressible(), true);
    /// ```
    pub fn is_compressible(&self) -> bool {
        self.compressible.get().is_some()
    }

    /// Make the node compressible.
    /// Returns true if the node was made compressible, false if it already was.
    ///
    /// Beware: this operation is irreversible.
    ///
    /// # Examples
    ///
    /// ```
    /// use dag::Node;
    ///
    /// let node = Node::new_leaf(1, false);
    /// assert_eq!(node.make_compressible(), true);
    /// let node2 = Node::new_leaf(2, true);
    /// assert_eq!(node.make_compressible(), false);
    /// ```
    pub fn make_compressible(&self) -> bool {
        self.compressible.set(()).is_ok()
    }

    // What's the maximum distance from this to a leaf?
    #[cfg(test)]
    fn height(&self) -> usize {
        if self.is_leaf() {
            1
        } else {
            let max_p_heights = self
                .parents
                .load()
                .iter()
                .map(|p| p.height())
                .max()
                .unwrap_or(1);
            max_p_heights + 1
        }
    }

    /// Get the parent nodes in a [`Vec`]. Note the "parents" are in the reverse of the usual tree structure.
    ///
    /// If this node is a leaf node, this function returns [`Vec::empty()`].
    fn raw_parents_snapshot(&self) -> Vec<NodeRef<T>> {
        self.parents.load().to_vec()
    }

    // A trivial node is one whose parents are all incompressible (or a leaf)
    fn is_trivial(&self) -> bool {
        self.parents.load().iter().all(|p| !p.is_compressible())
    }
}

impl<T: Sync + Send + std::fmt::Debug> Node<T> {
    /// Compress the path from this node to the next incompressible layer of the DAG.
    /// Returns the parents of the node.
    ///
    /// After path compression, one of these three conditions holds:
    /// * This node is a leaf node;
    /// * This node has only incompressible parents, and keeps them;
    /// * This node has compressible parents, and after path compression, they are replaced by their closest incompressible ancestors.
    pub fn parents(&self) -> Vec<NodeRef<T>> {
        // Quick check to bail the trivial situations out in which:
        // * `self` is itself a leaf node;
        // * The parent nodes of `self` are all incompressible node.
        //
        // In any of the two cases above, we don't have to do anything.
        if self.is_trivial() {
            return self.raw_parents_snapshot();
        }

        // create set of initial compressible and incompressible nodes
        let (mut compressible, mut incompressible): (Vec<NodeRef<T>>, Vec<NodeRef<T>>) = self
            .raw_parents_snapshot()
            .into_iter()
            .partition(|p| p.is_compressible());

        while !compressible.is_empty() {
            let curr = compressible.pop().unwrap();
            let (curr_compressible, curr_incompressible): (Vec<NodeRef<T>>, Vec<NodeRef<T>>) = curr
                .raw_parents_snapshot()
                .into_iter()
                .partition(|p| p.is_compressible());
            compressible.extend(curr_compressible);
            incompressible.extend(curr_incompressible);
            // deduplicate
            incompressible = incompressible
                .into_iter()
                .unique_by(|arc| Arc::as_ptr(arc))
                .collect();
        }

        // save compressed set
        self.parents.store(Arc::new(incompressible));
        debug_assert!(self.is_trivial());
        self.raw_parents_snapshot()
    }
}

/// Returns a Breadth-first search of the DAG, as an iterator of [`NodeRef`]
/// This is expected to be used in conjunction with a [`NodeDag<T>`], walking the graph from one of its heads.
///
pub fn bfs<T: Sync + Send + std::fmt::Debug>(
    initial: NodeRef<T>,
) -> impl Iterator<Item = NodeRef<T>> {
    bft::Bft::new(initial, |node| node.parents().into_iter())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    prop_compose! {
        pub fn arb_leaf_node()(
            value in any::<u64>(),
            compressible in any::<bool>(),
        ) -> Node<u64> {
            Node::new_leaf(value, compressible)
        }
    }

    prop_compose! {
        pub fn arb_inner_node(prior_round: Vec<NodeRef<u64>>)(
            // this is a 50% inclusion rate, in production we'd shoot for > 67%
            picks in prop::collection::vec(any::<bool>(), prior_round.len()..=prior_round.len()),
            value in any::<u64>(),
            compressible in any::<bool>(),
        ) -> Node<u64> {
            let parents = prior_round.iter().zip(picks).flat_map(|(parent, pick)| pick.then_some(parent.clone())).collect();
            Node::new(value, compressible, parents)
        }
    }

    prop_compose! {
        pub fn next_round(prior_round: Vec<NodeRef<u64>>)(
            nodes in { let n = prior_round.len(); prop::collection::vec(arb_inner_node(prior_round), n..=n) }
        ) -> Vec<NodeRef<u64>> {
            nodes.into_iter().map(|node| node.into()).collect()
        }
    }

    pub fn arb_dag_complete(
        authorities: usize,
        rounds: usize,
    ) -> impl Strategy<Value = Vec<NodeRef<u64>>> {
        let initial_round =
            prop::collection::vec(arb_leaf_node().no_shrink(), authorities..=authorities)
                .prop_map(|nodevec| nodevec.into_iter().map(|node| node.into()).collect());

        initial_round.prop_recursive(
            rounds as u32,                 // max rounds level deep
            (authorities * rounds) as u32, // max authorities nodes total
            authorities as u32,            // authorities nodes per round
            move |inner| inner.prop_flat_map(next_round),
        )
    }

    proptest! {
        #[test]
        fn test_dag_sanity_check(
            dag in arb_dag_complete(10, 10)
        ) {
            assert!(dag.len() <= 10);
            assert!(dag.iter().all(|node| node.height() <= 10));
            assert!(dag.iter().all(|node| node.raw_parents_snapshot().len() <= 10));
        }

        #[test]
        fn test_path_compression(
            dag in arb_dag_complete(10, 100)
        ) {
            let first = dag.first().unwrap();
            let initial_height = first.height();
            let _parents = first.parents();
            let final_height = first.height();
            assert!(final_height <= initial_height);
            assert!(first.is_trivial())
        }

        #[test]
        fn test_path_compression_bfs(
            dag in arb_dag_complete(10, 100)
        ) {
            let first = dag.first().unwrap();
            let iter = bfs(first.clone());
            // The first node may end up compressible as a result of our random DAG
            let mut is_first = true;
            for node in iter {
                if !is_first {
                assert!(!node.is_compressible())
                }
                is_first = false;
            }

        }

    }
}
