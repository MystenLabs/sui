// Copyright (c) Walrus Foundation
// SPDX-License-Identifier: Apache-2.0

//! Merkle tree implementation for Walrus.
use core::{fmt::Debug, marker::PhantomData};

use fastcrypto::hash::{Blake2b256, Digest, HashFunction};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::Level;

/// The length of the digests used in the merkle tree.
pub const DIGEST_LEN: usize = 32;

const LEAF_PREFIX: [u8; 1] = [0];
const INNER_PREFIX: [u8; 1] = [1];
const EMPTY_NODE: [u8; DIGEST_LEN] = [0; DIGEST_LEN];

/// Returned if the specified index is out of bounds for a Merkle tree or proof.
#[derive(Error, Debug, PartialEq, Eq)]
#[error("Leaf index {0} exceeds tree bounds")]
pub struct LeafIndexOutOfBounds(usize);

/// A node in the Merkle tree.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize, Serialize)]
pub enum Node {
    /// A node with an empty subtree.
    Empty,
    /// A node with children, hash value of length 32 bytes.
    Digest([u8; DIGEST_LEN]),
}

impl Node {
    /// Get the byte representation of [`self`].
    pub fn bytes(&self) -> [u8; DIGEST_LEN] {
        match self {
            Node::Empty => EMPTY_NODE,
            Node::Digest(val) => *val,
        }
    }
}

impl From<Digest<DIGEST_LEN>> for Node {
    fn from(value: Digest<DIGEST_LEN>) -> Self {
        Self::Digest(value.digest)
    }
}

impl From<[u8; DIGEST_LEN]> for Node {
    fn from(value: [u8; DIGEST_LEN]) -> Self {
        Self::Digest(value)
    }
}

impl AsRef<[u8]> for Node {
    fn as_ref(&self) -> &[u8] {
        match self {
            Node::Empty => EMPTY_NODE.as_ref(),
            Node::Digest(val) => val.as_ref(),
        }
    }
}

/// The operations required to authenticate a Merkle proof.
pub trait MerkleAuth: Clone + std::fmt::Debug {
    /// Verifies the proof given a Merkle root and the leaf data.
    #[tracing::instrument(level = Level::DEBUG, skip(self, leaf))]
    fn verify_proof(&self, root: &Node, leaf: &[u8], leaf_index: usize) -> bool {
        self.compute_root(leaf, leaf_index).as_ref() == Some(root)
    }

    /// Recomputes the Merkle root from the proof and the provided leaf data.
    ///
    /// Returns `None` if the provided index is too large.
    fn compute_root(&self, leaf: &[u8], leaf_index: usize) -> Option<Node>;
}

/// A proof that some data is at index `leaf_index` in a [`MerkleTree`].
#[derive(Serialize, Deserialize)]
pub struct MerkleProof<T = Blake2b256> {
    _hash_type: PhantomData<T>,
    /// The sibling hash values on the path from the leaf to the root.
    path: Vec<Node>,
}

impl<T> MerkleProof<T>
where
    T: HashFunction<DIGEST_LEN>,
{
    /// Construct Merkle proof from list of hashes and leaf index.
    pub fn new(path: &[Node]) -> Self {
        Self {
            _hash_type: PhantomData,
            path: path.into(),
        }
    }

    // Check if the proof is for the rightmost leaf in the tree
    pub fn is_right_most(&self, leaf_index: usize) -> bool {
        let mut level_index = leaf_index;
        for sibling in self.path.iter() {
            // The sibling hash of the current node
            if level_index % 2 == 0 {
                // The current node is a left child
                if sibling.as_ref() != EMPTY_NODE.as_ref() {
                    return false;
                }
            }
            // Update to the level index one level up in the tree
            level_index /= 2;
        }
        true
    }
}

// Cannot be derived as many hash functions don't implement `Clone` and the derive is not smart
// enough to see that it is not necessary.
impl<T> Clone for MerkleProof<T> {
    fn clone(&self) -> Self {
        Self {
            _hash_type: PhantomData,
            path: self.path.clone(),
        }
    }
}

// Cannot be derived as many hash functions don't implement `Debug` and the derive is not smart
// enough to see that it is not necessary.
impl<T> core::fmt::Debug for MerkleProof<T> {
    fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        fmt.debug_struct(&format!("MerkleProof<{}>", core::any::type_name::<T>()))
            .field("path", &self.path)
            .finish()
    }
}

