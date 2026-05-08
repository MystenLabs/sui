// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Experimental annotated type-layout representation — an alternative to
//! the canonical [`super::layout`] / [`super::ref_layout`] families that
//! reorganizes the pool to test whether a denser, more uniformly-indexed
//! representation traverses faster.
//!
//! All public types and traits are prefixed with `Exp` to avoid colliding
//! with the canonical family in the same module.
//!
//! ## How the pool differs from [`super::layout`]
//!
//! | Concern                       | Canonical ([`super::layout`])                                 | Experimental (this module)                                 |
//! |-------------------------------|---------------------------------------------------------------|------------------------------------------------------------|
//! | Pool shape                    | `Arc<[MoveTypeNode]>` — single vec of compound nodes          | [`ExpMoveTypePool`] — struct of vecs (`structs`, `enums`, `variants`, `fields`, `vectors`) |
//! | Field/variant lists           | Each struct/variant owns an `Arc<[…Entry]>` of inline entries | Each struct/variant carries a `Range<u16>` into a shared, dense `fields`/`variants` vec |
//! | Reference encoding            | `LayoutRef`: `u16` with 1-bit leaf tag + 15-bit index         | [`ExpLayoutRef`]: packed `u16` with 2-bit pool tag + 14-bit index, leaves encoded specially |
//! | Cardinality cap               | 32K compound nodes per pool                                   | 16K entries *per sub-pool* (structs / enums / variants / fields / vectors) |
//! | Owned type clone              | `Arc` refcount bump                                           | `Arc` refcount bump (the whole pool sits behind one `Arc`) |
//! | Borrowed type                 | [`super::ref_layout::MoveTypeLayoutRef`] — `(&pool, root)`    | [`ExpMoveTypeLayoutRef`] — wide enum, parallel `*Ref<'a>` types are `(&pool, u16)` pairs |
//! | Vectors                       | Stored inline as `MoveTypeNode::Vector(LayoutRef)`            | Element refs live in a dedicated `pool.vectors: Vec<ExpLayoutRef>`; `ExpMoveVectorLayoutRef` wraps `(&pool, idx)` |
//!
//! ## How traversal differs
//!
//! Both representations achieve zero-allocation traversal in their borrowed
//! form. The structural cost difference is one extra pointer hop per
//! field/variant in the experimental version: descending into a struct
//! requires `pool.structs[idx] → field range → pool.fields[i] → resolve`,
//! versus the canonical layout's `pool[idx] → field array (inline) → resolve`.
//! In benchmarks this shows up as a 0–60% slowdown on deeply-nested or
//! struct-heavy shapes; clones and pointer-Copy of the borrowed type are
//! marginally faster.
//!
//! ## When to use this
//!
//! Right now: **only for benchmarking and design exploration.** This module
//! is not wired into the visitor framework, has no `Display`/`inflate`
//! parity beyond `Display`, and lacks dedup. The canonical
//! [`super::layout`] (+ [`super::ref_layout`]) family is what production
//! code should use.
//!
//! Construction is via `TryFrom<&AV::MoveTypeLayout>` — convert from a
//! tree-form annotated layout when you want to compare paths.

use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use crate::annotated_value as AV;
use crate::compressed::{LeafType, VariantTag};
use crate::identifier::Identifier;
use crate::language_storage::StructTag;
use anyhow::Result as AResult;

// =============================================================================
// Internal storage
// =============================================================================

/// Packed reference: 2-bit pool tag + 14-bit payload.
///
/// Encoding:
///   * tag `0b00` (leaf)   — payload = `LeafType` discriminant.
///   * tag `0b01` (struct) — payload = index into `pool.structs`.
///   * tag `0b10` (enum)   — payload = index into `pool.enums`.
///   * tag `0b11` (vector) — payload = index into `pool.vectors`.
///
/// `Copy`; comparable by raw bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExpLayoutRef(u16);

