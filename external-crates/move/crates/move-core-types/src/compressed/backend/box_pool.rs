// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! `Box`-owned flat node table backend.
//!
//! Same shape as [`super::arc_pool::ArcPool`] but uses `Box<[Node]>` (deep
//! clones, no refcount). Node data types and pool-builder aliases live in
//! [`super::runtime_nodes`] / [`super::annotated_nodes`]; this module owns
//! the storage type, the per-flavor `TypeLayout` impls, and the
//! `BackendBuilder` impls that finalize into a [`BoxPool`].
//!
//! `BoxPool` does **not** define the `MoveTypeLayout::bool()`/`TryFrom<&tree>`/
//! `MoveTypeLayoutBuilder::new()` conveniences — those live on the default
//! backend ([`super::arc_pool::ArcPool`]). Defining them for both pools would
//! make calls that elide the backend type parameter ambiguous (default type
//! parameters do not apply during trait-impl resolution). To build a layout
//! into a `BoxPool`, use the generic builder directly:
//!
//! ```ignore
//! let mut b = RC::MoveTypeLayoutBuilder(RuntimeBoxPoolBuilder::default());
//! let root = b.intern_tree(&tree)?;
//! let layout: RC::MoveTypeLayout<RuntimeBoxPool> = b.build(root);
//! ```

use anyhow::Result as AResult;

use crate::compressed::VariantTag;
use crate::compressed::backend::annotated_nodes::{self, AnnotatedPoolBuilder};
use crate::compressed::backend::encoding::{LayoutRef, LeafType, ResolvedRef};
use crate::compressed::backend::runtime_nodes::{self, RuntimePoolBuilder};
use crate::compressed::{annotated, runtime};
use crate::identifier::Identifier;
use crate::language_storage::StructTag;

// =============================================================================
// BoxPool storage
// =============================================================================

/// `Box`-owned flat node table. Cloning deep-clones the slice (`Node: Clone`).
#[derive(Debug, Clone)]
pub struct BoxPool<Node> {
    pub(crate) nodes: Box<[Node]>,
}

impl<Node> BoxPool<Node> {
    pub(crate) fn from_vec(nodes: Vec<Node>) -> Self {
        Self {
            nodes: nodes.into_boxed_slice(),
        }
    }
}

impl<Node> Default for BoxPool<Node> {
    fn default() -> Self {
        Self::from_vec(Vec::new())
    }
}

// =============================================================================
// Concrete per-flavor aliases
// =============================================================================

/// Annotated-flavor [`BoxPool`] specialization.
pub type AnnotatedBoxPool = BoxPool<annotated_nodes::MoveTypeNode<LayoutRef>>;
/// Runtime-flavor [`BoxPool`] specialization.
pub type RuntimeBoxPool = BoxPool<runtime_nodes::MoveTypeNode<LayoutRef>>;

// =============================================================================
// `TypeLayout` impls (per flavor)
// =============================================================================

impl runtime::TypeLayout for RuntimeBoxPool {
    type Root = LayoutRef;

    #[inline]
    fn realize_view<'a>(&'a self, r: &'a LayoutRef) -> runtime::MoveLayoutView<'a, Self> {
        match r.resolve() {
            ResolvedRef::Leaf(l) => runtime_nodes::leaf_view(l),
            ResolvedRef::Index(i) => runtime_nodes::build_view_from_node(self, &self.nodes[i]),
        }
    }

    #[inline]
    fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

impl annotated::TypeLayout for AnnotatedBoxPool {
    type Root = LayoutRef;

    #[inline]
    fn realize_view<'a>(&'a self, r: &'a LayoutRef) -> annotated::MoveLayoutView<'a, Self> {
        match r.resolve() {
            ResolvedRef::Leaf(l) => annotated_nodes::leaf_view(l),
            ResolvedRef::Index(i) => annotated_nodes::build_view_from_node(self, &self.nodes[i]),
        }
    }

    #[inline]
    fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

// =============================================================================
// Per-pool builder wrappers + `BackendBuilder` impls
// =============================================================================

/// Runtime-flavor builder that finalizes into a [`RuntimeBoxPool`].
#[derive(Debug, Default)]
pub struct RuntimeBoxPoolBuilder(pub RuntimePoolBuilder);

/// Annotated-flavor builder that finalizes into an [`AnnotatedBoxPool`].
#[derive(Debug, Default)]
pub struct AnnotatedBoxPoolBuilder(pub AnnotatedPoolBuilder);

impl runtime::BackendBuilder for RuntimeBoxPoolBuilder {
    type Root = LayoutRef;
    type Output = RuntimeBoxPool;
    type Error = anyhow::Error;