impl<T> MerkleAuth for MerkleProof<T>
where
    T: HashFunction<DIGEST_LEN>,
{
    fn compute_root(&self, leaf: &[u8], leaf_index: usize) -> Option<Node> {
        if leaf_index >> self.path.len() != 0 {
            return None;
        }
        let mut current_hash = leaf_hash::<T>(leaf);
        let mut level_index = leaf_index;
        for sibling in self.path.iter() {
            // The sibling hash of the current node
            if level_index % 2 == 0 {
                // The current node is a left child
                current_hash = inner_hash::<T>(&current_hash, sibling);
            } else {
                // The current node is a right child
                current_hash = inner_hash::<T>(sibling, &current_hash);
            };
            // Update to the level index one level up in the tree
            level_index /= 2;
        }
        Some(current_hash)
    }
}

impl<T> PartialEq for MerkleProof<T> {
    fn eq(&self, other: &Self) -> bool {
        self.path.eq(&other.path)
    }
}

impl Eq for MerkleProof {}

/// A proof that some data is not in a Merkle tree.
#[derive(Serialize, Deserialize)]
pub struct MerkleNonInclusionProof<L, T = Blake2b256>
where
    L: Ord + AsRef<[u8]>, // TODO: Also need PartialEq?
{
    _hash_type: PhantomData<T>,
    pub index: usize,
    pub left_leaf: Option<(L, MerkleProof<T>)>,
    pub right_leaf: Option<(L, MerkleProof<T>)>,
}

impl<L, T> core::fmt::Debug for MerkleNonInclusionProof<L, T>
where
    L: Debug + Ord + AsRef<[u8]>,
    T: HashFunction<DIGEST_LEN>,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct(&format!(
            "MerkleNonInclusionProof<L={}, T={}>",
            std::any::type_name::<L>(),
            std::any::type_name::<T>()
        ))
        .field("left_leaf", &self.left_leaf)
        .field("right_leaf", &self.right_leaf)
        .finish()
    }
}

impl<L, T> MerkleNonInclusionProof<L, T>
where
    T: HashFunction<DIGEST_LEN>,
    L: Ord + AsRef<[u8]>,
{
    pub fn new(
        left_leaf: Option<(L, MerkleProof<T>)>,
        right_leaf: Option<(L, MerkleProof<T>)>,
        index: usize,
    ) -> Self {
        Self {
            _hash_type: PhantomData,
            left_leaf,
            right_leaf,
            index,
        }
    }

    fn is_valid_neighbor(
        &self,
        neighbor: &L,
        proof: &MerkleProof<T>,
        neighbor_index: usize,
        root: &Node,
    ) -> bool {
        proof.verify_proof(root, neighbor.as_ref(), neighbor_index)
    }

    // Prove non-inclusion of target_leaf in a Merkle tree assuming that the tree is sorted.
    // Edge case explanations
    // - Empty tree: no leaves, automatically valid
    // - Target leaf smaller than all: no left neighbor, right neighbor must be at position 0
    // - Target leaf larger than all: left neighbor must be rightmost leaf
    pub fn verify_proof(&self, root: &Node, target_leaf: &L) -> bool {
        // Note: For empty trees, we don't need to check anything
        if root.as_ref() == EMPTY_NODE.as_ref() {
            return true;
        }

        let right_leaf_index = self.index;

        if let Some((left_leaf, left_proof)) = &self.left_leaf {
            let left_leaf_index = self.index - 1;
            if left_leaf >= target_leaf
                || !self.is_valid_neighbor(left_leaf, left_proof, left_leaf_index, root)
            {
                return false;
            }
            // Milestone: If left leaf is present, then left_leaf < target_leaf
        } else if right_leaf_index != 0 || self.right_leaf.is_none() {
            return false;
            // Milestone: If left leaf is not present, then right leaf must be present with index 0.
        }

        if let Some((right_leaf, right_proof)) = &self.right_leaf {
            if right_leaf <= target_leaf
                || !self.is_valid_neighbor(right_leaf, right_proof, right_leaf_index, root)
            {
                return false;
            }

            // Milestone: If right leaf is present, then right_leaf > target_leaf
        } else {
            if let Some((_, left_proof)) = &self.left_leaf {
                let left_leaf_index = self.index - 1;
                if !left_proof.is_right_most(left_leaf_index) {
                    return false;
                }
            } else {
                return false;
            }
            // Milestone: If right leaf is not present, then left leaf must be present and be the rightmost leaf
        }

        true
    }
}

