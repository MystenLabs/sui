// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use arc_swap::ArcSwap;
use crypto::Digest;
use dashmap::DashMap;
use either::Either;
use once_cell::sync::OnceCell;
use std::sync::Arc;
use thiserror::Error;

use super::{Node, NodeRef, WeakNodeRef};

/// A trait marking the minimum information we need to sort out the value for a node:
/// - `parents`: hash pointers to its parents, and
/// - `compressible`: a value-derived boolean indicating if that value is, initially, compressible
///
/// The `crypto:Hash` trait bound offers the digest-ibility.
pub trait Affiliated: crypto::Hash {
    /// Hash pointers to the parents of the current value
    fn parents(&self) -> Vec<<Self as crypto::Hash>::TypedDigest>;

    /// Whether the current value should be marked as compressible when first inserted in a Node.
    /// Defaults to a blanket false for all values.
    fn compressible(&self) -> bool {
        false
    }
}
/// The Dag data structure
/// This consists morally of two tables folded in one:
/// - the node table, which contains mappings from node hashes to weak references,
///   maintaining which nodes were historically processed by the graph,
/// - the heads of the graph (aka the roots): those nodes which do not have antecedents in the graph,
///   and are holding transitive references to all the other nodes.
///
/// Those two tables are coalesced into one, which value type is either a weak reference (regular node) or a strong one (heads).
/// During the normal processing of the graph, heads which gain antecedents lost their head status and become regular nodes.
/// Moreover, the transitive references to nodes in the graph may disappear because of its changing topology (see above: path compression).
/// In this case, the weak references may be invalidate. We do not remove them from the graph, and their presence serves as a "tombstone".
///
/// /!\ Warning /!\: do not drop the heads of the graph without having given them new antecedents,
/// as this will transitively drop all the nodes they point to and may cause loss of data.
///   
#[derive(Debug)]
pub struct NodeDag<T: Affiliated> {
    // Not that we should need to ever serialize this (we'd rather rebuild the Dag from a persistent store)
    // but the way to serialize this in key order is using serde_with and an annotation of:
    // as = "FromInto<std::collections::BTreeMap<T::TypedDigest, Either<WeakNodeRef<T>, NodeRef<T>>>"
    node_table: DashMap<T::TypedDigest, Either<WeakNodeRef<T>, NodeRef<T>>>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum NodeDagError {
    #[error("No vertex known by these digests: {0:?}")]
    UnknownDigests(Vec<Digest>),
    #[error("The vertex known by this digest was dropped: {0}")]
    DroppedDigest(Digest),
}

impl<T: Affiliated> NodeDag<T> {
    /// Creates a new Node dag
    pub fn new() -> NodeDag<T> {
        NodeDag {
            node_table: DashMap::new(),
        }
    }

    /// Returns a weak reference to the requested vertex.
    /// This does not prevent the vertex from being dropped off the graph.
    pub fn get_weak(&self, digest: T::TypedDigest) -> Result<WeakNodeRef<T>, NodeDagError> {
        let node_ref = self
            .node_table
            .get(&digest)
            .ok_or_else(|| NodeDagError::UnknownDigests(vec![digest.into()]))?;
        match *node_ref {
            Either::Left(ref node) => Ok(node.clone()),
            Either::Right(ref node) => Ok(Arc::downgrade(node)),
        }
    }

    // Returns a strong (`Arc`) reference to the graph node.
    // This bumps the reference count to the vertex and may prevent it from being GC-ed off the graph.
    // This is not publicly accessible, so as to not let wandering references to DAG nodes prevent GC logic
    pub fn get(&self, digest: T::TypedDigest) -> Result<NodeRef<T>, NodeDagError> {
        let node_ref = self
            .node_table
            .get(&digest)
            .ok_or_else(|| NodeDagError::UnknownDigests(vec![digest.into()]))?;
        match *node_ref {
            Either::Left(ref node) => {
                Ok(NodeRef(node.upgrade().ok_or_else(|| {
                    NodeDagError::DroppedDigest(digest.into())
                })?))
            }
            // the node is a head of the graph, just return
            Either::Right(ref node) => Ok(node.clone()),
        }
    }

    /// Returns whether the vertex pointed to by the hash passed as an argument was
    /// contained in the DAG at any point in the past.
    pub fn contains(&self, hash: T::TypedDigest) -> bool {
        self.node_table.contains_key(&hash)
    }

    /// Returns whether the vertex pointed to by the hash passed as an argument is
    /// contained in the DAG and still a live (uncompressed) reference.
    pub fn contains_live(&self, digest: T::TypedDigest) -> bool {
        self.get(digest).is_ok()
    }

