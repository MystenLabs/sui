// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! `Arc`-shared flat node table backend.
//!
//! Defines the `Arc<[Node]>` storage and per-flavor builder wrappers that
//! produce it. Node data types and pool-builder aliases live in
//! [`super::runtime_nodes`] / [`super::annotated_nodes`]; this module owns
//! the storage type, the per-flavor `TypeLayout` impls, the `BackendBuilder`
//! impls that finalize into an [`ArcPool`], and the convenience surface
//! (`MoveTypeLayout::bool()`/…, `TryFrom<&tree>`, `MoveTypeLayoutBuilder::new`).

use std::sync::Arc;

use anyhow::Result as AResult;

use crate::compressed::VariantTag;
use crate::compressed::backend::annotated_nodes::{self, AnnotatedPoolBuilder};
use crate::compressed::backend::encoding::{LayoutRef, LeafType, ResolvedRef};
use crate::compressed::backend::runtime_nodes::{self, RuntimePoolBuilder};
use crate::compressed::{annotated, runtime};
use crate::identifier::Identifier;
use crate::language_storage::StructTag;
use crate::{annotated_value, runtime_value};

// =============================================================================
// ArcPool storage
// =============================================================================

/// `Arc`-shared flat node table. Cloning is a refcount bump.
#[derive(Debug, Clone)]
pub struct ArcPool<Node> {
    pub(crate) nodes: Arc<[Node]>,
}

impl<Node> ArcPool<Node> {
    pub(crate) fn from_vec(nodes: Vec<Node>) -> Self {
        Self {
            nodes: Arc::from(nodes),
        }
    }
}

impl<Node> Default for ArcPool<Node> {
    fn default() -> Self {
        Self::from_vec(Vec::new())
    }
}

// =============================================================================
// Concrete per-flavor aliases
// =============================================================================

/// Annotated-flavor [`ArcPool`] specialization.
pub type AnnotatedArcPool = ArcPool<annotated_nodes::MoveTypeNode<LayoutRef>>;
/// Runtime-flavor [`ArcPool`] specialization.
pub type RuntimeArcPool = ArcPool<runtime_nodes::MoveTypeNode<LayoutRef>>;

// =============================================================================
// `TypeLayout` impls (per flavor)
// =============================================================================

impl runtime::TypeLayout for RuntimeArcPool {
    type Root = LayoutRef;

    fn realize_view<'a>(&'a self, r: &'a LayoutRef) -> runtime::MoveLayoutView<'a, Self> {
        match r.resolve() {
            ResolvedRef::Leaf(l) => runtime_nodes::leaf_view(l),
            ResolvedRef::Index(i) => runtime_nodes::build_view_from_node(self, &self.nodes[i]),
        }
    }

    fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

impl annotated::TypeLayout for AnnotatedArcPool {
    type Root = LayoutRef;

    fn realize_view<'a>(&'a self, r: &'a LayoutRef) -> annotated::MoveLayoutView<'a, Self> {
        match r.resolve() {
            ResolvedRef::Leaf(l) => annotated_nodes::leaf_view(l),
            ResolvedRef::Index(i) => annotated_nodes::build_view_from_node(self, &self.nodes[i]),
        }
    }

    fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

// =============================================================================
// Per-pool builder wrappers + `BackendBuilder` impls
// =============================================================================

/// Runtime-flavor builder that finalizes into a [`RuntimeArcPool`].
#[derive(Debug, Default)]
pub struct RuntimeArcPoolBuilder(pub RuntimePoolBuilder);

/// Annotated-flavor builder that finalizes into an [`AnnotatedArcPool`].
#[derive(Debug, Default)]
pub struct AnnotatedArcPoolBuilder(pub AnnotatedPoolBuilder);

impl RuntimeArcPoolBuilder {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AnnotatedArcPoolBuilder {
    pub fn new() -> Self {
        Self::default()
    }
}

impl runtime::BackendBuilder for RuntimeArcPoolBuilder {
    type Root = LayoutRef;
    type Output = RuntimeArcPool;
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
    fn finalize(self, _root: LayoutRef) -> RuntimeArcPool {
        ArcPool::from_vec(self.0.into_vec())
    }
}

impl annotated::BackendBuilder for AnnotatedArcPoolBuilder {
    type Root = LayoutRef;
    type Output = AnnotatedArcPool;
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
    fn finalize(self, _root: LayoutRef) -> AnnotatedArcPool {
        ArcPool::from_vec(self.0.into_vec())
    }
}

// =============================================================================
// Convenience surface (leaf constructors, TryFrom, builder new/Default)
// =============================================================================
//
// These impls are concrete (not generic) so that callers can write
// `RC::MoveTypeLayout::try_from(...)` or `RC::MoveTypeLayoutBuilder::new()`
// without specifying a backend — the type-parameter default (`ArcPool`) makes
// resolution unambiguous.

impl runtime::MoveTypeLayout<RuntimeArcPool> {
    fn leaf(ty: LeafType) -> Self {
        runtime::MoveTypeLayout::from_parts(RuntimeArcPool::default(), LayoutRef::leaf(ty))
    }
}

impl annotated::MoveTypeLayout<AnnotatedArcPool> {
    fn leaf(ty: LeafType) -> Self {
        annotated::MoveTypeLayout::from_parts(AnnotatedArcPool::default(), LayoutRef::leaf(ty))
    }
}

// Per-flavor leaf-constructor convenience methods, generated identically for
// both flavors. Both delegate to the private `Self::leaf(ty)` defined above.
macro_rules! impl_leaf_ctors {
    ($ty:ty) => {
        impl $ty {
            pub fn bool() -> Self {
                Self::leaf(LeafType::Bool)
            }
            pub fn u8() -> Self {
                Self::leaf(LeafType::U8)
            }
            pub fn u16() -> Self {
                Self::leaf(LeafType::U16)
            }
            pub fn u32() -> Self {
                Self::leaf(LeafType::U32)
            }
            pub fn u64() -> Self {
                Self::leaf(LeafType::U64)
            }
            pub fn u128() -> Self {
                Self::leaf(LeafType::U128)
            }
            pub fn u256() -> Self {
                Self::leaf(LeafType::U256)
            }
            pub fn address() -> Self {
                Self::leaf(LeafType::Address)
            }
            pub fn signer() -> Self {
                Self::leaf(LeafType::Signer)
            }
        }
    };
}
impl_leaf_ctors!(runtime::MoveTypeLayout<RuntimeArcPool>);
impl_leaf_ctors!(annotated::MoveTypeLayout<AnnotatedArcPool>);

impl TryFrom<&runtime_value::MoveTypeLayout> for runtime::MoveTypeLayout<RuntimeArcPool> {
    type Error = anyhow::Error;
    fn try_from(layout: &runtime_value::MoveTypeLayout) -> Result<Self, Self::Error> {
        use runtime::BackendBuilder as _;
        let mut b = RuntimeArcPoolBuilder::default();
        let root = b.intern_tree(layout)?;
        Ok(b.build(root))
    }
}

impl TryFrom<&annotated_value::MoveTypeLayout> for annotated::MoveTypeLayout<AnnotatedArcPool> {
    type Error = anyhow::Error;
    fn try_from(layout: &annotated_value::MoveTypeLayout) -> Result<Self, Self::Error> {
        use annotated::BackendBuilder as _;
        let mut b = AnnotatedArcPoolBuilder::default();
        let root = b.intern_tree(layout)?;
        Ok(b.build(root))
    }
}
