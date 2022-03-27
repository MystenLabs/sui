// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use dashmap::DashMap;
use either::Either;
use itertools::Itertools;
use rayon::prelude::*;
use std::sync::{Arc, RwLock, Weak};
use thiserror::Error;

pub mod bft;

pub type NodeRef<T> = Arc<RwLock<Node<T>>>;
pub type WeakNodeRef<T> = Weak<RwLock<Node<T>>>;

impl<T> From<Node<T>> for NodeRef<T> {
    fn from(node: Node<T>) -> Self {
        Arc::new(RwLock::new(node))
    }
}

/// The Dag node.
#[derive(Debug, Clone)]
pub struct Node<T> {
    parents: Vec<NodeRef<T>>,
    compressible: bool,
    #[allow(dead_code)] // we'll read values elsewhere
    value: T,
}

impl<T: Sync + Send + std::fmt::Debug> Node<T> {
    /// Create a new DAG  node that contains the given value.
    pub fn new_leaf(value: T, compressible: bool) -> Self {
        Self::new(value, compressible, Vec::default())
    }

    pub fn new(value: T, compressible: bool, parents: Vec<NodeRef<T>>) -> Self {
        Self {
            parents,
            compressible,
            value,
        }
    }

    pub fn is_leaf(&self) -> bool {
        self.parents.is_empty()
    }

    // Is the node compressible?
    pub fn is_compressible(&self) -> bool {
        self.compressible
    }

    // What's the maximum distance from this to a leaf?
    #[cfg(test)]
    fn height(&self) -> usize {
        if self.is_leaf() {
            1
        } else {
            let max_p_heights = self
                .parents
                .iter()
                .map(|p| p.read().expect("failed to acquire a read lock").height())
                .max()
                .unwrap_or(1);
            max_p_heights + 1
        }
    }

    /// Get the parent nodes in a [`Vec`]. Note the "parents" are in the reverse of the usual tree structure.
    ///
    /// If this node is a leaf node, this function returns [`Vec::empty()`].
    pub fn raw_parents(&self) -> Vec<NodeRef<T>> {
        self.parents.to_vec()
    }

    // A trivial node is one whose parents are all incompressible (or a leaf)
    fn is_trivial(&self) -> bool {
        self.parents.iter().all(|p| {
            let p = p.read().expect("failed to acquire node read lock!");
            p.is_leaf() || !p.is_compressible()
        })
    }

    /// Compress the path from this node to the next incompressible layer of the DAG.
    /// Returns the parents of the node.
    ///
    /// After path compression, one of these three conditions holds:
    /// * This node is a leaf node;
    /// * This node has only incompressible parents, and keeps them;
    /// * This node has compressible parents, and after path compression, they are replaced by their closest incompressible ancestors.
    pub fn parents(&mut self) -> Vec<NodeRef<T>> {
        // Quick check to bail the trivial situations out in which:
        // * `self` is itself a leaf node;
        // * The parent nodes of `self` are all incompressible node.
        //
        // In any of the two cases above, we don't have to do anything.
        if self.is_trivial() {
            return self.raw_parents();
        }

        let mut res: Vec<NodeRef<T>> = Vec::new();
        // Do the path compression.
        let (compressible, incompressible): (Vec<NodeRef<T>>, Vec<NodeRef<T>>) =
            self.raw_parents().into_iter().partition(|p| {
                p.read()
                    .expect("failed to acquire read lock!")
                    .is_compressible()
            });

        res.extend(incompressible);
        // First, compress the path from the parent to some incompressible nodes. After this step, the parents of the
        // parent node should be incompressible.
        let new_parents: Vec<_> = compressible
            .par_iter()
            .flat_map_iter(|parent| {
                // there are no cycles!
                let these_new_parents: Vec<NodeRef<T>> = {
                    let mut parent = parent.write().expect("failed to acquire node write lock");

                    parent.parents()
                };

                // parent is compressed: it's now trivial
                debug_assert!(
                    parent
                        .read()
                        .expect("failed to acquire node read lock!")
                        .is_trivial(),
                    "{:?} is not trivial!",
                    parent.read().unwrap()
                );
                // we report its parents to the final parents result, enacting the path compression
                these_new_parents
            })
            .collect();
        res.extend(new_parents);

        let res = res.into_iter().unique_by(|arc| Arc::as_ptr(arc)).collect();
        self.parents = res;
        debug_assert!(self.is_trivial());
        self.raw_parents()
    }
}

