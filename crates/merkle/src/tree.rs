use std::collections::{BTreeMap, HashMap};
use std::fmt;

use anyhow::{anyhow, Result};
use fastcrypto::encoding::{Base58, Encoding};
use fastcrypto::hash::HashFunction;
use serde::{Deserialize, Serialize};
use sui_types::{
    base_types::{ObjectID, ObjectRef},
    crypto::DefaultHash,
    digests::Digest,
};

use crate::nibble::{self, Nibble, NibblePath};

const CHILDREN_PER_NODE: usize = 16;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    children: [Child; CHILDREN_PER_NODE],
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub enum Child {
    #[default]
    None,
    Internal {
        digest: MerkleDigest,
        leaf_count: usize,
    },
    Leaf {
        object_ref: ObjectRef,
    },
    // InternalWithCommonPath {
    //     path: NibblePath,
    //     digest: MerkleDigest,
    //     leaf_count: usize,
    // },
    // TODO maybe have a collapsed node that has the common nibble path this could help situations
    // for leading zero ids 0x1,0x2, 0x5, etc although this may not help due to 0xdee9?
}

impl Child {
    fn is_none(&self) -> bool {
        match self {
            Child::None => true,
            Child::Internal { .. } | Child::Leaf { .. } => false,
        }
    }
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Hash, Serialize, Deserialize)]
pub struct MerkleDigest(Digest);

impl MerkleDigest {
    pub const ZERO: Self = Self(Digest::ZERO);

    pub const fn new(digest: [u8; 32]) -> Self {
        Self(Digest::new(digest))
    }

    pub fn random() -> Self {
        Self(Digest::random())
    }

    pub fn into_inner(self) -> [u8; 32] {
        self.0.into_inner()
    }
}

impl fmt::Debug for MerkleDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("MerkleDigest").field(&self.0).finish()
    }
}

impl fmt::Display for MerkleDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<[u8]> for MerkleDigest {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsRef<[u8; 32]> for MerkleDigest {
    fn as_ref(&self) -> &[u8; 32] {
        self.0.as_ref()
    }
}

impl std::str::FromStr for MerkleDigest {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut result = [0; 32];
        result.copy_from_slice(&Base58::decode(s).map_err(|e| anyhow::anyhow!(e))?);
        Ok(Self::new(result))
    }
}

impl Node {
    pub fn empty() -> Self {
        Self {
            children: Default::default(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.children.iter().all(Child::is_none)
    }

    pub fn child_count(&self) -> usize {
        self.children
            .iter()
            .map(|c| if c.is_none() { 0 } else { 1 })
            .sum()
    }

    pub fn leaf_count(&self) -> usize {
        self.children.iter().map(Child::leaf_count).sum()
    }

    fn first_child(&self) -> Child {
        self.children
            .iter()
            .find(|child| !child.is_none())
            .cloned()
            .unwrap()
    }

    //TODO have crypto audit this
    pub fn digest(&self) -> MerkleDigest {
        let mut digest = DefaultHash::default();
        bcs::serialize_into(&mut digest, self).expect("serialization should not fail");
        let hash = digest.finalize();
        MerkleDigest::new(hash.into())
    }

    pub fn into_iter(self) -> NodeIntoIter {
        NodeIntoIter {
            node: self,
            position: 0,
        }
    }
}

pub struct NodeIntoIter {
    node: Node,
    position: usize,
}

impl Iterator for NodeIntoIter {
    type Item = Child;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position < CHILDREN_PER_NODE {
            let ret = self.node.children[self.position].clone();
            self.position += 1;
            Some(ret)
        } else {
            None
        }
    }
}

impl Child {
    pub fn leaf_count(&self) -> usize {
        match self {
            Child::None => 0,
            Child::Internal { leaf_count, .. } => *leaf_count,
            Child::Leaf { .. } => 1,
        }
    }
}

pub trait TreeStore {
    fn get_node(&self, digest: MerkleDigest) -> anyhow::Result<Option<Node>>;
    fn write_node(&mut self, node: Node) -> anyhow::Result<()>;
}

#[derive(Debug)]
pub struct InMemoryStore {
    pub inner: HashMap<MerkleDigest, Node>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            inner: Default::default(),
        }
    }
}

