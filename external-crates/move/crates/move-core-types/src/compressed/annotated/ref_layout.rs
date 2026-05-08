// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Borrowed ("ref") view family for the compressed annotated layout.
//!
//! These mirror the owned types in [`super::layout`] but parameterize over
//! a layout lifetime `'a` instead of holding `Arc`s. Every compound type is
//! a small `(&'a pool, …)` pair; the entire family is `Copy`.
//!
//! ## How it differs from [`super::layout`] (owned)
//!
//! | Concern              | Owned ([`super::layout`])                     | Borrowed (this module)                           |
//! |----------------------|-----------------------------------------------|--------------------------------------------------|
//! | Storage              | `Arc<Pool>`-backed                            | `&'a Pool` (no refcount)                         |
//! | Clone cost           | One `Arc` refcount bump                       | `Copy` (pointer-pair memcpy)                     |
//! | Per-step navigation  | One `Arc` bump per `as_view()` step           | Zero allocations, zero refcount work             |
//! | API surface          | Full: `Display`, `inflate`, `is_type`, `equivalent` | Minimal: navigation only                   |
//! | Lifetime             | Owns its data; `'static`-friendly             | Tied to the source layout's lifetime             |
//!
//! ## When to use
//!
//! - **Use this** for hot traversal paths: BCS deserialization visitors,
//!   field-by-name lookup in tight loops, structural matching on layout
//!   shape. The whole family is alloc-free, so the only cost is the
//!   actual byte-level work.
//! - **Use the owned [`super::layout`] types** when you need to *store* a
//!   layout (or sub-layout) past the borrow — e.g. inside a struct field,
//!   or when handing it to an API that takes the type by value.
//!
//! ## Conversion
//!
//! - [`MoveTypeLayout::as_layout_ref`] borrows the layout's pool without
//!   bumping its `Arc`.
//! - [`MoveTypeLayout::as_view_ref`] additionally resolves the root into a
//!   [`MoveLayoutViewRef`] — useful for `match` over kind.
//! - There is no automatic ref→owned bridge; if you need an owned copy of
//!   a sub-tree, build one through [`super::layout::MoveTypeLayoutBuilder`]
//!   or by indexing into the original owned layout.
//!
//! Note that this is the **same pool** as [`super::layout`] — these types
//! borrow into it directly. The unrelated [`super::exp_layout`] family has
//! its own incompatible pool representation.

use crate::compressed::annotated::layout::{
    AnnotatedFieldEntry, AnnotatedVariantEntry, MoveTypeLayout, MoveTypeNode,
};
use crate::compressed::{LayoutRef, LeafType, ResolvedRef, VariantTag};
use crate::identifier::Identifier;
use crate::language_storage::StructTag;

/// Borrowed counterpart of [`MoveTypeLayout`]: a `(&pool, root)` pair.
#[derive(Debug, Clone, Copy)]
pub struct MoveTypeLayoutRef<'a> {
    pool: &'a [MoveTypeNode],
    root: LayoutRef,
}

/// Borrowed counterpart of [`super::MoveLayoutView`]. Compound variants embed
/// borrowed structs (no `Box`), since each is just a few words.
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

/// Borrowed counterpart of [`super::MoveStructLayout`].
#[derive(Debug, Clone, Copy)]
pub struct MoveStructLayoutRef<'a> {
    type_: &'a StructTag,
    pool: &'a [MoveTypeNode],
    fields: &'a [AnnotatedFieldEntry],
}

/// Borrowed counterpart of [`super::MoveEnumLayout`].
#[derive(Debug, Clone, Copy)]
pub struct MoveEnumLayoutRef<'a> {
    type_: &'a StructTag,
    pool: &'a [MoveTypeNode],
    variants: &'a [AnnotatedVariantEntry],
}

/// Borrowed counterpart of [`super::MoveFieldsLayout`].
#[derive(Debug, Clone, Copy)]
pub struct MoveFieldsLayoutRef<'a> {
    pool: &'a [MoveTypeNode],
    fields: &'a [AnnotatedFieldEntry],
}

/// Borrowed counterpart of [`super::VariantLayout`].
#[derive(Debug, Clone, Copy)]
pub enum VariantLayoutRef<'a> {
    Known {
        name: &'a Identifier,
        tag: VariantTag,
        fields: MoveFieldsLayoutRef<'a>,
    },
    Unknown {
        name: &'a Identifier,
        tag: VariantTag,
    },
}

/// Borrowed counterpart of [`super::MoveDatatypeLayout_`].
#[derive(Debug, Clone, Copy)]
pub enum MoveDatatypeLayoutRef<'a> {
    Struct(MoveStructLayoutRef<'a>),
    Enum(MoveEnumLayoutRef<'a>),
}

// --- Bridges from owned to borrowed ---

impl MoveTypeLayout {
    /// Borrow this layout without bumping the pool's `Arc` refcount.
    pub fn as_layout_ref(&self) -> MoveTypeLayoutRef<'_> {
        MoveTypeLayoutRef {
            pool: &self.pool,
            root: self.root,
        }
    }

    /// Borrow this layout and immediately resolve into a [`MoveLayoutViewRef`].
    pub fn as_view_ref(&self) -> MoveLayoutViewRef<'_> {
        self.as_layout_ref().as_view()
    }
}

// --- MoveTypeLayoutRef ---