    fn bool(&mut self) -> LayoutRef {
        LayoutRef::leaf(LeafType::Bool)
    }
    fn u8(&mut self) -> LayoutRef {
        LayoutRef::leaf(LeafType::U8)
    }
    fn u16(&mut self) -> LayoutRef {
        LayoutRef::leaf(LeafType::U16)
    }
    fn u32(&mut self) -> LayoutRef {
        LayoutRef::leaf(LeafType::U32)
    }
    fn u64(&mut self) -> LayoutRef {
        LayoutRef::leaf(LeafType::U64)
    }
    fn u128(&mut self) -> LayoutRef {
        LayoutRef::leaf(LeafType::U128)
    }
    fn u256(&mut self) -> LayoutRef {
        LayoutRef::leaf(LeafType::U256)
    }
    fn address(&mut self) -> LayoutRef {
        LayoutRef::leaf(LeafType::Address)
    }
    fn signer(&mut self) -> LayoutRef {
        LayoutRef::leaf(LeafType::Signer)
    }
    fn vector(&mut self, element: LayoutRef) -> AResult<LayoutRef> {
        self.0.intern(runtime_nodes::MoveTypeNode::Vector(element))
    }
    fn struct_layout(&mut self, fields: &[LayoutRef]) -> AResult<LayoutRef> {
        self.0
            .intern(runtime_nodes::MoveTypeNode::struct_node(fields))
    }
    fn enum_layout(&mut self, variants: &[Option<&[LayoutRef]>]) -> AResult<LayoutRef> {
        self.0
            .intern(runtime_nodes::MoveTypeNode::enum_node(variants))
    }
    fn finalize(self, _root: LayoutRef) -> RuntimeBoxPool {
        BoxPool::from_vec(self.0.into_vec())
    }
}

impl annotated::BackendBuilder for AnnotatedBoxPoolBuilder {
    type Root = LayoutRef;
    type Output = AnnotatedBoxPool;
    type Error = anyhow::Error;

    fn bool(&mut self) -> LayoutRef {
        LayoutRef::leaf(LeafType::Bool)
    }
    fn u8(&mut self) -> LayoutRef {
        LayoutRef::leaf(LeafType::U8)
    }
    fn u16(&mut self) -> LayoutRef {
        LayoutRef::leaf(LeafType::U16)
    }
    fn u32(&mut self) -> LayoutRef {
        LayoutRef::leaf(LeafType::U32)
    }
    fn u64(&mut self) -> LayoutRef {
        LayoutRef::leaf(LeafType::U64)
    }
    fn u128(&mut self) -> LayoutRef {
        LayoutRef::leaf(LeafType::U128)
    }
    fn u256(&mut self) -> LayoutRef {
        LayoutRef::leaf(LeafType::U256)
    }
    fn address(&mut self) -> LayoutRef {
        LayoutRef::leaf(LeafType::Address)
    }
    fn signer(&mut self) -> LayoutRef {
        LayoutRef::leaf(LeafType::Signer)
    }
    fn vector(&mut self, element: LayoutRef) -> AResult<LayoutRef> {
        self.0
            .intern(annotated_nodes::MoveTypeNode::Vector(element))
    }
    fn struct_layout(
        &mut self,
        type_tag: &StructTag,
        fields: &[(&Identifier, LayoutRef)],
    ) -> AResult<LayoutRef> {
        self.0
            .intern(annotated_nodes::MoveTypeNode::struct_node(type_tag, fields))
    }
    fn enum_layout(
        &mut self,
        type_tag: &StructTag,
        variants: &[(&Identifier, VariantTag, Option<&[(&Identifier, LayoutRef)]>)],
    ) -> AResult<LayoutRef> {
        self.0
            .intern(annotated_nodes::MoveTypeNode::enum_node(type_tag, variants))
    }
    fn finalize(self, _root: LayoutRef) -> AnnotatedBoxPool {
        BoxPool::from_vec(self.0.into_vec())
    }
}
