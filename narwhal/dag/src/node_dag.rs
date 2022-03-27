// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use dashmap::DashMap;
use either::Either;
use std::sync::{Arc, RwLock};
use thiserror::Error;

use super::{Node, NodeRef, WeakNodeRef};

/// A trait marking the minimum information we need to sort out the value for a node:
/// - `parents`: hash pointers to its parents, and
/// - `compressible`: the inital value of whether it's compressible
/// The `crypto:Hash` trait bound offers the digest-ibility.
pub trait Affiliated: crypto::Hash {
    fn parents(&self) -> Vec<<Self as crypto::Hash>::TypedDigest>;
    fn compressible(&self) -> bool;
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
    pub fn new() -> NodeDag<T> {
        NodeDag {
            node_table: DashMap::new(),
        }
    }

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

    // Note: the dag currently does not do any causal completion, and maintains that
    // - insertion should be idempotent
    // - an unseen node is a head (not pointed) to by any other node.
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
        // important: do this first, before downgrading the head references
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

impl<T: Affiliated> Default for NodeDag<T> {
    fn default() -> Self {
        Self::new()
    }
}