/// Merkle tree using a hash function `T` (default: [`Blake2b256`]) from the [`fastcrypto`] crate.
///
/// The data of the leaves is prefixed with `0x00` before hashing and hashes of inner nodes are
/// computed over the concatenation of their children prefixed with `0x01`. Hashes of empty
/// subtrees (i.e. subtrees without data at their leaves) are replaced with all zeros.
#[derive(Serialize, Deserialize)]
pub struct MerkleTree<T = Blake2b256> {
    _hash_type: PhantomData<T>,
    // The nodes of the Merkle tree are stored in a vector level by level starting with
    // the leaf hashes.
    nodes: Vec<Node>,
    n_leaves: usize,
}

impl<T> core::fmt::Debug for MerkleTree<T> {
    fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        fmt.debug_struct(&format!("MerkleTree<{}>", core::any::type_name::<T>()))
            .field("nodes", &self.nodes)
            .field("n_leaves", &self.n_leaves)
            .finish()
    }
}

pub fn merkle_root<T, I>(iter: I) -> [u8; DIGEST_LEN]
where
    I: IntoIterator,
    I::IntoIter: ExactSizeIterator,
    I::Item: AsRef<[u8]>,
    T: HashFunction<DIGEST_LEN>,
{
    let tree = MerkleTree::<T>::build(iter);
    tree.root().bytes()
}

impl<T> MerkleTree<T>
where
    T: HashFunction<DIGEST_LEN>,
{
    /// Create the [`MerkleTree`] as a commitment to the provided data.
    pub fn build<I>(iter: I) -> Self
    where
        I: IntoIterator,
        I::IntoIter: ExactSizeIterator,
        I::Item: AsRef<[u8]>,
    {
        Self::build_from_leaf_hashes(iter.into_iter().map(|leaf| leaf_hash::<T>(leaf.as_ref())))
    }

    /// Create the [`MerkleTree`] as a commitment to the provided data hashes.
    pub fn build_from_leaf_hashes<I>(iter: I) -> Self
    where
        I: IntoIterator,
        I::IntoIter: ExactSizeIterator<Item = Node>,
    {
        let iter = iter.into_iter();
        tracing::trace!("building Merkle tree over {} items", iter.len());

        // Create the capacity that we know will be needed, since the vec will be
        // reused by the call to from_leaf_nodes.
        let mut nodes = Vec::with_capacity(n_nodes(iter.len()));
        nodes.extend(iter);

        let n_leaves = nodes.len();
        let mut level_nodes = n_leaves;
        let mut prev_level_index = 0;

        // Fill all other nodes of the Merkle Tree
        while level_nodes > 1 {
            if level_nodes % 2 == 1 {
                // We need an empty sibling for the last node on the previous level
                nodes.push(Node::Empty);
                level_nodes += 1;
            }

            let new_level_index = prev_level_index + level_nodes;

            (prev_level_index..new_level_index)
                .step_by(2)
                .for_each(|index| nodes.push(inner_hash::<T>(&nodes[index], &nodes[index + 1])));

            prev_level_index = new_level_index;
            level_nodes /= 2;
        }

        Self {
            nodes,
            n_leaves,
            _hash_type: PhantomData,
        }
    }

    /// Verify that the root of `self` matches the provided root hash.
    pub fn verify_root(&self, root: &Node) -> bool {
        self.root() == *root
    }

    /// Get a copy of the root hash of `self`.
    pub fn root(&self) -> Node {
        self.nodes.last().map_or(Node::Empty, |val| val.clone())
    }

    /// Get the [`MerkleProof`] for the leaf at `leaf_index` consisting
    /// of all sibling hashes on the path from the leaf to the root.
    #[tracing::instrument(skip_all, level = Level::DEBUG, fields(n_leaves = self.n_leaves))]
    pub fn get_proof(&self, leaf_index: usize) -> Result<MerkleProof<T>, LeafIndexOutOfBounds> {
        tracing::trace!("computing Merkle proof");
        if leaf_index >= self.n_leaves {
            tracing::warn!("leaf index out of bounds");
            return Err(LeafIndexOutOfBounds(leaf_index));
        }
        let mut path = Vec::with_capacity(
            usize::try_from(self.n_leaves.ilog2()).expect("this is smaller than `n_leaves`") + 1,
        );
        let mut level_index = leaf_index;
        let mut n_level = self.n_leaves;
        let mut level_base_index = 0;
        while n_level > 1 {
            // All levels contain an even number of nodes
            n_level = n_level.next_multiple_of(2);
            let sibling_index = if level_index % 2 == 0 {
                level_base_index + level_index + 1
            } else {
                level_base_index + level_index - 1
            };
            path.push(self.nodes[sibling_index].clone());
            // Index of the parent on the next level
            level_index /= 2;
            level_base_index += n_level;
            n_level /= 2;
        }
        Ok(MerkleProof {
            _hash_type: PhantomData,
            path,
        })
    }

    /// Compute the non-inclusion proof for the target leaf.
    /// Returns an error if the target leaf is already in the tree.
    pub fn compute_non_inclusion_proof<L: AsRef<[u8]> + Ord + Clone>(
        &self,
        leaves: &[L],
        target_leaf: &L,
    ) -> Result<MerkleNonInclusionProof<L, T>, anyhow::Error> {
        let position = leaves.partition_point(|x| x <= target_leaf);
        if position > 0 && leaves[position - 1] == *target_leaf {
            return Err(anyhow::anyhow!("Target leaf is already in the tree"));
        }

        let left_leaf_proof = if position > 0 {
            Some((leaves[position - 1].clone(), self.get_proof(position - 1)?))
        } else {
            None
        };

        let right_leaf_proof = if position < leaves.len() {
            Some((leaves[position].clone(), self.get_proof(position)?))
        } else {
            None
        };
        Ok(MerkleNonInclusionProof::new(
            left_leaf_proof,
            right_leaf_proof,
            position,
        ))
    }
}

