// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Annotated-flavor node-table representation. Holds the node data types, the
//! [`AnnotatedPoolBuilder`] alias, and the helpers that translate stored
//! nodes into an annotated `MoveLayoutView`.

use super::encoding::{LayoutRef, LeafType, PoolBuilder};
use crate::compressed::VariantTag;
use crate::compressed::annotated::{
    MoveEnumLayout, MoveFieldsLayout, MoveLayoutView, MoveStructLayout, MoveTypeLayoutRef,
    TypeLayout,
};
use crate::identifier::Identifier;
use crate::language_storage::StructTag;

// =============================================================================
// Node types
// =============================================================================

/// A named field entry: field name paired with its layout reference.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnnotatedFieldEntry<R> {
    pub name: Identifier,
    pub layout: R,
}

/// A single variant entry in an enum node.
/// `None` fields means the variant exists but its field layout is unknown.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnnotatedVariantEntry<R> {
    pub name: Identifier,
    pub tag: VariantTag,
    pub fields: Option<Box<[AnnotatedFieldEntry<R>]>>,
}

/// Annotated struct layout node with type tag and named fields inline.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MoveStructNode<R> {
    pub(crate) type_: StructTag,
    pub(crate) fields: Box<[AnnotatedFieldEntry<R>]>,
}

/// Annotated enum layout node with type tag and named variants inline.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MoveEnumNode<R> {
    pub(crate) type_: StructTag,
    pub(crate) variants: Box<[AnnotatedVariantEntry<R>]>,
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
    /// Build a struct node from a type tag and a slice of `(name, layout-ref)`
    /// pairs.
    pub(crate) fn struct_node(type_tag: &StructTag, fields: &[(&Identifier, LayoutRef)]) -> Self {
        let field_entries: Box<[AnnotatedFieldEntry<LayoutRef>]> = fields
            .iter()
            .map(|(name, h)| AnnotatedFieldEntry {
                name: (*name).clone(),
                layout: *h,
            })
            .collect();
        MoveTypeNode::Struct(MoveStructNode {
            type_: type_tag.clone(),
            fields: field_entries,
        })
    }

    /// Build an enum node from a type tag and a slice of variant descriptors.
    pub(crate) fn enum_node(
        type_tag: &StructTag,
        variants: &[(&Identifier, VariantTag, Option<&[(&Identifier, LayoutRef)]>)],
    ) -> Self {
        let variant_entries: Box<[AnnotatedVariantEntry<LayoutRef>]> = variants
            .iter()
            .map(|(vn, tag, fields)| {
                let field_entries = fields.map(|fields| {
                    fields
                        .iter()
                        .map(|(fn_name, h)| AnnotatedFieldEntry {
                            name: (*fn_name).clone(),
                            layout: *h,
                        })
                        .collect()
                });
                AnnotatedVariantEntry {
                    name: (*vn).clone(),
                    tag: *tag,
                    fields: field_entries,
                }
            })
            .collect();
        MoveTypeNode::Enum(MoveEnumNode {
            type_: type_tag.clone(),
            variants: variant_entries,
        })
    }
}

// =============================================================================
// PoolBuilder alias
// =============================================================================

/// Annotated-flavor node-table builder.
pub type AnnotatedPoolBuilder = PoolBuilder<MoveTypeNode<LayoutRef>>;

// =============================================================================
// View helpers
// =============================================================================

/// Map a [`LeafType`] to the corresponding leaf variant of [`MoveLayoutView`].
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
pub(crate) fn build_view_from_node<'a, T: TypeLayout>(
    pool: &'a T,
    node: &'a MoveTypeNode<T::Root>,
) -> MoveLayoutView<'a, T> {
    match node {
        MoveTypeNode::Vector(inner) => MoveLayoutView::Vector(MoveTypeLayoutRef::new(pool, inner)),
        MoveTypeNode::Struct(s) => MoveLayoutView::Struct(MoveStructLayout {
            type_: &s.type_,
            fields: MoveFieldsLayout {
                pool,
                fields: &s.fields,
            },
        }),
        MoveTypeNode::Enum(e) => MoveLayoutView::Enum(MoveEnumLayout {
            type_: &e.type_,
            variants: &e.variants,
            pool,
        }),
    }
}