impl TreeStore for InMemoryStore {
    fn get_node(&self, digest: MerkleDigest) -> anyhow::Result<Option<Node>> {
        Ok(self.inner.get(&digest).cloned())
    }

    fn write_node(&mut self, node: Node) -> anyhow::Result<()> {
        self.inner.insert(node.digest(), node);
        Ok(())
    }
}

pub struct MerkleTree<S> {
    store: S,
    root: Node,
}

impl<S: TreeStore> MerkleTree<S> {
    pub fn new(store: S) -> Self {
        Self {
            store,
            root: Node::empty(),
        }
    }

    pub fn with_root(store: S, root: MerkleDigest) -> Result<Self> {
        let root = store
            .get_node(root)?
            .ok_or_else(|| anyhow::anyhow!("missing {root}"))?;

        Ok(Self { store, root })
    }

    pub fn into_builder(self) -> MerkleTreeBuilder<S> {
        let mut nodes = BTreeMap::new();
        nodes.insert(
            NibblePath::empty(),
            NodeBuilder::from_node(self.root.clone()),
        );

        MerkleTreeBuilder {
            store: self.store,
            nodes,
        }
    }

    pub fn root(&self) -> &Node {
        &self.root
    }

    pub fn into_iter(self) -> ObjectRefIter<S> {
        ObjectRefIter::new(self.store, self.root)
    }
}

// struct NodeIter<S> {
//     store: S,
//     current: Option<NodeIntoIter>,
//     stack: Vec<NodeIntoIter>,
// }

pub struct ObjectRefIter<S> {
    store: S,
    current: Option<NodeIntoIter>,
    stack: Vec<NodeIntoIter>,
}

impl<S> ObjectRefIter<S> {
    fn new(store: S, root: Node) -> Self {
        Self {
            store,
            current: Some(root.into_iter()),
            stack: vec![],
        }
    }
}

impl<S: TreeStore> Iterator for ObjectRefIter<S> {
    type Item = Result<ObjectRef>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current) = &mut self.current {
                match current.next() {
                    Some(Child::None) => continue,
                    Some(Child::Internal { digest, .. }) => {
                        let node = match self.store.get_node(digest) {
                            Ok(Some(node)) => node,
                            Ok(None) => return Some(Err(anyhow!("missing data"))),
                            Err(e) => return Some(Err(e)),
                        };
                        let prev = self.current.replace(node.into_iter());
                        self.stack.push(prev.unwrap());
                    }
                    Some(Child::Leaf { object_ref }) => return Some(Ok(object_ref)),
                    None => {
                        self.current = self.stack.pop();
                    }
                }
            } else {
                return None;
            }
        }
    }
}

pub struct MerkleTreeBuilder<S> {
    store: S,
    nodes: BTreeMap<NibblePath, NodeBuilder>,
}

#[derive(Clone)]
struct NodeBuilder {
    children: [Option<Child>; 16],
}

impl NodeBuilder {
    fn new() -> Self {
        Self::from_node(Node::empty())
    }

    fn from_node(node: Node) -> Self {
        let children = node.children.map(Some);
        Self { children }
    }
}

