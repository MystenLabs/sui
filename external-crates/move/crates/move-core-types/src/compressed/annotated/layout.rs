// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Owned compressed annotated layout — the canonical, cloneable layout type
//! used everywhere a Move value's *typed* layout (with field names, variant
//! names, and `StructTag`s) needs to be stored or passed by value.
//!
//! ## Representation
//!
//! [`MoveTypeLayout`] is `(Arc<MoveTypeLayoutPool>, LayoutRef)` — a shared
//! pool of compound nodes plus a 16-bit reference into it. Cloning a layout
//! is one `Arc` refcount bump, regardless of layout size.
//!
//! Inside the pool, struct and enum nodes hold:
//!   - `type_: Arc<StructTag>` — shared so `as_view()` is an `Arc` bump,
//!     not a deep `StructTag` clone.
//!   - field/variant entry slices with `name: Arc<Identifier>` for the
//!     same reason.
//!
//! Variants in [`MoveEnumLayout`] are stored in their compressed form
//! (`Arc<[AnnotatedVariantEntry]>`) and are materialized into the public
//! [`VariantLayout`] form only on access — so `as_view()` on a wide enum
//! is O(1).
//!
//! ## When to use
//!
//! Use `MoveTypeLayout` when you need an **owned** layout: storing one in a
//! struct, returning one from a function across a lifetime boundary, or
//! handing one to APIs that don't take a borrow. Cloning is cheap.
//!
//! For zero-allocation traversal where a borrow suffices, prefer the
//! borrowed family in [`super::ref_layout`] (`MoveTypeLayoutRef<'a>`),
//! reachable via [`MoveTypeLayout::as_layout_ref`] /
//! [`MoveTypeLayout::as_view_ref`].
//!
//! For an experimental alternative pool layout (struct-of-vecs, packed
//! refs) that's denser at the cost of an extra indirection per field, see
//! [`super::exp_layout`].

use crate::annotated_value as AV;
pub use crate::compressed::LayoutHandle;
use crate::compressed::{LayoutRef, LeafType, ResolvedRef, VariantTag};
use crate::identifier::Identifier;
use crate::language_storage::{StructTag, TypeTag};
use anyhow::Result as AResult;
use indexmap::IndexSet;
use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

static EMPTY_POOL: std::sync::LazyLock<Arc<MoveTypeLayoutPool>> =
    std::sync::LazyLock::new(|| Arc::from(Vec::<MoveTypeNode>::new()));

// --- Node types ---

/// A named field entry: field name paired with its layout reference.
/// `name` is `Arc<Identifier>` so that materializing the name on field
/// access is an `Arc` bump rather than a `Box<str>` realloc.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AnnotatedFieldEntry {
    pub name: Arc<Identifier>,
    pub layout: LayoutRef,
}

/// A single variant entry in an enum node.
/// `None` fields means the variant exists but its field layout is unknown.
/// `name` is `Arc<Identifier>` for the same reason as in
/// [`AnnotatedFieldEntry`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AnnotatedVariantEntry {
    pub name: Arc<Identifier>,
    pub tag: VariantTag,
    pub fields: Option<Arc<[AnnotatedFieldEntry]>>,
}

/// Annotated struct layout node with type tag and named fields inline.
/// `type_` is wrapped in `Arc` so that `as_view()` is an `Arc` bump rather
/// than a deep [`StructTag`] clone (which would allocate two `Identifier`s).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MoveStructNode {
    pub(crate) type_: Arc<StructTag>,
    pub(crate) fields: Arc<[AnnotatedFieldEntry]>,
}

/// Annotated enum layout node with type tag and named variants inline.
/// See [`MoveStructNode`] for the rationale on `Arc<StructTag>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MoveEnumNode {
    pub(crate) type_: Arc<StructTag>,
    pub(crate) variants: Arc<[AnnotatedVariantEntry]>,
}

/// A compound layout node in the annotated compressed node table.
/// Leaf types (primitives) are encoded inline in [`LayoutRef`] and never
/// appear in the table.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum MoveTypeNode {
    Vector(LayoutRef),
    Struct(MoveStructNode),
    Enum(MoveEnumNode),
}

/// The shared node table backing a [`MoveTypeLayout`].
pub(crate) type MoveTypeLayoutPool = [MoveTypeNode];

// --- Owned layout types ---

/// A deduplicated, flat representation of an annotated [`AV::MoveTypeLayout`] tree.
/// Names and type tags are stored inline in nodes. Cloning is cheap — the
/// node table is shared via `Arc`.
///
/// NOTE: `Eq`/`PartialEq` are implemented manually (delegating to
/// [`MoveTypeLayout::equivalent`]) rather than derived, because two layouts
/// representing the same type may have different pool orderings or sharing
/// patterns and structural equality on the raw fields would produce false
/// negatives. `Hash` is intentionally not implemented (no canonical form).
#[derive(Debug, Clone)]
pub struct MoveTypeLayout {
    pub(crate) pool: Arc<MoveTypeLayoutPool>,
    pub(crate) root: LayoutRef,
}

/// A resolved view of an annotated layout node. Compound types contain
/// owned layout types for direct navigation.
#[derive(Debug, Clone)]
pub enum MoveLayoutView {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Address,
    Signer,
    Vector(MoveTypeLayout),
    Struct(MoveStructLayout),
    Enum(MoveEnumLayout),
}

/// A compressed layout that is known to be a struct or enum (not a primitive
/// or vector). This mirrors the tree-based [`crate::annotated_value::MoveDatatypeLayout`].
#[derive(Debug, Clone)]
pub enum MoveDatatypeLayout_ {
    Struct(MoveStructLayout),
    Enum(MoveEnumLayout),
}

/// Datatype layout with a reference to the original layout for inflation and conversion.
#[derive(Debug, Clone)]
pub struct MoveDatatypeLayout {
    self_layout: MoveTypeLayout,
    inner: MoveDatatypeLayout_,
}

