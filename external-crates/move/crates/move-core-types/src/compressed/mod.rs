// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod annotated;
pub mod runtime;

// =============================================================================
// Shared types used by both runtime and annotated compressed layouts
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

/// Tag identifying an enum variant. This is a type alias for `u16` — the
/// canonical `VariantTag` lives in `move-binary-format` but cannot be
/// referenced from here (circular dependency), so we define a local alias.
pub type VariantTag = u16;

/// Discriminant for primitive (leaf) Move types, encoded inline in a
/// [`LayoutRef`] rather than stored in the node table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub(crate) enum LeafType {
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

/// A compact reference to a layout node. Bit 15 distinguishes between:
/// - **Leaf** (bit 15 set): the low bits encode a [`LeafType`] discriminant.
/// - **Table index** (bit 15 clear): the low 15 bits index into the node table.
///
/// This is an internal storage type. External callers interact with layouts
/// through [`LayoutHandle`] (for building) and view types (for reading).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct LayoutRef(u16);

/// The result of resolving a [`LayoutRef`].
pub(crate) enum ResolvedRef {
    Leaf(LeafType),
    Index(usize),
}

/// An opaque handle to a layout node returned by the builder.
///
/// Handles are only useful for passing back into the same builder (to compose
/// compound types) or to [`MoveTypeLayoutBuilder::build`] to designate the root.
/// The internal representation is not exposed.
#[derive(Debug, Clone, Copy)]
pub struct LayoutHandle(pub(crate) LayoutRef);

// =============================================================================
// Implementations
// =============================================================================

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

impl LayoutRef {
    pub(crate) const fn leaf(ty: LeafType) -> Self {
        LayoutRef(LEAF_TAG | ty as u16)
    }

    pub(crate) fn index(idx: usize) -> anyhow::Result<Self> {
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