impl<'a> MoveTypeLayoutRef<'a> {
    /// Number of compound nodes in the borrowed pool.
    pub fn node_count(&self) -> usize {
        self.pool.len()
    }

    /// Resolve the root reference into a [`MoveLayoutViewRef`].
    pub fn as_view(&self) -> MoveLayoutViewRef<'a> {
        resolve_ref_borrowed(self.pool, self.root)
    }
}

// --- MoveStructLayoutRef ---

impl<'a> MoveStructLayoutRef<'a> {
    pub fn type_(&self) -> &'a StructTag {
        self.type_
    }

    pub fn fields_layout(&self) -> MoveFieldsLayoutRef<'a> {
        MoveFieldsLayoutRef {
            pool: self.pool,
            fields: self.fields,
        }
    }

    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    pub fn field(&self, i: u16) -> Option<(&'a Identifier, MoveTypeLayoutRef<'a>)> {
        let pool = self.pool;
        self.fields.get(i as usize).map(move |entry| {
            (
                &*entry.name,
                MoveTypeLayoutRef {
                    pool,
                    root: entry.layout,
                },
            )
        })
    }

    pub fn fields(&self) -> impl ExactSizeIterator<Item = (&'a Identifier, MoveTypeLayoutRef<'a>)> {
        let pool = self.pool;
        self.fields.iter().map(move |entry| {
            (
                &*entry.name,
                MoveTypeLayoutRef {
                    pool,
                    root: entry.layout,
                },
            )
        })
    }
}

// --- MoveEnumLayoutRef ---

impl<'a> MoveEnumLayoutRef<'a> {
    pub fn type_(&self) -> &'a StructTag {
        self.type_
    }

    pub fn variant_count(&self) -> usize {
        self.variants.len()
    }

    pub fn variant(&self, i: VariantTag) -> Option<VariantLayoutRef<'a>> {
        self.variants
            .get(i as usize)
            .map(|entry| variant_entry_to_ref(self.pool, entry))
    }

    pub fn variant_by_tag(&self, tag: VariantTag) -> Option<VariantLayoutRef<'a>> {
        self.variants
            .iter()
            .find(|entry| entry.tag == tag)
            .map(|entry| variant_entry_to_ref(self.pool, entry))
    }

    pub fn variants(&self) -> impl ExactSizeIterator<Item = VariantLayoutRef<'a>> {
        let pool = self.pool;
        self.variants
            .iter()
            .map(move |entry| variant_entry_to_ref(pool, entry))
    }
}

// --- MoveFieldsLayoutRef ---

impl<'a> MoveFieldsLayoutRef<'a> {
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    pub fn field(&self, i: u16) -> Option<(&'a Identifier, MoveTypeLayoutRef<'a>)> {
        let pool = self.pool;
        self.fields.get(i as usize).map(move |entry| {
            (
                &*entry.name,
                MoveTypeLayoutRef {
                    pool,
                    root: entry.layout,
                },
            )
        })
    }

    pub fn field_by_name(&self, name: &str) -> Option<MoveTypeLayoutRef<'a>> {
        let pool = self.pool;
        self.fields
            .iter()
            .find(|entry| entry.name.as_str() == name)
            .map(move |entry| MoveTypeLayoutRef {
                pool,
                root: entry.layout,
            })
    }

    pub fn fields(&self) -> impl ExactSizeIterator<Item = (&'a Identifier, MoveTypeLayoutRef<'a>)> {
        let pool = self.pool;
        self.fields.iter().map(move |entry| {
            (
                &*entry.name,
                MoveTypeLayoutRef {
                    pool,
                    root: entry.layout,
                },
            )
        })
    }
}

// --- VariantLayoutRef ---

impl<'a> VariantLayoutRef<'a> {
    pub fn name(&self) -> &'a Identifier {
        match self {
            VariantLayoutRef::Known { name, .. } => name,
            VariantLayoutRef::Unknown { name, .. } => name,
        }
    }

    pub fn tag(&self) -> VariantTag {
        match self {
            VariantLayoutRef::Known { tag, .. } => *tag,
            VariantLayoutRef::Unknown { tag, .. } => *tag,
        }
    }

    pub fn fields(&self) -> Option<MoveFieldsLayoutRef<'a>> {
        match self {
            VariantLayoutRef::Known { fields, .. } => Some(*fields),
            VariantLayoutRef::Unknown { .. } => None,
        }
    }
}

// --- MoveDatatypeLayoutRef ---

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

// --- helpers ---

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
    entry: &'a AnnotatedVariantEntry,
) -> VariantLayoutRef<'a> {
    match &entry.fields {
        Some(fields) => VariantLayoutRef::Known {
            name: &entry.name,
            tag: entry.tag,
            fields: MoveFieldsLayoutRef { pool, fields },
        },
        None => VariantLayoutRef::Unknown {
            name: &entry.name,
            tag: entry.tag,
        },
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
            MoveTypeNode::Struct(s) => MoveLayoutViewRef::Struct(MoveStructLayoutRef {
                type_: &s.type_,
                pool,
                fields: &s.fields,
            }),
            MoveTypeNode::Enum(e) => MoveLayoutViewRef::Enum(MoveEnumLayoutRef {
                type_: &e.type_,
                pool,
                variants: &e.variants,
            }),
        },
    }
}