const TAG_SHIFT: u16 = 14;
const IDX_MASK: u16 = (1 << TAG_SHIFT) - 1;
const TAG_LEAF: u16 = 0b00 << TAG_SHIFT;
const TAG_STRUCT: u16 = 0b01 << TAG_SHIFT;
const TAG_ENUM: u16 = 0b10 << TAG_SHIFT;
const TAG_VECTOR: u16 = 0b11 << TAG_SHIFT;

/// The maximum index addressable by [`ExpLayoutRef`]'s 14-bit payload (per
/// pool). Construction returns an error past this.
pub const MAX_POOL_INDEX: usize = (1 << TAG_SHIFT) - 1;

/// Resolved view of an [`ExpLayoutRef`] — produced cheaply, never allocates.
/// Internal only because the leaf payload type [`LeafType`] is `pub(crate)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResolvedExpLayoutRef {
    Leaf(LeafType),
    Struct(u16),
    Enum(u16),
    Vector(u16),
}

impl ExpLayoutRef {
    pub(crate) fn leaf(ty: LeafType) -> Self {
        ExpLayoutRef(TAG_LEAF | ty as u16)
    }

    pub(crate) fn struct_(idx: usize) -> AResult<Self> {
        if idx > MAX_POOL_INDEX {
            anyhow::bail!("exp annotated struct pool overflow: {idx}");
        }
        Ok(ExpLayoutRef(TAG_STRUCT | idx as u16))
    }

    pub(crate) fn enum_(idx: usize) -> AResult<Self> {
        if idx > MAX_POOL_INDEX {
            anyhow::bail!("exp annotated enum pool overflow: {idx}");
        }
        Ok(ExpLayoutRef(TAG_ENUM | idx as u16))
    }

    pub(crate) fn vector(idx: usize) -> AResult<Self> {
        if idx > MAX_POOL_INDEX {
            anyhow::bail!("exp annotated vector pool overflow: {idx}");
        }
        Ok(ExpLayoutRef(TAG_VECTOR | idx as u16))
    }