impl<S: TreeStore> MerkleTreeBuilder<S> {
    pub fn insert(&mut self, object_ref: ObjectRef) -> Result<()> {
        let path = NibblePath::new_even(object_ref.0);
        let mut iter = path.nibbles();
        let mut traversed_path = NibblePath::empty();

        loop {
            let curr = self.nodes.get_mut(&traversed_path).unwrap();
            let nibble = iter.next().unwrap();
            traversed_path.push(nibble);
            let child = &mut curr.children[nibble.inner() as usize];
            match child {
                Some(Child::None) => {
                    *child = Some(Child::Leaf { object_ref });
                    break;
                }
                Some(Child::Leaf {
                    object_ref: child_object_ref,
                }) => {
                    if object_ref.0 == child_object_ref.0 {
                        assert!(object_ref.1 > child_object_ref.1);
                        *child = Some(Child::Leaf { object_ref });
                        break;
                    }

                    let mut next = NodeBuilder::new();

                    let mut traversed_iter = traversed_path.nibbles();
                    let child_path = NibblePath::new_even(child_object_ref.0);
                    let mut child_iter = child_path.nibbles();
                    nibble::skip_common_prefix(&mut traversed_iter, &mut child_iter);
                    let next_child_nibble = child_iter.next().unwrap();
                    next.children[next_child_nibble.inner() as usize] = child.clone();

                    *child = None;
                    assert!(self.nodes.insert(traversed_path.clone(), next).is_none());
                }
                Some(Child::Internal { digest, .. }) => {
                    let next = NodeBuilder::from_node(
                        self.store.get_node(*digest)?.expect("missing node"),
                    );

                    *child = None;
                    assert!(self.nodes.insert(traversed_path.clone(), next).is_none());
                }
                None => continue,
            }
        }

        Ok(())
    }

    pub fn remove(&mut self, object_id: ObjectID) -> Result<()> {
        let path = NibblePath::new_even(object_id);
        let mut iter = path.nibbles();
        let mut traversed_path = NibblePath::empty();

        loop {
            let curr = self.nodes.get_mut(&traversed_path).unwrap();
            let nibble = iter.next().unwrap();
            traversed_path.push(nibble);
            let child = &mut curr.children[nibble.inner() as usize];
            match child {
                Some(Child::None) => {
                    break;
                }
                Some(Child::Leaf {
                    object_ref: child_object_ref,
                }) => {
                    if object_id == child_object_ref.0 {
                        *child = Some(Child::None);
                    }

                    break;
                }
                Some(Child::Internal { digest, .. }) => {
                    let next = NodeBuilder::from_node(
                        self.store.get_node(*digest)?.expect("missing node"),
                    );

                    *child = None;
                    assert!(self.nodes.insert(traversed_path.clone(), next).is_none());
                }
                None => continue,
            }
        }

        Ok(())
    }

    pub fn build(mut self) -> Result<MerkleTree<S>> {
        let mut complete: HashMap<NibblePath, Node> = HashMap::new();
        for (path, node_builder) in self.nodes.into_iter().rev() {
            let mut node = Node::empty();

            for (i, child) in node_builder.children.into_iter().enumerate() {
                let child = match child {
                    Some(child) => child,
                    None => {
                        let nibble = Nibble::from(i as u8);
                        let mut path = path.clone();
                        path.push(nibble);
                        let node = complete.get(&path).unwrap();
                        if node.is_empty() {
                            complete.remove(&path);
                            Child::None
                        } else {
                            let leaf_count = node.leaf_count();
                            if leaf_count == 1 {
                                let child = node.first_child();
                                complete.remove(&path);
                                child
                            } else {
                                let digest = node.digest();
                                Child::Internal { digest, leaf_count }
                            }
                        }
                    }
                };
                node.children[i] = child;
            }
            complete.insert(path, node);
        }

        let root = complete.get(&NibblePath::empty()).cloned().unwrap();
        for node in complete.into_values() {
            self.store.write_node(node)?;
        }

        Ok(MerkleTree {
            store: self.store,
            root,
        })
    }
}

#[cfg(test)]
mod tests {
    use sui_types::{base_types::SequenceNumber, digests::ObjectDigest};

    use super::*;