/// Computes the hash of the provided input to be used as a leaf hash of a Merkle tree.
pub(crate) fn leaf_hash<T>(input: &[u8]) -> Node
where
    T: HashFunction<DIGEST_LEN>,
{
    let mut hash_fun = T::default();
    hash_fun.update(LEAF_PREFIX);
    hash_fun.update(input);
    hash_fun.finalize().into()
}

fn inner_hash<T>(left: &Node, right: &Node) -> Node
where
    T: HashFunction<DIGEST_LEN>,
{
    let mut hash_fun = T::default();
    hash_fun.update(INNER_PREFIX);
    hash_fun.update(left.bytes());
    hash_fun.update(right.bytes());
    hash_fun.finalize().into()
}

fn n_nodes(n_leaves: usize) -> usize {
    let mut lvl_nodes = n_leaves;
    let mut tot_nodes = 0;
    while lvl_nodes > 1 {
        lvl_nodes += lvl_nodes % 2;
        tot_nodes += lvl_nodes;
        lvl_nodes /= 2;
    }
    tot_nodes + lvl_nodes
}

#[cfg(test)]
mod test {
    use super::*;

    const TEST_INPUT: [&[u8]; 9] = [
        b"foo", b"bar", b"fizz", b"baz", b"buzz", b"fizz", b"foobar", b"walrus", b"fizz",
    ];

    #[test]
    fn test_n_nodes() {
        assert!(n_nodes(0) == 0);
        assert!(n_nodes(1) == 1);
        assert!(n_nodes(2) == 3);
        assert!(n_nodes(3) == 7);
        assert!(n_nodes(4) == 7);
        assert!(n_nodes(5) == 13);
        assert!(n_nodes(6) == 13);
        assert!(n_nodes(7) == 15);
        assert!(n_nodes(8) == 15);
        assert!(n_nodes(9) == 23);
    }

    #[test]
    fn test_merkle_tree_empty() {
        let mt: MerkleTree = MerkleTree::build::<[&[u8]; 0]>([]);
        assert_eq!(mt.root().bytes(), EMPTY_NODE);
    }

    #[test]
    fn test_merkle_tree_single_element() {
        let test_inp = "Test";
        let mt: MerkleTree = MerkleTree::build(&[test_inp.as_bytes()]);
        let mut hash_fun = Blake2b256::default();
        hash_fun.update(LEAF_PREFIX);
        hash_fun.update(test_inp.as_bytes());
        assert_eq!(mt.root().bytes(), hash_fun.finalize().digest);
    }

    #[test]
    fn test_merkle_tree_empty_element() {
        let mt: MerkleTree = MerkleTree::build(&[&[]]);
        let mut hash_fun = Blake2b256::default();
        hash_fun.update(LEAF_PREFIX);
        hash_fun.update([]);
        assert_eq!(mt.root().bytes(), hash_fun.finalize().digest);
    }

