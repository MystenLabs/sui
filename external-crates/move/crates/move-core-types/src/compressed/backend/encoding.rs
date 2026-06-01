// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Generic encoding primitives shared by all node-table compressed-layout
//! backends: the [`LayoutRef`] 16-bit packed encoding and the deduplicating
//! [`PoolBuilder<Node>`] used during construction.

use anyhow::Result as AResult;
use indexmap::IndexSet;

// =============================================================================
// LeafType
// =============================================================================

/// Discriminant for primitive (leaf) Move types, encoded inline in a
/// [`LayoutRef`] rather than stored in a node table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum LeafType {
    Bool = 0,
    U8 = 1,
    U16 = 2,
    U32 = 3,
    U64 = 4,
    U128 = 5,
    U256 = 6,
    Address = 7,
    Signer = 8,
}

impl LeafType {
    pub(crate) fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Bool),
            1 => Some(Self::U8),
            2 => Some(Self::U16),
            3 => Some(Self::U32),
            4 => Some(Self::U64),
            5 => Some(Self::U128),
            6 => Some(Self::U256),
            7 => Some(Self::Address),
            8 => Some(Self::Signer),
            _ => None,
        }
    }
}

// =============================================================================
// LayoutRef encoding: leaf-inline / pool-index (16 bits)
// =============================================================================

const LEAF_TAG: u16 = 0x8000;
const LEAF_MASK: u16 = !LEAF_TAG;

const _: () = {
    assert!(
        LEAF_TAG & LEAF_MASK == 0,
        "leaf tag and mask must be disjoint"
    );
    assert!(LEAF_TAG == 0x8000, "leaf tag must be the high bit of a u16");
    assert!(
        LEAF_MASK == 0x7FFF,
        "leaf mask must be the low 15 bits of a u16"
    );
};

/// A compact reference to a layout node. Bit 15 distinguishes between:
/// - **Leaf** (bit 15 set): the low bits encode a [`LeafType`] discriminant.
/// - **Table index** (bit 15 clear): the low 15 bits index into the node table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayoutRef(u16);

/// The result of resolving a [`LayoutRef`].
pub(crate) enum ResolvedRef {
    Leaf(LeafType),
    Index(usize),
}

impl LayoutRef {
    pub(crate) const fn leaf(ty: LeafType) -> Self {
        LayoutRef(LEAF_TAG | ty as u16)
    }

    pub(crate) fn index(idx: usize) -> AResult<Self> {
        if idx > LEAF_MASK as usize {
            anyhow::bail!("table index {idx} exceeds 15-bit maximum ({LEAF_MASK})");
        }
        Ok(LayoutRef(idx as u16))
    }

    pub(crate) fn resolve(self) -> ResolvedRef {
        if self.0 & LEAF_TAG != 0 {
            let disc = (self.0 & LEAF_MASK) as u8;
            ResolvedRef::Leaf(
                LeafType::from_u8(disc)
                    .unwrap_or_else(|| panic!("invalid leaf discriminant: {disc}")),
            )
        } else {
            ResolvedRef::Index(self.0 as usize)
        }
    }
}

// =============================================================================
// PoolBuilder: in-progress, deduplicating node table
// =============================================================================

/// Deduplicating builder for a flat node table. Stores compound nodes in an
/// [`IndexSet`] keyed by value; produces a `Vec<Node>` on finalize.
#[derive(Debug)]
pub struct PoolBuilder<Node: Eq + std::hash::Hash> {
    nodes: IndexSet<Node>,
}

impl<Node: Eq + std::hash::Hash> Default for PoolBuilder<Node> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Node: Eq + std::hash::Hash> PoolBuilder<Node> {
    pub fn new() -> Self {
        Self {
            nodes: IndexSet::new(),
        }
    }

    pub(crate) fn intern(&mut self, n: Node) -> AResult<LayoutRef> {
        let (idx, _) = self.nodes.insert_full(n);
        LayoutRef::index(idx)
    }

    pub fn into_vec(self) -> Vec<Node> {
        self.nodes.into_iter().collect()
    }
}