/// The enum layout with type tag and named variants, as a view into a shared
/// pool. Variant `VariantLayout`s are materialized lazily from the underlying
/// pool entries — `as_view()` no longer pre-collects a `Vec<VariantLayout>`,
/// so wide enums see O(1) view construction.
#[derive(Debug, Clone)]
pub struct MoveEnumLayout {
    type_: Arc<StructTag>,
    pool: Arc<MoveTypeLayoutPool>,
    pub(crate) variants: Arc<[AnnotatedVariantEntry]>,
}

/// The struct layout with type tag and named fields, as a view into a shared
/// pool. `type_` is `Arc<StructTag>`-shared with the pool so cloning is a
/// refcount bump rather than a deep [`StructTag`] clone.
#[derive(Debug, Clone)]
pub struct MoveStructLayout {
    type_: Arc<StructTag>,
    pub(crate) fields: MoveFieldsLayout,
}

/// The result of looking up a variant in an annotated enum layout.
/// `name` is `Arc<Identifier>` so that materializing a variant from a pool
/// entry is just an `Arc` bump.
#[derive(Debug, Clone)]
pub enum VariantLayout {
    /// The variant's field layout is known.
    Known {
        name: Arc<Identifier>,
        tag: VariantTag,
        fields: MoveFieldsLayout,
    },
    /// The variant exists but its field layout is not available.
    Unknown {
        name: Arc<Identifier>,
        tag: VariantTag,
    },
}

/// The field layout of a struct or enum variant, as a view into a shared pool.
#[derive(Debug, Clone)]
pub struct MoveFieldsLayout {
    pool: Arc<MoveTypeLayoutPool>,
    fields: Arc<[AnnotatedFieldEntry]>,
}

// --- Builder type ---

/// Incrementally builds an annotated [`MoveTypeLayout`] with automatic
/// deduplication of nodes.
#[derive(Debug, Clone)]
pub struct MoveTypeLayoutBuilder {
    nodes: IndexSet<MoveTypeNode>,
}

// =============================================================================
// Implementations
// =============================================================================

// --- MoveTypeLayout ---

impl MoveTypeLayout {
    /// Number of compound nodes in the table (excludes inline leaf types).
    pub fn node_count(&self) -> usize {
        self.pool.len()
    }

    fn leaf(ty: LeafType) -> Self {
        MoveTypeLayout {
            pool: EMPTY_POOL.clone(),
            root: LayoutRef::leaf(ty),
        }
    }

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

    /// Create a resolved view for navigating this layout.
    pub fn as_view(&self) -> MoveLayoutView {
        resolve_ref(&self.pool, self.root)
    }

    /// Inflate back into a tree-based [`MoveTypeLayout`].
    pub fn inflate(&self) -> AResult<AV::MoveTypeLayout> {
        self.as_view().inflate()
    }

    pub fn is_type(&self, t: &TypeTag) -> bool {
        self.as_view().is_type(t)
    }

    /// If this layout is a struct, return it. Otherwise `None`.
    pub fn into_struct(self) -> Option<MoveStructLayout> {
        match self.as_view() {
            MoveLayoutView::Struct(s) => Some(s),
            _ => None,
        }
    }

    /// If this layout is an enum, return it. Otherwise `None`.
    pub fn into_enum(self) -> Option<MoveEnumLayout> {
        match self.as_view() {
            MoveLayoutView::Enum(e) => Some(e),
            _ => None,
        }
    }

    /// Returns `true` iff `self` and `other` describe the same Move type
    /// (same shape, type tags, field names, variant names+tags), regardless
    /// of pool ordering or how subtrees are shared.
    pub fn equivalent(&self, other: &Self) -> bool {
        if Arc::ptr_eq(&self.pool, &other.pool) && self.root == other.root {
            return true;
        }
        let mut memo = HashSet::new();
        nodes_equivalent(&self.pool, self.root, &other.pool, other.root, &mut memo)
    }
}

impl fmt::Display for MoveTypeLayout {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() {
            write!(f, "{:#}", self.as_view())
        } else {
            write!(f, "{}", self.as_view())
        }
    }
}

impl PartialEq for MoveTypeLayout {
    fn eq(&self, other: &Self) -> bool {
        self.equivalent(other)
    }
}

impl Eq for MoveTypeLayout {}

impl PartialEq for MoveLayoutView {
    fn eq(&self, other: &Self) -> bool {
        self.equivalent(other)
    }
}

impl Eq for MoveLayoutView {}

impl PartialEq for MoveStructLayout {
    fn eq(&self, other: &Self) -> bool {
        self.equivalent(other)
    }
}

impl Eq for MoveStructLayout {}

impl PartialEq for MoveEnumLayout {
    fn eq(&self, other: &Self) -> bool {
        self.equivalent(other)
    }
}

impl Eq for MoveEnumLayout {}

impl PartialEq for MoveFieldsLayout {
    fn eq(&self, other: &Self) -> bool {
        self.equivalent(other)
    }
}

impl Eq for MoveFieldsLayout {}

impl PartialEq for MoveDatatypeLayout {
    fn eq(&self, other: &Self) -> bool {
        self.equivalent(other)
    }
}

impl Eq for MoveDatatypeLayout {}

impl TryFrom<&AV::MoveTypeLayout> for MoveTypeLayout {
    type Error = anyhow::Error;
    fn try_from(layout: &AV::MoveTypeLayout) -> Result<Self, Self::Error> {
        let mut b = MoveTypeLayoutBuilder::new();
        let root = b.from_tree(layout)?;
        Ok(b.build(root))
    }
}

impl TryFrom<AV::MoveTypeLayout> for MoveTypeLayout {
    type Error = anyhow::Error;
    fn try_from(layout: AV::MoveTypeLayout) -> Result<Self, Self::Error> {
        (&layout).try_into()
    }
}

impl TryFrom<&AV::MoveStructLayout> for MoveStructLayout {
    type Error = anyhow::Error;
    fn try_from(layout: &AV::MoveStructLayout) -> Result<Self, Self::Error> {
        let mut b = MoveTypeLayoutBuilder::new();
        let root = b.from_tree_struct_layout(layout)?;
        let built = b.build(root);
        match built.as_view() {
            MoveLayoutView::Struct(s) => Ok(s),
            _ => anyhow::bail!("expected struct layout from from_tree_struct_layout"),
        }
    }
}