    /// Returns an iterator over the digests of the heads of the graph, i.e. the nodes which do not have a child.
    pub fn head_digests(&self) -> impl Iterator<Item = T::TypedDigest> + '_ {
        self.node_table
            .iter()
            .flat_map(|node_ref| node_ref.as_ref().right().map(|node| node.value().digest()))
    }

    /// Returns whether the vertex pointed to by the hash passed as an argument is a
    /// head of the DAG (nodes not pointed to by any other). Heads carry strong references
    /// to many downward nodes and dropping them might GC large spans of the graph.
    ///
    /// This returns an error if the queried node is unknown
    pub fn has_head(&self, hash: T::TypedDigest) -> Result<bool, NodeDagError> {
        let node_ref = self
            .node_table
            .get(&hash)
            .ok_or_else(|| NodeDagError::UnknownDigests(vec![hash.into()]))?;
        match *node_ref {
            Either::Right(ref _node) => Ok(true),
            Either::Left(ref _node) => Ok(false),
        }
    }

    /// Marks the node passed as argument as compressible, leaving it to be reaped by path compression.
    /// Returns true if the node was made compressible, and false if it already was
    ///
    /// This return an error if the queried node is unknown or dropped from the graph
    pub fn make_compressible(&self, hash: T::TypedDigest) -> Result<bool, NodeDagError> {
        let node_ref = self.get(hash)?;
        Ok(node_ref.make_compressible())
    }

    /// Inserts a node in the Dag from the provided value
    ///
    /// When the value is inserted, its parent references are interpreted as hash pointers (see [`Affiliated`])`.
    /// Those hash pointers are converted to [`NodeRef`] based on the pointed nodes that are already in the DAG.
    ///
    /// Note: the dag currently does not do any causal completion. It is an error to insert a node which parents
    /// are unknown by the DAG it's inserted into.
    ///
    /// This insertion procedure only maintains the invariant that
    /// - insertion should be idempotent
    /// - an unseen node is a head (not pointed) to by any other node.
    ///
    pub fn try_insert(&mut self, value: T) -> Result<(), NodeDagError> {
        let digest = value.digest();
        // Do we have this node already?
        if self.contains(digest) {
            // idempotence (beware: re-adding removed nodes under the same hash won't bump the Rc)
            return Ok(());
        }
        let parent_digests = value.parents();
        let parents = parent_digests
            .iter()
            .map(|hash| self.get(*hash))
            // We use Either::Left to collect parent refs, Either::Right to collect missing parents in case we encounter a failure
            .fold(Either::Left(Vec::new()), |acc, res| {
                match (acc, res) {
                    // This node was previously dropped, continue
                    (acc, Err(NodeDagError::DroppedDigest(_))) => {
                        // TODO : log this properly! The parent is known, but was pruned in the past.
                        acc
                    }
                    // Found a parent with no errors met, collect
                    (Either::Left(mut v), Ok(parent_ref)) => {
                        v.push(parent_ref);
                        Either::Left(v)
                    }
                    // We meet our first error! Switch to collecting digests of missing parents
                    (Either::Left(_), Err(NodeDagError::UnknownDigests(digest_vec))) => {
                        Either::Right(digest_vec)
                    }
                    // Found a parent while we know some are missing, ignore
                    (acc @ Either::Right(_), Ok(_)) => acc,
                    // Another missing parent while we know some are missing, collect
                    (Either::Right(mut v), Err(NodeDagError::UnknownDigests(digest_vec))) => {
                        v.extend(digest_vec);
                        Either::Right(v)
                    }
                }
            });
        let parents = match parents {
            Either::Right(missing_parents) => {
                return Err(NodeDagError::UnknownDigests(missing_parents))
            }
            Either::Left(found_parents) => found_parents,
        };

        let compressible: OnceCell<()> = {
            let cell = OnceCell::new();
            if value.compressible() {
                let _ = cell.set(());
            }
            cell
        };

        let node = Node {
            parents: ArcSwap::from_pointee(parents),
            value,
            compressible,
        };
        let strong_node_ref = NodeRef::from_pointee(node);
        // important: do this first, before downgrading the head references
        self.node_table
            .insert(digest, Either::Right(strong_node_ref));
        // maintain the head invariant: the node table should no longer have a strong reference to the head
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

impl<T: Affiliated + Sync + Send + std::fmt::Debug> NodeDag<T> {
    /// Performs a breadth-first traversal of the Dag starting at the given vertex
    pub fn bft(
        &self,
        hash: T::TypedDigest,
    ) -> Result<impl Iterator<Item = NodeRef<T>>, NodeDagError> {
        let start = self.get(hash)?;
        Ok(crate::bfs(start))
    }

    /// Returns the number of elements (nodes) stored
    pub fn size(&self) -> usize {
        self.node_table.len()
    }
}

impl<T: Affiliated> Default for NodeDag<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, fmt};