    #[test]
    fn insert() {
        let store = InMemoryStore {
            inner: Default::default(),
        };

        let mut ids = BTreeMap::new();
        let mut builder = MerkleTree::new(store).into_builder();

        for _ in 0..10000 {
            let object_ref = sui_types::base_types::random_object_ref();
            builder.insert(object_ref);
            ids.insert(object_ref.0, object_ref);
        }

        let tree = builder.build().unwrap();

        assert_eq!(tree.root.leaf_count(), ids.len());

        for (actual, expected) in tree.into_iter().zip(ids.into_values()) {
            let actual = actual.unwrap();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn remove() {
        let store = InMemoryStore {
            inner: Default::default(),
        };

        let mut ids = BTreeMap::new();
        let mut builder = MerkleTree::new(store).into_builder();

        for _ in 0..10000 {
            let object_ref = sui_types::base_types::random_object_ref();
            builder.insert(object_ref);
            ids.insert(object_ref.0, object_ref);
        }

        let tree = builder.build().unwrap();

        let mut builder = tree.into_builder();

        let mut to_remove = BTreeMap::new();
        let mut to_keep = BTreeMap::new();

        for (i, (k, v)) in ids.into_iter().enumerate() {
            if i % 2 == 0 {
                to_remove.insert(k, v);
            } else {
                to_keep.insert(k, v);
            }
        }

        for id in to_remove.keys() {
            builder.remove(*id);
        }

        let tree = builder.build().unwrap();

        assert_eq!(tree.root.leaf_count(), to_keep.len());

        for (actual, expected) in tree.into_iter().zip(to_keep.into_values()) {
            let actual = actual.unwrap();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn add_and_remove() {
        let store = InMemoryStore {
            inner: Default::default(),
        };

        let mut ids = BTreeMap::new();
        let mut builder = MerkleTree::new(store).into_builder();

        for _ in 0..10000 {
            let object_ref = sui_types::base_types::random_object_ref();
            builder.insert(object_ref);
            ids.insert(object_ref.0, object_ref);
        }

        let tree = builder.build().unwrap();

        let mut builder = tree.into_builder();

        let mut to_remove = BTreeMap::new();
        let mut to_keep = BTreeMap::new();

        for (i, (k, v)) in ids.into_iter().enumerate() {
            if i % 2 == 0 {
                to_remove.insert(k, v);
            } else {
                to_keep.insert(k, v);
            }
        }

        for id in to_keep.values_mut() {
            id.1.increment();
            builder.insert(*id);
        }

        for id in to_remove.keys() {
            builder.remove(*id);
        }

        for _ in 0..10000 {
            let object_ref = sui_types::base_types::random_object_ref();
            builder.insert(object_ref);
            to_keep.insert(object_ref.0, object_ref);
        }

        let tree = builder.build().unwrap();

        assert_eq!(tree.root.leaf_count(), to_keep.len());

        for (actual, expected) in tree.into_iter().zip(to_keep.into_values()) {
            let actual = actual.unwrap();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn long_branch() {
        let store = InMemoryStore {
            inner: Default::default(),
        };

        let mut builder = MerkleTree::new(store).into_builder();

        let o1 = (
            ObjectID::ZERO,
            SequenceNumber::new(),
            ObjectDigest::new([0; 32]),
        );

        let o2 = (
            {
                let mut bytes = ObjectID::ZERO.into_bytes();
                bytes[31] = 1;
                ObjectID::new(bytes)
            },
            SequenceNumber::new(),
            ObjectDigest::new([0; 32]),
        );
        builder.insert(o1);
        builder.insert(o2);

        let tree = builder.build().unwrap();

        println!("{}", tree.store.inner.len());
        println!("{:#?}", tree.store);

        let mut builder = tree.into_builder();

        builder.remove(o1.0);
        let tree = builder.build().unwrap();
        println!("{:#?}", tree.root);

        let mut builder = tree.into_builder();

        builder.remove(o2.0);
        let tree = builder.build().unwrap();
        println!("{:#?}", tree.root);
    }
}