impl TryFrom<AV::MoveStructLayout> for MoveStructLayout {
    type Error = anyhow::Error;
    fn try_from(layout: AV::MoveStructLayout) -> Result<Self, Self::Error> {
        (&layout).try_into()
    }
}

impl TryFrom<&AV::MoveEnumLayout> for MoveEnumLayout {
    type Error = anyhow::Error;
    fn try_from(layout: &AV::MoveEnumLayout) -> Result<Self, Self::Error> {
        let tree = AV::MoveTypeLayout::Enum(Box::new(layout.clone()));
        let built: MoveTypeLayout = (&tree).try_into()?;
        match built.as_view() {
            MoveLayoutView::Enum(e) => Ok(e),
            _ => anyhow::bail!("expected enum layout from AV::MoveTypeLayout::Enum"),
        }
    }
}

impl TryFrom<AV::MoveEnumLayout> for MoveEnumLayout {
    type Error = anyhow::Error;
    fn try_from(layout: AV::MoveEnumLayout) -> Result<Self, Self::Error> {
        (&layout).try_into()
    }
}

impl TryFrom<&AV::MoveDatatypeLayout> for MoveDatatypeLayout {
    type Error = anyhow::Error;
    fn try_from(layout: &AV::MoveDatatypeLayout) -> Result<Self, Self::Error> {
        let tree = match layout {
            AV::MoveDatatypeLayout::Struct(s) => AV::MoveTypeLayout::Struct(s.clone()),
            AV::MoveDatatypeLayout::Enum(e) => AV::MoveTypeLayout::Enum(e.clone()),
        };
        let built: MoveTypeLayout = (&tree).try_into()?;
        MoveDatatypeLayout::new(built)
            .ok_or_else(|| anyhow::anyhow!("expected struct or enum layout"))
    }
}

impl TryFrom<AV::MoveDatatypeLayout> for MoveDatatypeLayout {
    type Error = anyhow::Error;
    fn try_from(layout: AV::MoveDatatypeLayout) -> Result<Self, Self::Error> {
        (&layout).try_into()
    }
}

// --- MoveLayoutView ---

impl MoveLayoutView {
    /// Reconstruct the equivalent tree-based layout. Returns an error
    /// if any enum variant has an unknown layout.
    pub fn inflate(&self) -> AResult<AV::MoveTypeLayout> {
        use crate::annotated_value::{
            MoveEnumLayout as TreeEnumLayout, MoveStructLayout as TreeStructLayout,
        };
        Ok(match self {
            MoveLayoutView::Bool => AV::MoveTypeLayout::Bool,
            MoveLayoutView::U8 => AV::MoveTypeLayout::U8,
            MoveLayoutView::U16 => AV::MoveTypeLayout::U16,
            MoveLayoutView::U32 => AV::MoveTypeLayout::U32,
            MoveLayoutView::U64 => AV::MoveTypeLayout::U64,
            MoveLayoutView::U128 => AV::MoveTypeLayout::U128,
            MoveLayoutView::U256 => AV::MoveTypeLayout::U256,
            MoveLayoutView::Address => AV::MoveTypeLayout::Address,
            MoveLayoutView::Signer => AV::MoveTypeLayout::Signer,
            MoveLayoutView::Vector(vv) => AV::MoveTypeLayout::Vector(Box::new(vv.inflate()?)),
            MoveLayoutView::Struct(sv) => {
                let fields = sv
                    .fields()
                    .map(|(name, layout)| {
                        Ok(AV::MoveFieldLayout::new(
                            (**name).clone(),
                            layout.inflate()?,
                        ))
                    })
                    .collect::<AResult<_>>()?;
                AV::MoveTypeLayout::Struct(Box::new(TreeStructLayout {
                    type_: sv.type_().clone(),
                    fields,
                }))
            }
            MoveLayoutView::Enum(ev) => {
                let variants = ev
                    .variants()
                    .map(|vl| match vl.fields() {
                        Some(fields) => {
                            let field_layouts = fields
                                .fields()
                                .map(|(name, layout)| {
                                    Ok(AV::MoveFieldLayout::new(
                                        (**name).clone(),
                                        layout.inflate()?,
                                    ))
                                })
                                .collect::<AResult<_>>()?;
                            Ok(((vl.name().clone(), vl.tag()), field_layouts))
                        }
                        None => {
                            anyhow::bail!("cannot inflate enum with unknown variant layout")
                        }
                    })
                    .collect::<AResult<_>>()?;
                AV::MoveTypeLayout::Enum(Box::new(TreeEnumLayout {
                    type_: ev.type_().clone(),
                    variants,
                }))
            }
        })
    }

    /// Check whether this layout matches the given [`TypeTag`].
    pub fn is_type(&self, t: &TypeTag) -> bool {
        match self {
            MoveLayoutView::Bool => *t == TypeTag::Bool,
            MoveLayoutView::U8 => *t == TypeTag::U8,
            MoveLayoutView::U16 => *t == TypeTag::U16,
            MoveLayoutView::U32 => *t == TypeTag::U32,
            MoveLayoutView::U64 => *t == TypeTag::U64,
            MoveLayoutView::U128 => *t == TypeTag::U128,
            MoveLayoutView::U256 => *t == TypeTag::U256,
            MoveLayoutView::Address => *t == TypeTag::Address,
            MoveLayoutView::Signer => *t == TypeTag::Signer,
            MoveLayoutView::Struct(sv) => sv.is_type(t),
            MoveLayoutView::Vector(vv) => {
                if let TypeTag::Vector(inner) = t {
                    vv.as_view().is_type(inner)
                } else {
                    false
                }
            }
            MoveLayoutView::Enum(ev) => ev.is_type(t),
        }
    }

