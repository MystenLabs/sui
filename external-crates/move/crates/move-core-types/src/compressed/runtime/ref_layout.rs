// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Borrowed ("ref") view family for the compressed runtime layout.
//!
//! These mirror the owned types in [`super::layout`] but parameterize over
//! a layout lifetime `'a` instead of holding `Arc`s. Every compound type is
//! a small `(&'a pool, …)` pair; the entire family is `Copy`.
//!
//! ## How it differs from [`super::layout`] (owned)
//!
//! | Concern              | Owned ([`super::layout`])                | Borrowed (this module)                 |
//! |----------------------|------------------------------------------|----------------------------------------|
//! | Storage              | `Arc<Pool>`-backed                       | `&'a Pool` (no refcount)               |
//! | Clone cost           | One `Arc` refcount bump                  | `Copy` (pointer-pair memcpy)           |
//! | Per-step navigation  | One `Arc` bump per `as_view()` step      | Zero allocations                       |
//! | API surface          | Full: `Display`, `inflate`, `equivalent` | Minimal: navigation only               |
//! | Lifetime             | Owns its data; `'static`-friendly        | Tied to the source layout's lifetime   |
//!
//! ## How it differs from [`crate::compressed::annotated::ref_layout`]
//!
//! Same role, different pool: this family carries no `Identifier`s or
//! `StructTag`s, matching the structurally-typed runtime form. If you need
//! names alongside structure, use the annotated family.
//!
//! ## When to use
//!
//! - **Use this** for hot deserialization paths where you only need the
//!   structural shape of a value — e.g. when decoding BCS bytes against a
//!   known runtime schema.
//! - **Use [`super::layout`]** when you need to *store* a layout past the
//!   borrow.
//!
//! Convert via [`MoveTypeLayout::as_layout_ref`] /
//! [`MoveTypeLayout::as_view_ref`]. There is no automatic ref→owned
//! bridge — rebuild through [`super::layout::MoveTypeLayoutBuilder`] if
//! you need ownership.

use std::sync::Arc;

use crate::compressed::runtime::layout::{MoveTypeLayout, MoveTypeNode};
use crate::compressed::{LayoutRef, LeafType, ResolvedRef, VariantTag};

#[derive(Debug, Clone, Copy)]
pub struct MoveTypeLayoutRef<'a> {
    pool: &'a [MoveTypeNode],
    root: LayoutRef,
}

#[derive(Debug, Clone, Copy)]
pub enum MoveLayoutViewRef<'a> {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Address,
    Signer,
    Vector(MoveTypeLayoutRef<'a>),
    Struct(MoveStructLayoutRef<'a>),
    Enum(MoveEnumLayoutRef<'a>),
}

#[derive(Debug, Clone, Copy)]
pub struct MoveStructLayoutRef<'a>(pub MoveFieldsLayoutRef<'a>);

#[derive(Debug, Clone, Copy)]
pub struct MoveEnumLayoutRef<'a> {
    pool: &'a [MoveTypeNode],
    variants: &'a [Option<Arc<[LayoutRef]>>],
}

#[derive(Debug, Clone, Copy)]
pub struct MoveFieldsLayoutRef<'a> {
    pool: &'a [MoveTypeNode],
    fields: &'a [LayoutRef],
}

#[derive(Debug, Clone, Copy)]
pub enum VariantLayoutRef<'a> {
    Known(MoveFieldsLayoutRef<'a>),
    Unknown,
}

#[derive(Debug, Clone, Copy)]
pub enum MoveDatatypeLayoutRef<'a> {
    Struct(MoveStructLayoutRef<'a>),
    Enum(MoveEnumLayoutRef<'a>),
}

impl MoveTypeLayout {
    pub fn as_layout_ref(&self) -> MoveTypeLayoutRef<'_> {
        MoveTypeLayoutRef {
            pool: &self.pool,
            root: self.root,
        }
    }

    pub fn as_view_ref(&self) -> MoveLayoutViewRef<'_> {
        self.as_layout_ref().as_view()
    }
}

impl<'a> MoveTypeLayoutRef<'a> {
    pub fn node_count(&self) -> usize {
        self.pool.len()
    }

    pub fn as_view(&self) -> MoveLayoutViewRef<'a> {
        resolve_ref_borrowed(self.pool, self.root)
    }
}