    /// Cheap, branch-and-mask — never allocates.
    pub(crate) fn resolve(self) -> ResolvedExpLayoutRef {
        let payload = self.0 & IDX_MASK;
        match self.0 & !IDX_MASK {
            TAG_LEAF => ResolvedExpLayoutRef::Leaf(
                LeafType::from_u8(payload as u8)
                    .expect("invalid leaf discriminant in ExpLayoutRef payload"),
            ),
            TAG_STRUCT => ResolvedExpLayoutRef::Struct(payload),
            TAG_ENUM => ResolvedExpLayoutRef::Enum(payload),
            TAG_VECTOR => ResolvedExpLayoutRef::Vector(payload),
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpMoveStructLayout {
    pub type_: StructTag,
    /// Range into [`ExpMoveTypePool::fields`].
    pub fields: Range<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpMoveEnumLayout {
    pub type_: StructTag,
    /// Range into [`ExpMoveTypePool::variants`].
    pub variants: Range<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpMoveVariantLayout {
    pub name: Identifier,
    pub tag: VariantTag,
    /// `None` represents an "unknown" variant — preserved for parity with
    /// the existing compressed annotated layout. `Some(range)` (possibly
    /// empty) means the variant's fields are known and stored at
    /// `pool.fields[range]`.
    pub fields: Option<Range<u16>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpMoveFieldLayout {
    pub name: Identifier,
    pub layout: ExpLayoutRef,
}

/// Flat-vec pool. All structural data lives in dense `Vec<_>`s for
/// cache-friendly traversal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExpMoveTypePool {
    pub structs: Vec<ExpMoveStructLayout>,
    pub enums: Vec<ExpMoveEnumLayout>,
    pub variants: Vec<ExpMoveVariantLayout>,
    pub fields: Vec<ExpMoveFieldLayout>,
    /// One [`ExpLayoutRef`] per vector — the element layout.
    pub vectors: Vec<ExpLayoutRef>,
}

/// Top-level layout: shared pool + root reference.
#[derive(Debug, Clone)]
pub struct ExpMoveTypeLayout {
    pool: Arc<ExpMoveTypePool>,
    root: ExpLayoutRef,
}

impl PartialEq for ExpMoveTypeLayout {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.pool, &other.pool) && self.root == other.root
    }
}
impl Eq for ExpMoveTypeLayout {}

impl ExpMoveTypeLayout {
    pub fn pool(&self) -> &ExpMoveTypePool {
        &self.pool
    }

    pub fn root(&self) -> ExpLayoutRef {
        self.root
    }

    pub fn as_layout_ref(&self) -> ExpMoveTypeLayoutRef<'_> {
        resolve_ref_borrowed(&self.pool, self.root)
    }

    /// Convenience: same as [`Self::as_layout_ref`].
    pub fn as_view_ref(&self) -> ExpMoveTypeLayoutRef<'_> {
        self.as_layout_ref()
    }
}

// =============================================================================
// Borrowed ("ref") view family
// =============================================================================
//
// All `Copy`. Every variant carries a `&'a ExpMoveTypePool` plus enough
// integer state to navigate. No `Arc` clones, no boxes, no allocation.

#[derive(Debug, Clone, Copy)]
pub enum ExpMoveTypeLayoutRef<'a> {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Address,
    Signer,
    Vector(ExpMoveVectorLayoutRef<'a>),
    Struct(ExpMoveStructLayoutRef<'a>),
    Enum(ExpMoveEnumLayoutRef<'a>),
}

#[derive(Debug, Clone, Copy)]
pub struct ExpMoveVectorLayoutRef<'a> {
    pool: &'a ExpMoveTypePool,
    idx: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct ExpMoveStructLayoutRef<'a> {
    pool: &'a ExpMoveTypePool,
    idx: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct ExpMoveEnumLayoutRef<'a> {
    pool: &'a ExpMoveTypePool,
    idx: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct ExpMoveVariantLayoutRef<'a> {
    pool: &'a ExpMoveTypePool,
    idx: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct ExpMoveFieldLayoutRef<'a> {
    pool: &'a ExpMoveTypePool,
    idx: u16,
}

// --- Vector ---

impl<'a> ExpMoveVectorLayoutRef<'a> {
    pub fn element(&self) -> ExpMoveTypeLayoutRef<'a> {
        let r = self.pool.vectors[self.idx as usize];
        resolve_ref_borrowed(self.pool, r)
    }
}

// --- Struct ---

impl<'a> ExpMoveStructLayoutRef<'a> {
    fn entry(&self) -> &'a ExpMoveStructLayout {
        &self.pool.structs[self.idx as usize]
    }

    pub fn type_(&self) -> &'a StructTag {
        &self.entry().type_
    }

    pub fn field_count(&self) -> usize {
        let r = &self.entry().fields;
        (r.end - r.start) as usize
    }

    pub fn fields(&self) -> ExpFields<'a> {
        ExpFields {
            pool: self.pool,
            range: self.entry().fields.clone(),
        }
    }

    pub fn field(&self, i: u16) -> Option<ExpMoveFieldLayoutRef<'a>> {
        let r = &self.entry().fields;
        let end = r.end;
        let abs = r.start.checked_add(i)?;
        if abs >= end {
            return None;
        }
        Some(ExpMoveFieldLayoutRef {
            pool: self.pool,
            idx: abs,
        })
    }

    pub fn field_by_name(&self, name: &str) -> Option<ExpMoveFieldLayoutRef<'a>> {
        let r = &self.entry().fields;
        for i in r.clone() {
            if self.pool.fields[i as usize].name.as_str() == name {
                return Some(ExpMoveFieldLayoutRef {
                    pool: self.pool,
                    idx: i,
                });
            }
        }
        None
    }
}

// --- Enum ---