    /// Returns `true` iff `self` and `other` describe the same Move type,
    /// regardless of pool ordering or how subtrees are shared.
    pub fn equivalent(&self, other: &Self) -> bool {
        use MoveLayoutView::*;
        match (self, other) {
            (Bool, Bool)
            | (U8, U8)
            | (U16, U16)
            | (U32, U32)
            | (U64, U64)
            | (U128, U128)
            | (U256, U256)
            | (Address, Address)
            | (Signer, Signer) => true,
            (Vector(a), Vector(b)) => a.equivalent(b),
            (Struct(a), Struct(b)) => a.equivalent(b),
            (Enum(a), Enum(b)) => a.equivalent(b),
            _ => false,
        }
    }
}

impl From<&MoveTypeLayout> for TypeTag {
    fn from(layout: &MoveTypeLayout) -> Self {
        TypeTag::from(layout.as_view())
    }
}

impl From<MoveTypeLayout> for TypeTag {
    fn from(layout: MoveTypeLayout) -> Self {
        TypeTag::from(layout.as_view())
    }
}

impl From<MoveLayoutView> for TypeTag {
    fn from(view: MoveLayoutView) -> TypeTag {
        match view {
            MoveLayoutView::Bool => TypeTag::Bool,
            MoveLayoutView::U8 => TypeTag::U8,
            MoveLayoutView::U16 => TypeTag::U16,
            MoveLayoutView::U32 => TypeTag::U32,
            MoveLayoutView::U64 => TypeTag::U64,
            MoveLayoutView::U128 => TypeTag::U128,
            MoveLayoutView::U256 => TypeTag::U256,
            MoveLayoutView::Address => TypeTag::Address,
            MoveLayoutView::Signer => TypeTag::Signer,
            MoveLayoutView::Vector(vv) => TypeTag::Vector(Box::new(TypeTag::from(vv.as_view()))),
            MoveLayoutView::Struct(sv) => TypeTag::Struct(Box::new(sv.type_().clone())),
            MoveLayoutView::Enum(ev) => TypeTag::Struct(Box::new(ev.type_().clone())),
        }
    }
}

impl fmt::Display for MoveLayoutView {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MoveLayoutView::Bool => write!(f, "bool"),
            MoveLayoutView::U8 => write!(f, "u8"),
            MoveLayoutView::U16 => write!(f, "u16"),
            MoveLayoutView::U32 => write!(f, "u32"),
            MoveLayoutView::U64 => write!(f, "u64"),
            MoveLayoutView::U128 => write!(f, "u128"),
            MoveLayoutView::U256 => write!(f, "u256"),
            MoveLayoutView::Address => write!(f, "address"),
            MoveLayoutView::Signer => write!(f, "signer"),
            MoveLayoutView::Vector(vv) if f.alternate() => write!(f, "vector<{:#}>", vv),
            MoveLayoutView::Vector(vv) => write!(f, "vector<{}>", vv),
            MoveLayoutView::Struct(sv) if f.alternate() => write!(f, "{sv:#}"),
            MoveLayoutView::Struct(sv) => write!(f, "{sv}"),
            MoveLayoutView::Enum(ev) if f.alternate() => write!(f, "{ev:#}"),
            MoveLayoutView::Enum(ev) => write!(f, "{ev}"),
        }
    }
}

// --- MoveDatatypeLayout ---

impl MoveDatatypeLayout {
    /// Wrap a `MoveTypeLayout` that is known to be a struct or enum.
    /// Returns `None` if the layout is a primitive or vector.
    pub fn new(layout: MoveTypeLayout) -> Option<Self> {
        match layout.as_view() {
            MoveLayoutView::Struct(struct_layout) => Some(MoveDatatypeLayout {
                self_layout: layout,
                inner: MoveDatatypeLayout_::Struct(struct_layout),
            }),
            MoveLayoutView::Enum(enum_layout) => Some(MoveDatatypeLayout {
                self_layout: layout,
                inner: MoveDatatypeLayout_::Enum(enum_layout),
            }),

            _ => None,
        }
    }

    /// Convert into the underlying `MoveTypeLayout`.
    pub fn into_layout(self) -> MoveTypeLayout {
        self.self_layout
    }

    /// Borrow the underlying `MoveTypeLayout`.
    pub fn as_layout(&self) -> &MoveTypeLayout {
        &self.self_layout
    }

    /// Create a view for navigating this layout.
    pub fn as_view(&self) -> MoveLayoutView {
        self.self_layout.as_view()
    }

    /// Returns `true` iff `self` and `other` describe the same datatype,
    /// regardless of pool ordering.
    pub fn equivalent(&self, other: &Self) -> bool {
        self.self_layout.equivalent(&other.self_layout)
    }

    pub fn as_inner(&self) -> &MoveDatatypeLayout_ {
        &self.inner
    }

    pub fn into_inner(self) -> MoveDatatypeLayout_ {
        self.inner
    }

    /// Inflate back into a tree-based [`AV::MoveDatatypeLayout`].
    pub fn inflate(&self) -> AResult<crate::annotated_value::MoveDatatypeLayout> {
        match &self.inner {
            MoveDatatypeLayout_::Struct(move_struct_layout) => Ok(AV::MoveDatatypeLayout::Struct(
                Box::new(AV::MoveStructLayout {
                    type_: (*move_struct_layout.type_).clone(),
                    fields: move_struct_layout
                        .fields()
                        .map(|(name, layout)| {
                            Ok(AV::MoveFieldLayout {
                                name: (**name).clone(),
                                layout: layout.inflate()?,
                            })
                        })
                        .collect::<AResult<_>>()?,
                }),
            )),
            MoveDatatypeLayout_::Enum(move_enum_layout) => {
                let variants = move_enum_layout
                    .variants()
                    .map(|vl| match vl {
                        VariantLayout::Known { name, tag, fields } => {
                            let field_layouts = fields
                                .fields()
                                .map(|(name, layout)| {
                                    Ok(AV::MoveFieldLayout {
                                        name: (**name).clone(),
                                        layout: layout.inflate()?,
                                    })
                                })
                                .collect::<AResult<_>>()?;
                            Ok((((*name).clone(), tag), field_layouts))
                        }
                        VariantLayout::Unknown { name, tag } => anyhow::bail!(
                            "cannot inflate enum with unknown variant layout: {} (tag {})",
                            name,
                            tag
                        ),
                    })
                    .collect::<AResult<_>>()?;
                Ok(AV::MoveDatatypeLayout::Enum(Box::new(AV::MoveEnumLayout {
                    type_: (*move_enum_layout.type_).clone(),
                    variants,
                })))
            }
        }
    }
}

