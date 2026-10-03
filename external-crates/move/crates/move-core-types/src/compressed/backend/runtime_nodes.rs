// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Runtime-flavor node-table representation. Holds the node data types, the
//! [`RuntimePoolBuilder`] alias, and the helpers that translate stored nodes
//! into a runtime `MoveLayoutView`.

use super::encoding::{LayoutRef, LeafType, PoolBuilder};
use crate::compressed::runtime::{
    MoveEnumLayout, MoveFieldsLayout, MoveLayoutView, MoveStructLayout, MoveTypeLayoutRef,
    TypeLayout,
};

// =============================================================================
// Node types
// =============================================================================

/// Struct layout node: field types stored as references of type `R`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MoveStructNode<R> {
    pub(crate) fields: Box<[R]>,
}

/// Enum layout node: each variant is either a known list of field
/// references, or `None` if the variant exists but its field layout is
/// not available.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MoveEnumNode<R> {
    pub(crate) variants: Box<[Option<Box<[R]>>]>,
}

/// A compound layout node generic over the reference type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MoveTypeNode<R> {
    Vector(R),
    Struct(MoveStructNode<R>),
    Enum(MoveEnumNode<R>),
}

// =============================================================================
// Constructors for node-table pools (LayoutRef-keyed)
// =============================================================================

impl MoveTypeNode<LayoutRef> {
    /// Build a struct node from a slice of field references.
    pub(crate) fn struct_node(fields: &[LayoutRef]) -> Self {
        MoveTypeNode::Struct(MoveStructNode {
            fields: fields.iter().copied().collect(),
        })
    }

    /// Build an enum node from a slice of optional variant field-reference lists.
    pub(crate) fn enum_node(variants: &[Option<&[LayoutRef]>]) -> Self {
        MoveTypeNode::Enum(MoveEnumNode {
            variants: variants
                .iter()
                .map(|v| v.map(|fields| fields.iter().copied().collect()))
                .collect(),
        })
    }
}

// =============================================================================
// PoolBuilder alias
// =============================================================================

/// Runtime-flavor node-table builder.
pub type RuntimePoolBuilder = PoolBuilder<MoveTypeNode<LayoutRef>>;

// =============================================================================
// View helpers
// =============================================================================

/// Map a [`LeafType`] to the corresponding leaf variant of [`MoveLayoutView`].
#[inline]
pub(crate) fn leaf_view<'a, T: TypeLayout>(leaf: LeafType) -> MoveLayoutView<'a, T> {
    use MoveLayoutView as V;
    match leaf {
        LeafType::Bool => V::Bool,
        LeafType::U8 => V::U8,
        LeafType::U16 => V::U16,
        LeafType::U32 => V::U32,
        LeafType::U64 => V::U64,
        LeafType::U128 => V::U128,
        LeafType::U256 => V::U256,
        LeafType::Address => V::Address,
        LeafType::Signer => V::Signer,
    }
}

/// Build a `MoveLayoutView` from a node-table entry. Reusable across any
/// backend whose `Root` matches the node's reference type.
#[inline]
pub(crate) fn build_view_from_node<'a, T: TypeLayout>(
    pool: &'a T,
    node: &'a MoveTypeNode<T::Root>,
) -> MoveLayoutView<'a, T> {
    match node {
        MoveTypeNode::Vector(inner) => MoveLayoutView::Vector(MoveTypeLayoutRef::new(pool, inner)),
        MoveTypeNode::Struct(s) => MoveLayoutView::Struct(MoveStructLayout {
            fields: MoveFieldsLayout {
                pool,
                fields: &s.fields,
            },
        }),
        MoveTypeNode::Enum(e) => MoveLayoutView::Enum(MoveEnumLayout {
            pool,
            variants: &e.variants,
        }),
    }
}