pub fn bfs<T: Sync + Send + std::fmt::Debug>(
    initial: NodeRef<T>,
) -> impl Iterator<Item = NodeRef<T>> {
    bft::Bft::new(initial, |node| node.write().unwrap().parents().into_iter())
}

pub trait Affiliated: crypto::Hash {
    fn parents(&self) -> Vec<<Self as crypto::Hash>::TypedDigest>;
    fn compressible(&self) -> bool;
}
pub struct NodeDag<T: Affiliated> {
    // Not that we should need to ever serialize this (we'd rather rebuild the Dag from a persistent store)
    // but the way to serialize this in key order is using serde_with and an annotation of:
    // as = "FromInto<std::collections::BTreeMap<T::TypedDigest, Either<WeakNodeRef<T>, NodeRef<T>>>"
    node_table: DashMap<T::TypedDigest, Either<WeakNodeRef<T>, NodeRef<T>>>,
}

#[derive(Debug, Error)]
pub enum DagError<T: crypto::Hash> {
    #[error("No node known by this digest: {0}")]
    UnknownDigest(T::TypedDigest),
    #[error("The node known by this digest was dropped: {0}")]
    DroppedDigest(T::TypedDigest),
}

impl<T: Affiliated> NodeDag<T> {
    pub(crate) fn get_weak(&self, digest: T::TypedDigest) -> Result<WeakNodeRef<T>, DagError<T>> {
        let node_ref = self
            .node_table
            .get(&digest)
            .ok_or(DagError::UnknownDigest(digest))?;
        match *node_ref {
            Either::Left(ref node) => Ok(node.clone()),
            Either::Right(ref node) => Ok(Arc::downgrade(node)),
        }
    }

    pub fn get(&self, digest: T::TypedDigest) -> Result<NodeRef<T>, DagError<T>> {
        let node_ref = self
            .node_table
            .get(&digest)
            .ok_or(DagError::UnknownDigest(digest))?;
        match *node_ref {
            Either::Left(ref node) => Ok(node.upgrade().ok_or(DagError::DroppedDigest(digest))?),
            // the node is a head of the graph, just return
            Either::Right(ref node) => Ok(node.clone()),
        }
    }

    // Note: the dag currently does not do any causal completion, and assumes that the node is a head
    pub fn try_insert(&mut self, value: T) -> Result<(), DagError<T>> {
        let digest = value.digest();
        // Do we have this node already?
        if self.get_weak(digest).is_ok() {
            // idempotence (beware: re-adding removed nodes under the same hash won't bump the Rc)
            return Ok(());
        }
        let parent_digests = value.parents();
        let parents = parent_digests
            .iter()
            .map(|hash| self.get(*hash))
            .flat_map(|res| {
                match res {
                    Err(DagError::DroppedDigest(_)) => {
                        // TODO : log this properly! The parent is known, but was pruned in the past.
                        None
                    }
                    v => Some(v),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let compressible = value.compressible();

        let node = Node {
            parents,
            value,
            compressible,
        };
        let strong_node_ref = Arc::new(RwLock::new(node));
        self.node_table
            .insert(digest, Either::Right(strong_node_ref));
        // maintain the header invariant
        for mut parent in parent_digests
            .into_iter()
            .flat_map(|digest| self.node_table.get_mut(&digest))
        {
            if let Either::Right(strong_noderef) = &*parent {
                *parent = Either::Left(Arc::downgrade(strong_noderef));
            }
        }
        Ok(())
    }
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
            let parents = prior_round.iter().zip(picks).flat_map(|(parent, pick)| if pick { Some(parent.clone()) } else { None }).collect();
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
            assert!(dag.iter().all(|node| node.read().unwrap().height() <= 10));
            assert!(dag.iter().all(|node| node.read().unwrap().raw_parents().len() <= 10));
        }

        #[test]
        fn test_path_compression(
            dag in arb_dag_complete(10, 100)
        ) {
            let first = dag.first().unwrap();
            let initial_height = first.read().unwrap().height();
            first.write().expect("failed to acquire write lock").parents();
            let final_height = first.read().unwrap().height();
            assert!(final_height <= initial_height);
            assert!(first.read().unwrap().is_trivial())
        }

        #[test]
        fn test_path_compression_bfs(
            dag in arb_dag_complete(10, 100)
        ) {
            let first = dag.first().unwrap();
            let iter = bfs(first.clone());
            // The first nodemay end up compressible
            let mut is_first = true;
            for node_ref in iter {
                let node = node_ref.read().unwrap();
                if !is_first {
                assert!(node.is_leaf()|| !node.is_compressible())
                }
                is_first = false;
            }

        }

    }
}