// --- MoveFieldsLayout ---

impl MoveFieldsLayout {
    /// Number of fields.
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    /// Access a field by index, returning `(name, layout)`. The name is
    /// borrowed as `&Arc<Identifier>` — readers pay nothing, callers that
    /// need an owned name `Arc::clone` for a refcount bump.
    pub fn field(&self, i: u16) -> Option<(&Arc<Identifier>, MoveTypeLayout)> {
        self.fields.get(i as usize).map(|entry| {
            (
                &entry.name,
                MoveTypeLayout {
                    pool: self.pool.clone(),
                    root: entry.layout,
                },
            )
        })
    }

    /// Look up a field by name, returning its layout.
    pub fn field_by_name(&self, name: &str) -> Option<MoveTypeLayout> {
        self.fields
            .iter()
            .find(|entry| entry.name.as_str() == name)
            .map(|entry| MoveTypeLayout {
                pool: self.pool.clone(),
                root: entry.layout,
            })
    }

    /// Iterate over all fields as `(name, layout)` pairs. The name is
    /// borrowed as `&Arc<Identifier>` for zero-cost iteration.
    pub fn fields(&self) -> impl ExactSizeIterator<Item = (&Arc<Identifier>, MoveTypeLayout)> + '_ {
        let pool = &self.pool;
        self.fields.iter().map(move |entry| {
            (
                &entry.name,
                MoveTypeLayout {
                    pool: pool.clone(),
                    root: entry.layout,
                },
            )
        })
    }

    /// Returns `true` iff the two field-lists describe the same fields
    /// (same arity, pairwise-equal names, equivalent layouts), regardless
    /// of pool ordering.
    pub fn equivalent(&self, other: &Self) -> bool {
        if Arc::ptr_eq(&self.pool, &other.pool) && self.fields == other.fields {
            return true;
        }
        if self.fields.len() != other.fields.len() {
            return false;
        }
        let mut memo = HashSet::new();
        self.fields.iter().zip(other.fields.iter()).all(|(a, b)| {
            a.name == b.name
                && nodes_equivalent(&self.pool, a.layout, &other.pool, b.layout, &mut memo)
        })
    }
}

// --- MoveStructLayout ---

impl MoveStructLayout {
    /// The struct's type tag.
    pub fn type_(&self) -> &StructTag {
        &self.type_
    }

    /// Check whether this struct's type tag matches the given [`TypeTag`].
    pub fn is_type(&self, t: &TypeTag) -> bool {
        matches!(t, TypeTag::Struct(s) if &**s == self.type_())
    }

    /// Access the fields layout.
    pub fn fields_layout(&self) -> &MoveFieldsLayout {
        &self.fields
    }

    /// Number of fields.
    pub fn field_count(&self) -> usize {
        self.fields.field_count()
    }

    /// Access a field by index, returning `(name, layout)`.
    pub fn field(&self, i: u16) -> Option<(&Arc<Identifier>, MoveTypeLayout)> {
        self.fields.field(i)
    }

    /// Iterate over all fields as `(name, layout)` pairs.
    pub fn fields(&self) -> impl ExactSizeIterator<Item = (&Arc<Identifier>, MoveTypeLayout)> + '_ {
        self.fields.fields()
    }

    /// Returns `true` iff `self` and `other` describe the same struct type,
    /// regardless of pool ordering.
    pub fn equivalent(&self, other: &Self) -> bool {
        self.type_ == other.type_ && self.fields.equivalent(&other.fields)
    }
}

impl fmt::Display for MoveStructLayout {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use AV::DebugAsDisplay as DD;
        write!(f, "struct ")?;
        write!(f, "{} ", self.type_)?;
        let mut map = f.debug_map();
        for (name, layout) in self.fields() {
            map.entry(&DD(&**name), &DD(&layout));
        }
        map.finish()
    }
}

// --- VariantLayout ---

impl VariantLayout {
    /// The variant's name.
    pub fn name(&self) -> &Identifier {
        match self {
            VariantLayout::Known { name, .. } => name,
            VariantLayout::Unknown { name, .. } => name,
        }
    }

    /// The variant's name as a shared `Arc<Identifier>` — cheap to clone.
    pub fn name_arc(&self) -> &Arc<Identifier> {
        match self {
            VariantLayout::Known { name, .. } => name,
            VariantLayout::Unknown { name, .. } => name,
        }
    }

    /// The variant's tag.
    pub fn tag(&self) -> VariantTag {
        match self {
            VariantLayout::Known { tag, .. } => *tag,
            VariantLayout::Unknown { tag, .. } => *tag,
        }
    }

    /// The variant's fields, or `None` if the layout is unknown.
    pub fn fields(&self) -> Option<&MoveFieldsLayout> {
        match self {
            VariantLayout::Known { fields, .. } => Some(fields),
            VariantLayout::Unknown { .. } => None,
        }
    }
}

// --- MoveEnumLayout ---

impl MoveEnumLayout {
    /// The enum's type tag.
    pub fn type_(&self) -> &StructTag {
        &self.type_
    }

    /// Check whether this enum's type tag matches the given [`TypeTag`].
    pub fn is_type(&self, t: &TypeTag) -> bool {
        matches!(t, TypeTag::Struct(s) if **s == *self.type_())
    }

    /// Number of variants.
    pub fn variant_count(&self) -> usize {
        self.variants.len()
    }

    /// Access a variant by position index. Materializes the [`VariantLayout`]
    /// on demand from the underlying pool entry.
    pub fn variant(&self, i: VariantTag) -> Option<VariantLayout> {
        self.variants
            .get(i as usize)
            .map(|entry| materialize_variant(&self.pool, entry))
    }