    #[test]
    fn test_get_path_out_of_bounds() {
        let test_inp: Vec<_> = [
            "foo", "bar", "fizz", "baz", "buzz", "fizz", "foobar", "walrus", "fizz",
        ]
        .iter()
        .map(|x| x.as_bytes())
        .collect();
        for i in 0..test_inp.len() {
            let mt: MerkleTree = MerkleTree::build(&test_inp[..i]);
            match mt.get_proof(i.next_power_of_two()) {
                Err(_) => {}
                Ok(_) => panic!("Expected an error"),
            }
        }
    }

    #[test]
    fn test_merkle_path_verify() {
        for i in 0..TEST_INPUT.len() {
            let mt: MerkleTree = MerkleTree::build(&TEST_INPUT[..i]);
            for (index, leaf_data) in TEST_INPUT[..i].iter().enumerate() {
                let proof = mt.get_proof(index).unwrap();
                assert!(proof.verify_proof(&mt.root(), leaf_data, index));
            }
        }
    }

    #[test]
    fn test_merkle_path_verify_fails_for_wrong_index() {
        for i in 0..TEST_INPUT.len() {
            let mt: MerkleTree = MerkleTree::build(&TEST_INPUT[..i]);
            for (index, leaf_data) in TEST_INPUT[..i].iter().enumerate() {
                let proof = mt.get_proof(index).unwrap();
                assert!(!proof.verify_proof(&mt.root(), leaf_data, index + 1));
            }
        }
    }

    #[test]
    fn test_merkle_proof_is_right_most() {
        for i in 0..TEST_INPUT.len() {
            let mt: MerkleTree = MerkleTree::build(&TEST_INPUT[..i]);
            for j in 0..i {
                let proof = mt.get_proof(j).unwrap();
                println!("proof: {:?}", proof);
                if j == i - 1 {
                    assert!(proof.is_right_most(j));
                } else {
                    assert!(!proof.is_right_most(j));
                }
            }
        }
    }

    mod non_inclusion_proof {
        use super::*;

        #[test]
        fn test_empty_tree() {
            let mt: MerkleTree = MerkleTree::build::<[&[u8]; 0]>([]);
            let non_inclusion_proof = mt
                .compute_non_inclusion_proof(&[], &"foo".as_bytes())
                .unwrap();
            assert!(non_inclusion_proof.left_leaf.is_none());
            assert!(non_inclusion_proof.right_leaf.is_none());
            assert_eq!(non_inclusion_proof.index, 0);
            assert!(non_inclusion_proof.verify_proof(&mt.root(), &"foo".as_bytes()));
            assert!(non_inclusion_proof.verify_proof(&mt.root(), &"bar".as_bytes()));
        }

        #[test]
        fn test_single_leaf() {
            let mt: MerkleTree = MerkleTree::build(&["foo".as_bytes()]);
            let non_inclusion_proof = mt
                .compute_non_inclusion_proof(&["foo".as_bytes()], &"bar".as_bytes())
                .unwrap();
            assert!(non_inclusion_proof.verify_proof(&mt.root(), &"bar".as_bytes()));
            assert!(!non_inclusion_proof.verify_proof(&mt.root(), &"foo".as_bytes()));

            let non_inclusion_proof =
                mt.compute_non_inclusion_proof(&["foo".as_bytes()], &"foo".as_bytes());
            assert!(non_inclusion_proof.is_err());
        }

        #[test]
        fn test_multiple_leaves() {
            const TEST_INPUT: [&str; 9] = [
                "foo", "bar", "fizz", "baz", "buzz", "fizz", "foobar", "walrus", "fizz",
            ];
            let mut sorted_test_input = TEST_INPUT.to_vec();
            sorted_test_input.sort();
            println!("sorted_test_input: {:?}", sorted_test_input);
            let mt: MerkleTree = MerkleTree::build(&sorted_test_input);

            let test_cases = [["fuzz", "yankee", "aloha"].to_vec(), TEST_INPUT.to_vec()].concat();
            println!("test_cases: {:?}", test_cases);
            for item in test_cases {
                let non_inclusion_proof = mt.compute_non_inclusion_proof(&sorted_test_input, &item);
                if TEST_INPUT.contains(&item) {
                    assert!(non_inclusion_proof.is_err());
                } else {
                    assert!(non_inclusion_proof.is_ok());
                    let non_inclusion_proof = non_inclusion_proof.unwrap();
                    assert!(non_inclusion_proof.verify_proof(&mt.root(), &item));
                }
            }
        }
    }
}