impl<'a> MoveStructLayoutRef<'a> {
    pub fn fields_layout(&self) -> MoveFieldsLayoutRef<'a> {
        self.0
    }

    pub fn field_count(&self) -> usize {
        self.0.field_count()
    }

    pub fn field(&self, i: u16) -> Option<MoveTypeLayoutRef<'a>> {
        self.0.field(i)
    }

    pub fn fields(&self) -> impl ExactSizeIterator<Item = MoveTypeLayoutRef<'a>> {
        self.0.fields()
    }
}

impl<'a> MoveEnumLayoutRef<'a> {
    pub fn variant_count(&self) -> usize {
        self.variants.len()
    }

    pub fn variant(&self, i: VariantTag) -> Option<VariantLayoutRef<'a>> {
        self.variants
            .get(i as usize)
            .map(|v| variant_entry_to_ref(self.pool, v))
    }

    pub fn variants(&self) -> impl ExactSizeIterator<Item = VariantLayoutRef<'a>> {
        let pool = self.pool;
        self.variants
            .iter()
            .map(move |v| variant_entry_to_ref(pool, v))
    }
}

impl<'a> MoveFieldsLayoutRef<'a> {
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    pub fn field(&self, i: u16) -> Option<MoveTypeLayoutRef<'a>> {
        let pool = self.pool;
        self.fields
            .get(i as usize)
            .map(move |&root| MoveTypeLayoutRef { pool, root })
    }

    pub fn fields(&self) -> impl ExactSizeIterator<Item = MoveTypeLayoutRef<'a>> {
        let pool = self.pool;
        self.fields
            .iter()
            .map(move |&root| MoveTypeLayoutRef { pool, root })
    }
}

impl<'a> MoveDatatypeLayoutRef<'a> {
    pub fn as_struct(&self) -> Option<MoveStructLayoutRef<'a>> {
        match self {
            MoveDatatypeLayoutRef::Struct(s) => Some(*s),
            _ => None,
        }
    }

    pub fn as_enum(&self) -> Option<MoveEnumLayoutRef<'a>> {
        match self {
            MoveDatatypeLayoutRef::Enum(e) => Some(*e),
            _ => None,
        }
    }
}

fn leaf_to_layout_view_ref<'a>(leaf: LeafType) -> MoveLayoutViewRef<'a> {
    match leaf {
        LeafType::Bool => MoveLayoutViewRef::Bool,
        LeafType::U8 => MoveLayoutViewRef::U8,
        LeafType::U16 => MoveLayoutViewRef::U16,
        LeafType::U32 => MoveLayoutViewRef::U32,
        LeafType::U64 => MoveLayoutViewRef::U64,
        LeafType::U128 => MoveLayoutViewRef::U128,
        LeafType::U256 => MoveLayoutViewRef::U256,
        LeafType::Address => MoveLayoutViewRef::Address,
        LeafType::Signer => MoveLayoutViewRef::Signer,
    }
}

fn variant_entry_to_ref<'a>(
    pool: &'a [MoveTypeNode],
    entry: &'a Option<Arc<[LayoutRef]>>,
) -> VariantLayoutRef<'a> {
    match entry {
        Some(fields) => VariantLayoutRef::Known(MoveFieldsLayoutRef { pool, fields }),
        None => VariantLayoutRef::Unknown,
    }
}

/// Borrowed analogue of `resolve_ref` in [`super::layout`]. Panics on
/// out-of-bounds index.
fn resolve_ref_borrowed<'a>(pool: &'a [MoveTypeNode], r: LayoutRef) -> MoveLayoutViewRef<'a> {
    match r.resolve() {
        ResolvedRef::Leaf(leaf) => leaf_to_layout_view_ref(leaf),
        ResolvedRef::Index(idx) => match &pool[idx] {
            MoveTypeNode::Vector(inner) => {
                MoveLayoutViewRef::Vector(MoveTypeLayoutRef { pool, root: *inner })
            }
            MoveTypeNode::Struct(s) => {
                MoveLayoutViewRef::Struct(MoveStructLayoutRef(MoveFieldsLayoutRef {
                    pool,
                    fields: &s.fields,
                }))
            }
            MoveTypeNode::Enum(e) => MoveLayoutViewRef::Enum(MoveEnumLayoutRef {
                pool,
                variants: &e.variants,
            }),
        },
    }
}