    /// Find a variant by its tag value. Materializes the [`VariantLayout`]
    /// on demand for only the matched entry — wide enums no longer pay an
    /// O(n) materialization just to look up by tag.
    pub fn variant_by_tag(&self, tag: VariantTag) -> Option<VariantLayout> {
        self.variants
            .iter()
            .find(|entry| entry.tag == tag)
            .map(|entry| materialize_variant(&self.pool, entry))
    }

    /// Iterate over all variants. Each yielded [`VariantLayout`] is
    /// materialized on demand.
    pub fn variants(&self) -> impl ExactSizeIterator<Item = VariantLayout> + '_ {
        let pool = &self.pool;
        self.variants
            .iter()
            .map(move |entry| materialize_variant(pool, entry))
    }

    /// Returns `true` iff `self` and `other` describe the same enum type,
    /// regardless of pool ordering. Variants must match positionally with
    /// equal names+tags and equivalent fields when both are `Known`.
    pub fn equivalent(&self, other: &Self) -> bool {
        if self.type_ != other.type_ || self.variants.len() != other.variants.len() {
            return false;
        }
        let mut memo = HashSet::new();
        self.variants
            .iter()
            .zip(other.variants.iter())
            .all(|(a, b)| {
                a.name == b.name
                    && a.tag == b.tag
                    && match (&a.fields, &b.fields) {
                        (None, None) => true,
                        (Some(fa), Some(fb)) => {
                            fields_equivalent(&self.pool, fa, &other.pool, fb, &mut memo)
                        }
                        _ => false,
                    }
            })
    }
}

/// Materialize a single [`VariantLayout`] from a pool entry.
/// All clones are `Arc` bumps — no `Identifier` realloc.
fn materialize_variant(
    pool: &Arc<MoveTypeLayoutPool>,
    entry: &AnnotatedVariantEntry,
) -> VariantLayout {
    match &entry.fields {
        Some(fields) => VariantLayout::Known {
            name: Arc::clone(&entry.name),
            tag: entry.tag,
            fields: MoveFieldsLayout {
                pool: pool.clone(),
                fields: fields.clone(),
            },
        },
        None => VariantLayout::Unknown {
            name: Arc::clone(&entry.name),
            tag: entry.tag,
        },
    }
}

impl fmt::Display for MoveEnumLayout {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use AV::DebugAsDisplay as DD;
        write!(f, "enum {} ", self.type_)?;
        // Match tree-form `MoveEnumLayout`'s display, which keys variants in a
        // `BTreeMap<(Identifier, u16), _>` and so iterates them sorted by
        // (name, tag).
        let mut sorted: Vec<VariantLayout> = self.variants().collect();
        sorted.sort_by(|a, b| (a.name(), a.tag()).cmp(&(b.name(), b.tag())));
        let mut vmap = f.debug_set();
        for vl in &sorted {
            vmap.entry(&DD(&MoveVariantDisplay(vl)));
        }
        vmap.finish()
    }
}

struct MoveVariantDisplay<'a>(&'a VariantLayout);

impl fmt::Display for MoveVariantDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use AV::DebugAsDisplay as DD;
        let name = self.0.name().as_str();
        match self.0.fields() {
            Some(fields) => {
                let mut map = f.debug_struct(name);
                for (fname, layout) in fields.fields() {
                    map.field(fname.as_str(), &DD(&layout));
                }
                map.finish()
            }
            None => write!(f, "{name}(?)"),
        }
    }
}

// --- MoveTypeLayoutBuilder ---

impl MoveTypeLayoutBuilder {
    pub fn new() -> Self {
        Self {
            nodes: IndexSet::new(),
        }
    }

    fn add_node(&mut self, node: MoveTypeNode) -> AResult<LayoutHandle> {
        let (idx, _) = self.nodes.insert_full(node);
        Ok(LayoutHandle(LayoutRef::index(idx)?))
    }

    pub fn bool(&mut self) -> LayoutHandle {
        LayoutHandle(LayoutRef::leaf(LeafType::Bool))
    }
    pub fn u8(&mut self) -> LayoutHandle {
        LayoutHandle(LayoutRef::leaf(LeafType::U8))
    }
    pub fn u16(&mut self) -> LayoutHandle {
        LayoutHandle(LayoutRef::leaf(LeafType::U16))
    }
    pub fn u32(&mut self) -> LayoutHandle {
        LayoutHandle(LayoutRef::leaf(LeafType::U32))
    }
    pub fn u64(&mut self) -> LayoutHandle {
        LayoutHandle(LayoutRef::leaf(LeafType::U64))
    }
    pub fn u128(&mut self) -> LayoutHandle {
        LayoutHandle(LayoutRef::leaf(LeafType::U128))
    }
    pub fn u256(&mut self) -> LayoutHandle {
        LayoutHandle(LayoutRef::leaf(LeafType::U256))
    }
    pub fn address(&mut self) -> LayoutHandle {
        LayoutHandle(LayoutRef::leaf(LeafType::Address))
    }
    pub fn signer(&mut self) -> LayoutHandle {
        LayoutHandle(LayoutRef::leaf(LeafType::Signer))
    }

    pub fn vector(&mut self, element: LayoutHandle) -> AResult<LayoutHandle> {
        self.add_node(MoveTypeNode::Vector(element.0))
    }

    /// Build a struct layout node.
    /// `fields` is a list of (field_name, field_layout) pairs.
    pub fn struct_layout(
        &mut self,
        type_tag: StructTag,
        fields: Vec<(Identifier, LayoutHandle)>,
    ) -> AResult<LayoutHandle> {
        let fields: Arc<[AnnotatedFieldEntry]> = fields
            .into_iter()
            .map(|(name, h)| AnnotatedFieldEntry {
                name: Arc::new(name),
                layout: h.0,
            })
            .collect();
        self.add_node(MoveTypeNode::Struct(MoveStructNode {
            type_: Arc::new(type_tag),
            fields,
        }))
    }