impl<'a> ExpMoveEnumLayoutRef<'a> {
    fn entry(&self) -> &'a ExpMoveEnumLayout {
        &self.pool.enums[self.idx as usize]
    }

    pub fn type_(&self) -> &'a StructTag {
        &self.entry().type_
    }

    pub fn variant_count(&self) -> usize {
        let r = &self.entry().variants;
        (r.end - r.start) as usize
    }

    pub fn variants(&self) -> ExpVariants<'a> {
        ExpVariants {
            pool: self.pool,
            range: self.entry().variants.clone(),
        }
    }

    pub fn variant(&self, i: u16) -> Option<ExpMoveVariantLayoutRef<'a>> {
        let r = &self.entry().variants;
        let end = r.end;
        let abs = r.start.checked_add(i)?;
        if abs >= end {
            return None;
        }
        Some(ExpMoveVariantLayoutRef {
            pool: self.pool,
            idx: abs,
        })
    }

    pub fn variant_by_tag(&self, tag: VariantTag) -> Option<ExpMoveVariantLayoutRef<'a>> {
        let r = &self.entry().variants;
        for i in r.clone() {
            if self.pool.variants[i as usize].tag == tag {
                return Some(ExpMoveVariantLayoutRef {
                    pool: self.pool,
                    idx: i,
                });
            }
        }
        None
    }
}

// --- Variant ---

impl<'a> ExpMoveVariantLayoutRef<'a> {
    fn entry(&self) -> &'a ExpMoveVariantLayout {
        &self.pool.variants[self.idx as usize]
    }

    pub fn name(&self) -> &'a Identifier {
        &self.entry().name
    }

    pub fn tag(&self) -> VariantTag {
        self.entry().tag
    }

    /// `None` iff the variant's layout is unknown.
    pub fn fields(&self) -> Option<ExpFields<'a>> {
        self.entry().fields.as_ref().map(|range| ExpFields {
            pool: self.pool,
            range: range.clone(),
        })
    }

    pub fn field_count(&self) -> Option<usize> {
        self.entry()
            .fields
            .as_ref()
            .map(|r| (r.end - r.start) as usize)
    }
}

// --- Field ---

impl<'a> ExpMoveFieldLayoutRef<'a> {
    fn entry(&self) -> &'a ExpMoveFieldLayout {
        &self.pool.fields[self.idx as usize]
    }

    pub fn name(&self) -> &'a Identifier {
        &self.entry().name
    }

    pub fn layout(&self) -> ExpMoveTypeLayoutRef<'a> {
        resolve_ref_borrowed(self.pool, self.entry().layout)
    }
}

// =============================================================================
// Iterators
// =============================================================================

#[derive(Debug, Clone)]
pub struct ExpFields<'a> {
    pool: &'a ExpMoveTypePool,
    range: Range<u16>,
}

impl<'a> Iterator for ExpFields<'a> {
    type Item = ExpMoveFieldLayoutRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.range.start >= self.range.end {
            return None;
        }
        let idx = self.range.start;
        self.range.start += 1;
        Some(ExpMoveFieldLayoutRef {
            pool: self.pool,
            idx,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = (self.range.end - self.range.start) as usize;
        (n, Some(n))
    }
}

impl ExactSizeIterator for ExpFields<'_> {}

#[derive(Debug, Clone)]
pub struct ExpVariants<'a> {
    pool: &'a ExpMoveTypePool,
    range: Range<u16>,
}

impl<'a> Iterator for ExpVariants<'a> {
    type Item = ExpMoveVariantLayoutRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.range.start >= self.range.end {
            return None;
        }
        let idx = self.range.start;
        self.range.start += 1;
        Some(ExpMoveVariantLayoutRef {
            pool: self.pool,
            idx,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = (self.range.end - self.range.start) as usize;
        (n, Some(n))
    }
}

impl ExactSizeIterator for ExpVariants<'_> {}

// =============================================================================
// Resolution helpers
// =============================================================================

#[inline]
fn leaf_to_ref<'a>(leaf: LeafType) -> ExpMoveTypeLayoutRef<'a> {
    match leaf {
        LeafType::Bool => ExpMoveTypeLayoutRef::Bool,
        LeafType::U8 => ExpMoveTypeLayoutRef::U8,
        LeafType::U16 => ExpMoveTypeLayoutRef::U16,
        LeafType::U32 => ExpMoveTypeLayoutRef::U32,
        LeafType::U64 => ExpMoveTypeLayoutRef::U64,
        LeafType::U128 => ExpMoveTypeLayoutRef::U128,
        LeafType::U256 => ExpMoveTypeLayoutRef::U256,
        LeafType::Address => ExpMoveTypeLayoutRef::Address,
        LeafType::Signer => ExpMoveTypeLayoutRef::Signer,
    }
}

#[inline]
fn resolve_ref_borrowed<'a>(
    pool: &'a ExpMoveTypePool,
    r: ExpLayoutRef,
) -> ExpMoveTypeLayoutRef<'a> {
    match r.resolve() {
        ResolvedExpLayoutRef::Leaf(leaf) => leaf_to_ref(leaf),
        ResolvedExpLayoutRef::Struct(idx) => {
            ExpMoveTypeLayoutRef::Struct(ExpMoveStructLayoutRef { pool, idx })
        }
        ResolvedExpLayoutRef::Enum(idx) => {
            ExpMoveTypeLayoutRef::Enum(ExpMoveEnumLayoutRef { pool, idx })
        }
        ResolvedExpLayoutRef::Vector(idx) => {
            ExpMoveTypeLayoutRef::Vector(ExpMoveVectorLayoutRef { pool, idx })
        }
    }
}

