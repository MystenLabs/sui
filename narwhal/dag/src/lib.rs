use rayon::prelude::*;
use std::sync::{Arc, RwLock};

use itertools::Itertools;

pub type NodeRef<T> = Arc<RwLock<Node<T>>>;

impl<T> From<Node<T>> for NodeRef<T> {
    fn from(node: Node<T>) -> Self {
        Arc::new(RwLock::new(node))
    }
}

/// The DSU tree node.
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

    /// Get the parent nodes in a [`Vec`].
    ///
    /// If this node is a leaf node, this function returns [`Vec::empty()`].
    pub fn parents(&self) -> Vec<NodeRef<T>> {
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
    ///
    /// After path compression, one of these three conditions holds:
    /// * This node is a leaf node;
    /// * This node has only incompressible parents, and keeps them;
    /// * This node has compressible parents, and after path compression, they are replaced by their closest incompressible ancestors.
    pub fn compress_path(&mut self) {
        // Quick check to bail the trivial situations out in which:
        // * `self` is itself a leaf node;
        // * The parent nodes of `self` are all incompressible node.
        //
        // In any of the two cases above, we don't have to do anything.
        if self.is_trivial() {
            return;
        }

        let mut res: Vec<NodeRef<T>> = Vec::new();
        // Do the path compression.
        let (compressible, incompressible): (Vec<NodeRef<T>>, Vec<NodeRef<T>>) =
            self.parents().into_iter().partition(|p| {
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
                {
                    let mut parent = parent.write().expect("failed to acquire node write lock");

                    parent.compress_path();
                }

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
                parent
                    .read()
                    .expect("failed to acquire node read lock!")
                    .parents()
            })
            .collect();
        res.extend(new_parents);

        let res = res.into_iter().unique_by(|arc| Arc::as_ptr(arc)).collect();
        self.parents = res;
        debug_assert!(self.is_trivial())
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
            assert!(dag.iter().all(|node| node.read().unwrap().parents().len() <= 10));
        }

        #[test]
        fn test_path_compression(
            dag in arb_dag_complete(10, 100)
        ) {
            let first = dag.first().unwrap();
            let initial_height = first.read().unwrap().height();
            first.write().expect("failed to acquire write lock").compress_path();
            let final_height = first.read().unwrap().height();
            assert!(final_height <= initial_height);
            assert!(first.read().unwrap().is_trivial())
        }
    }
}