    /// Build an enum layout node.
    /// Each variant is `(variant_name, tag, fields)` where fields is
    /// `None` for unknown layout or `Some(&[(field_name, layout)])` for known.
    pub fn enum_layout(
        &mut self,
        type_tag: StructTag,
        variants: Vec<(
            Identifier,
            VariantTag,
            Option<Vec<(Identifier, LayoutHandle)>>,
        )>,
    ) -> AResult<LayoutHandle> {
        let variant_entries: Arc<[AnnotatedVariantEntry]> = variants
            .into_iter()
            .map(|(vn, tag, fields_opt)| {
                let fields = fields_opt.map(|fields| {
                    fields
                        .into_iter()
                        .map(|(fn_name, h)| AnnotatedFieldEntry {
                            name: Arc::new(fn_name),
                            layout: h.0,
                        })
                        .collect()
                });
                AnnotatedVariantEntry {
                    name: Arc::new(vn),
                    tag,
                    fields,
                }
            })
            .collect();
        self.add_node(MoveTypeNode::Enum(MoveEnumNode {
            type_: Arc::new(type_tag),
            variants: variant_entries,
        }))
    }

    /// Recursively intern a tree-based annotated struct layout, deduplicating
    /// shared subtrees.
    pub fn from_tree_struct_layout(
        &mut self,
        layout: &AV::MoveStructLayout,
    ) -> AResult<LayoutHandle> {
        let fields = layout
            .fields
            .iter()
            .map(|f| Ok((f.name.clone(), self.from_tree(&f.layout)?)))
            .collect::<AResult<Vec<_>>>()?;
        self.struct_layout(layout.type_.clone(), fields)
    }

    /// Recursively intern a tree-based annotated layout.
    /// Tree-based enum layouts always have known variants, so all variants
    /// are wrapped in `Some`.
    pub fn from_tree(&mut self, layout: &AV::MoveTypeLayout) -> AResult<LayoutHandle> {
        Ok(match layout {
            AV::MoveTypeLayout::Bool => self.bool(),
            AV::MoveTypeLayout::U8 => self.u8(),
            AV::MoveTypeLayout::U16 => self.u16(),
            AV::MoveTypeLayout::U32 => self.u32(),
            AV::MoveTypeLayout::U64 => self.u64(),
            AV::MoveTypeLayout::U128 => self.u128(),
            AV::MoveTypeLayout::U256 => self.u256(),
            AV::MoveTypeLayout::Address => self.address(),
            AV::MoveTypeLayout::Signer => self.signer(),
            AV::MoveTypeLayout::Vector(inner) => {
                let inner_h = self.from_tree(inner)?;
                self.vector(inner_h)?
            }
            AV::MoveTypeLayout::Struct(s) => self.from_tree_struct_layout(s)?,
            AV::MoveTypeLayout::Enum(e) => {
                let variants = e
                    .variants
                    .iter()
                    .map(|((variant_name, tag), field_layouts)| {
                        let fields: Vec<(Identifier, LayoutHandle)> = field_layouts
                            .iter()
                            .map(|f| Ok((f.name.clone(), self.from_tree(&f.layout)?)))
                            .collect::<AResult<_>>()?;
                        Ok((variant_name.clone(), *tag, Some(fields)))
                    })
                    .collect::<AResult<Vec<_>>>()?;
                self.enum_layout(e.type_.clone(), variants)?
            }
        })
    }

    /// Recursively absorb an existing compressed layout into this builder,
    /// deduplicating shared subtrees against the builder's pool.
    pub fn from_layout(&mut self, layout: &MoveTypeLayout) -> AResult<LayoutHandle> {
        self.intern_view(layout.as_view())
    }

    fn intern_view(&mut self, view: MoveLayoutView) -> AResult<LayoutHandle> {
        Ok(match view {
            MoveLayoutView::Bool => self.bool(),
            MoveLayoutView::U8 => self.u8(),
            MoveLayoutView::U16 => self.u16(),
            MoveLayoutView::U32 => self.u32(),
            MoveLayoutView::U64 => self.u64(),
            MoveLayoutView::U128 => self.u128(),
            MoveLayoutView::U256 => self.u256(),
            MoveLayoutView::Address => self.address(),
            MoveLayoutView::Signer => self.signer(),
            MoveLayoutView::Vector(inner) => {
                let inner_h = self.from_layout(&inner)?;
                self.vector(inner_h)?
            }
            MoveLayoutView::Struct(s) => {
                let fields: Vec<(Identifier, LayoutHandle)> = s
                    .fields()
                    .map(|(name, layout)| Ok(((**name).clone(), self.from_layout(&layout)?)))
                    .collect::<AResult<_>>()?;
                self.struct_layout(s.type_().clone(), fields)?
            }
            MoveLayoutView::Enum(e) => {
                let variants: Vec<(
                    Identifier,
                    VariantTag,
                    Option<Vec<(Identifier, LayoutHandle)>>,
                )> = e
                    .variants()
                    .map(|v| match v {
                        VariantLayout::Known { name, tag, fields } => {
                            let fs: Vec<(Identifier, LayoutHandle)> = fields
                                .fields()
                                .map(|(n, l)| Ok(((**n).clone(), self.from_layout(&l)?)))
                                .collect::<AResult<_>>()?;
                            Ok(((*name).clone(), tag, Some(fs)))
                        }
                        VariantLayout::Unknown { name, tag } => Ok(((*name).clone(), tag, None)),
                    })
                    .collect::<AResult<_>>()?;
                self.enum_layout(e.type_().clone(), variants)?
            }
        })
    }

    /// Finalize the builder into an immutable [`MoveTypeLayout`].
    pub fn build(self, root: LayoutHandle) -> MoveTypeLayout {
        let nodes: Vec<MoveTypeNode> = self.nodes.into_iter().collect();
        MoveTypeLayout {
            pool: Arc::from(nodes),
            root: root.0,
        }
    }

    pub fn with_builder<F, E>(f: F) -> Result<MoveTypeLayout, E>
    where
        F: FnOnce(&mut Self) -> Result<LayoutHandle, E>,
    {
        let mut builder = Self::new();
        let result = f(&mut builder)?;
        Ok(builder.build(result))
    }