// =============================================================================
// Construction (TryFrom tree-form annotated layout)
// =============================================================================

impl TryFrom<&AV::MoveTypeLayout> for ExpMoveTypeLayout {
    type Error = anyhow::Error;
    fn try_from(layout: &AV::MoveTypeLayout) -> Result<Self, Self::Error> {
        let mut pool = ExpMoveTypePool::default();
        let root = build_from_tree(&mut pool, layout)?;
        Ok(ExpMoveTypeLayout {
            pool: Arc::new(pool),
            root,
        })
    }
}

impl TryFrom<AV::MoveTypeLayout> for ExpMoveTypeLayout {
    type Error = anyhow::Error;
    fn try_from(layout: AV::MoveTypeLayout) -> Result<Self, Self::Error> {
        (&layout).try_into()
    }
}

fn build_from_tree(
    pool: &mut ExpMoveTypePool,
    layout: &AV::MoveTypeLayout,
) -> AResult<ExpLayoutRef> {
    Ok(match layout {
        AV::MoveTypeLayout::Bool => ExpLayoutRef::leaf(LeafType::Bool),
        AV::MoveTypeLayout::U8 => ExpLayoutRef::leaf(LeafType::U8),
        AV::MoveTypeLayout::U16 => ExpLayoutRef::leaf(LeafType::U16),
        AV::MoveTypeLayout::U32 => ExpLayoutRef::leaf(LeafType::U32),
        AV::MoveTypeLayout::U64 => ExpLayoutRef::leaf(LeafType::U64),
        AV::MoveTypeLayout::U128 => ExpLayoutRef::leaf(LeafType::U128),
        AV::MoveTypeLayout::U256 => ExpLayoutRef::leaf(LeafType::U256),
        AV::MoveTypeLayout::Address => ExpLayoutRef::leaf(LeafType::Address),
        AV::MoveTypeLayout::Signer => ExpLayoutRef::leaf(LeafType::Signer),

        AV::MoveTypeLayout::Vector(inner) => {
            let elem = build_from_tree(pool, inner)?;
            let idx = pool.vectors.len();
            pool.vectors.push(elem);
            ExpLayoutRef::vector(idx)?
        }

        AV::MoveTypeLayout::Struct(s) => {
            let field_refs: Vec<ExpLayoutRef> = s
                .fields
                .iter()
                .map(|f| build_from_tree(pool, &f.layout))
                .collect::<AResult<_>>()?;
            let names: Vec<Identifier> = s.fields.iter().map(|f| f.name.clone()).collect();

            let start = u16_try(pool.fields.len(), "fields")?;
            for (name, layout) in names.into_iter().zip(field_refs) {
                pool.fields.push(ExpMoveFieldLayout { name, layout });
            }
            let end = u16_try(pool.fields.len(), "fields")?;

            let idx = pool.structs.len();
            pool.structs.push(ExpMoveStructLayout {
                type_: s.type_.clone(),
                fields: start..end,
            });
            ExpLayoutRef::struct_(idx)?
        }

        AV::MoveTypeLayout::Enum(e) => {
            // Build all per-variant field layouts first (recursive), so that
            // the variants chunk is contiguous.
            struct PreVariant {
                name: Identifier,
                tag: VariantTag,
                field_entries: Vec<(Identifier, ExpLayoutRef)>,
            }
            let mut pre = Vec::with_capacity(e.variants.len());
            for ((vn, tag), field_layouts) in &e.variants {
                let mut entries = Vec::with_capacity(field_layouts.len());
                for f in field_layouts {
                    let r = build_from_tree(pool, &f.layout)?;
                    entries.push((f.name.clone(), r));
                }
                pre.push(PreVariant {
                    name: vn.clone(),
                    tag: *tag,
                    field_entries: entries,
                });
            }

            // Now allocate a contiguous variants chunk; each variant lays
            // its fields into a contiguous fields chunk.
            let v_start = u16_try(pool.variants.len(), "variants")?;
            for pv in pre {
                let f_start = u16_try(pool.fields.len(), "fields")?;
                for (name, layout) in pv.field_entries {
                    pool.fields.push(ExpMoveFieldLayout { name, layout });
                }
                let f_end = u16_try(pool.fields.len(), "fields")?;
                pool.variants.push(ExpMoveVariantLayout {
                    name: pv.name,
                    tag: pv.tag,
                    fields: Some(f_start..f_end),
                });
            }
            let v_end = u16_try(pool.variants.len(), "variants")?;

            let idx = pool.enums.len();
            pool.enums.push(ExpMoveEnumLayout {
                type_: e.type_.clone(),
                variants: v_start..v_end,
            });
            ExpLayoutRef::enum_(idx)?
        }
    })
}