    use crypto::{Digest, Hash};
    use proptest::prelude::*;

    use super::*;

    #[derive(Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Copy)]
    pub struct TestDigest([u8; crypto::DIGEST_LEN]);

    impl From<TestDigest> for Digest {
        fn from(hd: TestDigest) -> Self {
            Digest::new(hd.0)
        }
    }

    impl fmt::Debug for TestDigest {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
            write!(f, "{}", hex::encode(&self.0).get(0..16).unwrap())
        }
    }

    impl fmt::Display for TestDigest {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
            write!(f, "{}", hex::encode(&self.0).get(0..16).unwrap())
        }
    }

    #[derive(Debug, Clone)]
    pub struct TestNode {
        parents: Vec<TestDigest>,
        compressible: bool,
        digest: TestDigest,
    }

    impl crypto::Hash for TestNode {
        type TypedDigest = TestDigest;

        fn digest(&self) -> Self::TypedDigest {
            self.digest
        }
    }

    impl Affiliated for TestNode {
        fn parents(&self) -> Vec<<Self as crypto::Hash>::TypedDigest> {
            self.parents.clone()
        }

        fn compressible(&self) -> bool {
            self.compressible
        }
    }

    prop_compose! {
        pub fn arb_test_digest()(
            hash in prop::collection::vec(any::<u8>(), crypto::DIGEST_LEN..=crypto::DIGEST_LEN),
        ) -> TestDigest {
            TestDigest(hash.try_into().unwrap())
        }
    }

    prop_compose! {
        pub fn arb_leaf_node()(
            digest in arb_test_digest(),
            compressible in any::<bool>(),
        ) -> TestNode {
            TestNode {
                parents: Vec::new(),
                digest,
                compressible
            }
        }
    }

    prop_compose! {
        pub fn arb_inner_node(pot_parents: Vec<TestDigest>)(
            // this is a 50% inclusion rate, in production we'd shoot for > 67%
            picks in prop::collection::vec(any::<bool>(), pot_parents.len()..=pot_parents.len()),
            digest in arb_test_digest(),
            compressible in any::<bool>(),
        ) -> TestNode {
            let parents = pot_parents.iter().zip(picks).flat_map(|(parent, pick)| pick.then_some(*parent)).collect();
            TestNode{
                parents,
                compressible,
                digest
            }
        }
    }

    prop_compose! {
        pub fn next_round(prior_round: Vec<TestNode>)(
            nodes in {
                let n = prior_round.len();
                let digests: Vec<_> = prior_round.iter().map(|n| n.digest()).collect();
                prop::collection::vec(arb_inner_node(digests), n..=n)
            }
        ) -> Vec<TestNode> {
            let mut res = prior_round.clone();
            res.extend(nodes);
            res
        }
    }

    pub fn arb_dag_complete(breadth: usize, rounds: usize) -> impl Strategy<Value = Vec<TestNode>> {
        let initial_round = prop::collection::vec(arb_leaf_node().no_shrink(), breadth..=breadth);

        initial_round.prop_recursive(
            rounds as u32,             // max rounds level deep
            (breadth * rounds) as u32, // max branching total
            breadth as u32,            // branches  per round
            move |inner| inner.prop_flat_map(next_round),
        )
    }

    proptest! {
        #[test]
        fn test_insert_missing(
            digests in prop::collection::vec(arb_test_digest(), 0..10),
            // Note random_parents must be non-empty, or the insertion will succeed
            random_parents in prop::collection::vec(arb_test_digest(), 1..10)
        ) {
            let nodes = digests.into_iter().map(|digest| {

                TestNode{
                    digest,
                    parents: random_parents.clone(),
                    compressible: false
                }
            });
            let mut nu_dag = NodeDag::new();
            let random_parents_digests: Vec<Digest> = random_parents.iter().map(|digest| (*digest).into()).collect();
            let expected_error = NodeDagError::UnknownDigests(random_parents_digests);
            for node in nodes {
                assert_eq!(expected_error, nu_dag.try_insert(node).err().unwrap())
            }
        }

        #[test]
        fn test_dag_sanity_check(
            dag in arb_dag_complete(10, 10)
        ) {
            // the `prop_recursive` combinator used in `arb_dag_complete` is actually probabilistic, see:
            // https://github.com/AltSysrq/proptest/blob/master/proptest/src/strategy/recursive.rs#L83-L110
            // so we can't test for our desired size here (100), we rather test for something that will pass
            // with overwhelming probability
            assert!(dag.len() <= 200);
        }

        #[test]
        fn test_dag_insert_sanity_check(
            dag in arb_dag_complete(10, 10)
        ) {
            let mut node_dag = NodeDag::new();
            for node in dag.clone().into_iter() {
                // the elements are generated in order & with no missing parents => no surprises
                assert!(node_dag.try_insert(node).is_ok());
            }
            for ref_multi in node_dag.node_table.iter() {
                // no dangling reference (we haven't removed anything yet, and the parenthood coverage is exhaustive)
                match ref_multi.value() {
                    Either::Right(_) => (),
                    Either::Left(ref node) => assert!(node.upgrade().is_some()),
                }
            }

            assert_eq!(node_dag.size(), dag.len());
        }


        #[test]
        fn test_dag_contains_heads(
            dag in arb_dag_complete(10, 10)
        ) {
            let mut node_dag = NodeDag::new();
            let mut digests = Vec::new();
            for node in dag.iter() {
                digests.push(node.digest());
                // the elements are generated in order & with no missing parents => no suprises
                assert!(node_dag.try_insert(node.clone()).is_ok());
            }
            let mut heads = HashSet::new();
            for hash in digests {
                // all insertions are reflected
                assert!(node_dag.contains(hash));
                if node_dag.has_head(hash).unwrap() {
                    heads.insert(hash);
                }
            }
            // at least the last round has nothing pointing to themselves
            assert!(heads.len() >= 10);
            // check heads have nothing pointing to them
            for node in dag.into_iter() {
                assert!(node.parents().iter().all(|parent| !heads.contains(parent)))
            }
        }

        #[test]
        fn test_dag_head_digests(
            dag in arb_dag_complete(10, 10)
        ) {
            let mut node_dag = NodeDag::new();
            let mut digests = Vec::new();
            for node in dag.iter() {
                digests.push(node.digest());
                // the elements are generated in order & with no missing parents => no suprises
                assert!(node_dag.try_insert(node.clone()).is_ok());
            }
            let mut heads = HashSet::new();
            for hash in digests {
                // all insertions are reflected
                assert!(node_dag.contains(hash));
                if node_dag.has_head(hash).unwrap() {
                    heads.insert(hash);
                }
            }
            // check this matches head_digests
            for head_digest in node_dag.head_digests() {
                assert!(heads.contains(&head_digest));
            }
        }

        #[test]
        fn test_path_compression_from_dag(
            dag in arb_dag_complete(10, 10)
        ) {
            let mut node_dag = NodeDag::new();
            let mut compressibles = Vec::new();
            let mut digests = Vec::new();
            {
                for node in dag.iter() {
                    digests.push(node.digest());
                    if node.compressible(){
                        compressibles.push(node.digest());
                    }
                    // the elements are generated in order & with no missing parents => no suprises
                    assert!(node_dag.try_insert(node.clone()).is_ok());
                }
            }
            // the chance of this happening is (1/2)^90
            prop_assume!(!compressibles.is_empty());

            let mut heads = HashSet::new();
            for hash in digests {
                if node_dag.has_head(hash).unwrap() {
                    heads.insert(hash);

                    let node = node_dag.get(hash).unwrap(); // strong reference
                    crate::bfs(node).for_each(|_node| ()); // path compression
                }
            }
            // now we've done a graph walk from every head => everything is compressed, save for the heads themselves
            for compressed_node in compressibles {
                if !heads.contains(&compressed_node) {
                    assert!(
                        matches!(node_dag.get(compressed_node), Err(NodeDagError::DroppedDigest(_))),
                        "node {compressed_node} should have been compressed yet is still present"
                    );
                }
            }
        }

        #[test]
        fn test_compress_all_the_things(
            dag in arb_dag_complete(10, 10)
        ) {
            let mut node_dag = NodeDag::new();
            let mut digests = Vec::new();
            {
                for node in dag.iter() {
                    digests.push(node.digest());
                    // the elements are generated in order & with no missing parents => no suprises
                    assert!(node_dag.try_insert(node.clone()).is_ok());
                }
            }
            // make everything compressible
            for hash in digests.clone() {
                node_dag.make_compressible(hash).unwrap();
            }

            let mut heads = HashSet::new();
            for hash in digests.clone() {
                if node_dag.has_head(hash).unwrap() {
                    heads.insert(hash);

                    let node = node_dag.get(hash).unwrap(); // strong reference
                    crate::bfs(node).for_each(|_node| ()); // path compression
                }
            }
            // now we've done a graph walk from every head => everything is compressed,
            // and since everything is compressible, all that's left is the heads
            for digest in digests {
                if !heads.contains(&digest){
                    assert!(matches!(node_dag.get(digest), Err(NodeDagError::DroppedDigest(_))))
                }
            }
        }
    }
}