    pub fn type_tag(&self, handle: LayoutHandle) -> Option<TypeTag> {
        self.type_tag_of_ref(handle.0)
    }

    fn type_tag_of_ref(&self, r: LayoutRef) -> Option<TypeTag> {
        Some(match r.resolve() {
            ResolvedRef::Leaf(leaf) => match leaf {
                LeafType::Bool => TypeTag::Bool,
                LeafType::U8 => TypeTag::U8,
                LeafType::U16 => TypeTag::U16,
                LeafType::U32 => TypeTag::U32,
                LeafType::U64 => TypeTag::U64,
                LeafType::U128 => TypeTag::U128,
                LeafType::U256 => TypeTag::U256,
                LeafType::Address => TypeTag::Address,
                LeafType::Signer => TypeTag::Signer,
            },
            ResolvedRef::Index(idx) => match self.nodes.get_index(idx)? {
                MoveTypeNode::Vector(inner) => {
                    TypeTag::Vector(Box::new(self.type_tag_of_ref(*inner)?))
                }
                MoveTypeNode::Struct(s) => TypeTag::Struct(Box::new((*s.type_).clone())),
                MoveTypeNode::Enum(e) => TypeTag::Struct(Box::new((*e.type_).clone())),
            },
        })
    }
}

impl Default for MoveTypeLayoutBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Free functions
// =============================================================================

fn leaf_to_layout_view(leaf: LeafType) -> MoveLayoutView {
    match leaf {
        LeafType::Bool => MoveLayoutView::Bool,
        LeafType::U8 => MoveLayoutView::U8,
        LeafType::U16 => MoveLayoutView::U16,
        LeafType::U32 => MoveLayoutView::U32,
        LeafType::U64 => MoveLayoutView::U64,
        LeafType::U128 => MoveLayoutView::U128,
        LeafType::U256 => MoveLayoutView::U256,
        LeafType::Address => MoveLayoutView::Address,
        LeafType::Signer => MoveLayoutView::Signer,
    }
}

/// Resolve a [`LayoutRef`] against the pool into a [`MoveLayoutView`].
///
/// Panics if the reference points to an out-of-bounds table index.
fn resolve_ref(pool: &Arc<MoveTypeLayoutPool>, r: LayoutRef) -> MoveLayoutView {
    match r.resolve() {
        ResolvedRef::Leaf(leaf) => leaf_to_layout_view(leaf),
        ResolvedRef::Index(idx) => match &pool[idx] {
            MoveTypeNode::Vector(inner) => MoveLayoutView::Vector(MoveTypeLayout {
                pool: pool.clone(),
                root: *inner,
            }),
            // Three Arc bumps (no deep StructTag clone, no field allocation).
            MoveTypeNode::Struct(s) => MoveLayoutView::Struct(MoveStructLayout {
                type_: Arc::clone(&s.type_),
                fields: MoveFieldsLayout {
                    pool: pool.clone(),
                    fields: s.fields.clone(),
                },
            }),
            // Three Arc bumps and zero per-variant work — variants are
            // materialized only when accessed via the methods on `MoveEnumLayout`.
            MoveTypeNode::Enum(e) => MoveLayoutView::Enum(MoveEnumLayout {
                type_: Arc::clone(&e.type_),
                pool: pool.clone(),
                variants: e.variants.clone(),
            }),
        },
    }
}

/// Recursively check whether the nodes reachable from `ref_a` in `pool_a`
/// describe the same Move type as those reachable from `ref_b` in `pool_b`.
///
/// `memo` records `(ref_a, ref_b)` pairs already proven equivalent — preventing
/// exponential work on DAGs with shared subtrees, and defending against any
/// future cyclic builder.
fn nodes_equivalent(
    pool_a: &MoveTypeLayoutPool,
    ref_a: LayoutRef,
    pool_b: &MoveTypeLayoutPool,
    ref_b: LayoutRef,
    memo: &mut HashSet<(LayoutRef, LayoutRef)>,
) -> bool {
    match (ref_a.resolve(), ref_b.resolve()) {
        (ResolvedRef::Leaf(la), ResolvedRef::Leaf(lb)) => la == lb,
        (ResolvedRef::Index(ia), ResolvedRef::Index(ib)) => {
            if !memo.insert((ref_a, ref_b)) {
                return true;
            }
            match (&pool_a[ia], &pool_b[ib]) {
                (MoveTypeNode::Vector(ea), MoveTypeNode::Vector(eb)) => {
                    nodes_equivalent(pool_a, *ea, pool_b, *eb, memo)
                }
                (MoveTypeNode::Struct(sa), MoveTypeNode::Struct(sb)) => {
                    sa.type_ == sb.type_
                        && fields_equivalent(pool_a, &sa.fields, pool_b, &sb.fields, memo)
                }
                (MoveTypeNode::Enum(ea), MoveTypeNode::Enum(eb)) => {
                    ea.type_ == eb.type_
                        && ea.variants.len() == eb.variants.len()
                        && ea.variants.iter().zip(eb.variants.iter()).all(|(va, vb)| {
                            va.name == vb.name
                                && va.tag == vb.tag
                                && match (&va.fields, &vb.fields) {
                                    (None, None) => true,
                                    (Some(fa), Some(fb)) => {
                                        fields_equivalent(pool_a, fa, pool_b, fb, memo)
                                    }
                                    _ => false,
                                }
                        })
                }
                _ => false,
            }
        }
        _ => false,
    }
}

/// Compare two field lists for structural equivalence (matching arity, names,
/// and recursively-equivalent layouts).
fn fields_equivalent(
    pool_a: &MoveTypeLayoutPool,
    fields_a: &[AnnotatedFieldEntry],
    pool_b: &MoveTypeLayoutPool,
    fields_b: &[AnnotatedFieldEntry],
    memo: &mut HashSet<(LayoutRef, LayoutRef)>,
) -> bool {
    fields_a.len() == fields_b.len()
        && fields_a.iter().zip(fields_b.iter()).all(|(a, b)| {
            a.name == b.name && nodes_equivalent(pool_a, a.layout, pool_b, b.layout, memo)
        })
}