fn u16_try(n: usize, what: &str) -> AResult<u16> {
    if n > u16::MAX as usize {
        anyhow::bail!("exp annotated {what} pool overflow: {n}");
    }
    Ok(n as u16)
}

// =============================================================================
// Display
// =============================================================================

impl fmt::Display for ExpMoveTypeLayout {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt_ref(self.as_layout_ref(), f)
    }
}

fn fmt_ref(view: ExpMoveTypeLayoutRef<'_>, f: &mut fmt::Formatter) -> fmt::Result {
    match view {
        ExpMoveTypeLayoutRef::Bool => write!(f, "bool"),
        ExpMoveTypeLayoutRef::U8 => write!(f, "u8"),
        ExpMoveTypeLayoutRef::U16 => write!(f, "u16"),
        ExpMoveTypeLayoutRef::U32 => write!(f, "u32"),
        ExpMoveTypeLayoutRef::U64 => write!(f, "u64"),
        ExpMoveTypeLayoutRef::U128 => write!(f, "u128"),
        ExpMoveTypeLayoutRef::U256 => write!(f, "u256"),
        ExpMoveTypeLayoutRef::Address => write!(f, "address"),
        ExpMoveTypeLayoutRef::Signer => write!(f, "signer"),
        ExpMoveTypeLayoutRef::Vector(v) => {
            write!(f, "vector<")?;
            fmt_ref(v.element(), f)?;
            write!(f, ">")
        }
        ExpMoveTypeLayoutRef::Struct(s) => {
            write!(f, "struct {} {{ ", s.type_())?;
            for (i, fld) in s.fields().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}: ", fld.name())?;
                fmt_ref(fld.layout(), f)?;
            }
            write!(f, " }}")
        }
        ExpMoveTypeLayoutRef::Enum(e) => {
            write!(f, "enum {} {{ ", e.type_())?;
            for (i, v) in e.variants().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", v.name())?;
                match v.fields() {
                    Some(fs) => {
                        write!(f, "(")?;
                        for (j, fld) in fs.enumerate() {
                            if j > 0 {
                                write!(f, ", ")?;
                            }
                            write!(f, "{}: ", fld.name())?;
                            fmt_ref(fld.layout(), f)?;
                        }
                        write!(f, ")")?;
                    }
                    None => write!(f, "(?)")?,
                }
            }
            write!(f, " }}")
        }
    }
}
