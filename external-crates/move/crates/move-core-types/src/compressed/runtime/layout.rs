// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::compressed::{LayoutRef, LeafType, ResolvedRef, VariantTag};
use crate::runtime_value as RV;
use anyhow::Result as AResult;
use indexmap::IndexSet;
use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

pub use crate::compressed::LayoutHandle;

static EMPTY_POOL: std::sync::LazyLock<Arc<MoveTypeLayoutPool>> =
    std::sync::LazyLock::new(|| Arc::from(Vec::<MoveTypeNode>::new()));

// =============================================================================
// Type declarations
// =============================================================================

// --- Node types (internal) ---

/// Struct layout node: field types stored as [`LayoutRef`]s.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MoveStructNode {
    pub(crate) fields: Arc<[LayoutRef]>,
}

/// Enum layout node: each variant is either a known list of field
/// [`LayoutRef`]s, or `None` indicating the variant exists but its
/// field layout is not available.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MoveEnumNode {
    pub(crate) variants: Arc<[Option<Arc<[LayoutRef]>>]>,
}

/// A compound layout node in the compressed node table.
/// Leaf types (primitives) are encoded inline in [`LayoutRef`] and never
/// appear in the table.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum MoveTypeNode {
    Vector(LayoutRef),
    Struct(MoveStructNode),
    Enum(MoveEnumNode),
}

/// The backing store of compound layout nodes.
pub(crate) type MoveTypeLayoutPool = [MoveTypeNode];

// --- Owned layout types ---

/// A deduplicated, flat representation of a [`RV::MoveTypeLayout`] tree.
/// Cloning is cheap — the pool is shared via `Arc`.
///
/// NOTE: `Eq`/`PartialEq` are implemented manually (delegating to
/// [`MoveTypeLayout::equivalent`]) rather than derived, because two layouts
/// representing the same type may have different pool orderings or sharing
/// patterns and structural equality on the raw fields would produce false
/// negatives. `Hash` is intentionally not implemented (no canonical form).
#[derive(Debug, Clone)]
pub struct MoveTypeLayout {
    pool: Arc<MoveTypeLayoutPool>,
    root: LayoutRef,
}

/// A resolved view of a layout node. Leaf types are unit variants;
/// compound types contain owned layout types for direct navigation.
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
    Vector(Box<MoveTypeLayout>),
    Struct(Box<MoveStructLayout>),
    Enum(Box<MoveEnumLayout>),
}

/// The layout of a Move datatype, which is either a struct or an enum.
#[derive(Debug, Clone)]
pub enum MoveDatatypeLayout {
    Struct(Box<MoveStructLayout>),
    Enum(Box<MoveEnumLayout>),
}

/// The enum layout of an enum type, as a view into a shared pool.
#[derive(Debug, Clone)]
pub struct MoveEnumLayout {
    pub(crate) variants: Arc<[VariantLayout]>,
}

/// The struct layout of a struct type, as a view into a shared pool.
#[derive(Debug, Clone)]
pub struct MoveStructLayout(pub MoveFieldsLayout);

/// The result of looking up a variant in an enum view.
#[derive(Debug, Clone)]
pub enum VariantLayout {
    /// The variant's field layout is known.
    Known(MoveFieldsLayout),
    /// The variant exists but its field layout is not available.
    Unknown,
}

/// The field layout of a struct or enum variant, as a view into a shared pool.
#[derive(Debug, Clone)]
pub struct MoveFieldsLayout {
    pool: Arc<MoveTypeLayoutPool>,
    fields: Arc<[LayoutRef]>,
}

// --- Builder type ---

/// Incrementally builds a [`MoveTypeLayout`] with automatic deduplication.
/// Leaf types are encoded inline in [`LayoutRef`] and never stored in the
/// node table.
pub struct MoveTypeLayoutBuilder {
    nodes: IndexSet<MoveTypeNode>,
}

// --- Display helper ---

/// Helper type that uses `T`'s `Display` implementation as its own `Debug` implementation,
/// to allow other `Display` implementations to take advantage of structured formatting helpers.
struct DebugAsDisplay<'a, T>(&'a T);

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

    /// Reconstruct the equivalent tree-based layout. Returns an error
    /// if any enum variant has an unknown layout.
    pub fn inflate(&self) -> AResult<RV::MoveTypeLayout> {
        self.as_view().inflate()
    }

    /// If this layout is a struct, return it. Otherwise `None`.
    pub fn into_struct(self) -> Option<MoveStructLayout> {
        match self.as_view() {
            MoveLayoutView::Struct(s) => Some(*s),
            _ => None,
        }
    }

    /// If this layout is an enum, return it. Otherwise `None`.
    pub fn into_enum(self) -> Option<MoveEnumLayout> {
        match self.as_view() {
            MoveLayoutView::Enum(e) => Some(*e),
            _ => None,
        }
    }

    /// Returns `true` iff `self` and `other` describe the same Move type,
    /// regardless of pool ordering or how subtrees are shared.
    pub fn equivalent(&self, other: &Self) -> bool {
        if Arc::ptr_eq(&self.pool, &other.pool) && self.root == other.root {
            return true;
        }
        let mut memo = HashSet::new();
        nodes_equivalent(&self.pool, self.root, &other.pool, other.root, &mut memo)
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

impl TryFrom<&RV::MoveTypeLayout> for MoveTypeLayout {
    type Error = anyhow::Error;
    fn try_from(layout: &RV::MoveTypeLayout) -> Result<Self, Self::Error> {
        let mut b = MoveTypeLayoutBuilder::new();
        let root = b.from_tree(layout)?;
        Ok(b.build(root))
    }
}

impl fmt::Display for MoveTypeLayout {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.as_view() {
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
            MoveLayoutView::Struct(fv) if f.alternate() => write!(f, "{:#}", &*fv),
            MoveLayoutView::Struct(fv) => write!(f, "{}", &*fv),
            MoveLayoutView::Enum(ev) if f.alternate() => write!(f, "{ev:#}"),
            MoveLayoutView::Enum(ev) => write!(f, "{ev}"),
        }
    }
}

// --- MoveLayoutView ---

impl MoveLayoutView {
    /// Reconstruct the equivalent tree-based layout. Returns an error
    /// if any enum variant has an unknown layout.
    pub fn inflate(&self) -> AResult<RV::MoveTypeLayout> {
        Ok(match self {
            MoveLayoutView::Bool => RV::MoveTypeLayout::Bool,
            MoveLayoutView::U8 => RV::MoveTypeLayout::U8,
            MoveLayoutView::U16 => RV::MoveTypeLayout::U16,
            MoveLayoutView::U32 => RV::MoveTypeLayout::U32,
            MoveLayoutView::U64 => RV::MoveTypeLayout::U64,
            MoveLayoutView::U128 => RV::MoveTypeLayout::U128,
            MoveLayoutView::U256 => RV::MoveTypeLayout::U256,
            MoveLayoutView::Address => RV::MoveTypeLayout::Address,
            MoveLayoutView::Signer => RV::MoveTypeLayout::Signer,
            MoveLayoutView::Vector(vv) => RV::MoveTypeLayout::Vector(Box::new(vv.inflate()?)),
            MoveLayoutView::Struct(fv) => {
                let fields = fv.0.fields().map(|f| f.inflate()).collect::<AResult<_>>()?;
                RV::MoveTypeLayout::Struct(Box::new(RV::MoveStructLayout::new(fields)))
            }
            MoveLayoutView::Enum(ev) => {
                let variants = ev
                    .variants()
                    .iter()
                    .map(|vfv| match vfv {
                        VariantLayout::Known(fv) => {
                            fv.fields().map(|f| f.inflate()).collect::<AResult<_>>()
                        }
                        VariantLayout::Unknown => {
                            anyhow::bail!("cannot inflate enum with unknown variant layout")
                        }
                    })
                    .collect::<AResult<_>>()?;
                RV::MoveTypeLayout::Enum(Box::new(RV::MoveEnumLayout(Box::new(variants))))
            }
        })
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

// --- MoveFieldsLayout ---

impl MoveFieldsLayout {
    /// Number of fields.
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    /// Access a field by index.
    pub fn field(&self, i: u16) -> Option<MoveTypeLayout> {
        self.fields.get(i as usize).map(|f| MoveTypeLayout {
            pool: self.pool.clone(),
            root: *f,
        })
    }

    /// Iterate over all fields as layout views.
    pub fn fields(&self) -> impl ExactSizeIterator<Item = MoveTypeLayout> {
        self.fields.iter().map(move |f| MoveTypeLayout {
            pool: self.pool.clone(),
            root: *f,
        })
    }

    /// Returns `true` iff the two field-lists describe the same fields
    /// (same arity, pairwise-equivalent layouts), regardless of pool ordering.
    pub fn equivalent(&self, other: &Self) -> bool {
        if Arc::ptr_eq(&self.pool, &other.pool) && self.fields == other.fields {
            return true;
        }
        if self.fields.len() != other.fields.len() {
            return false;
        }
        let mut memo = HashSet::new();
        self.fields
            .iter()
            .zip(other.fields.iter())
            .all(|(a, b)| nodes_equivalent(&self.pool, *a, &other.pool, *b, &mut memo))
    }
}

// --- MoveStructLayout ---

impl MoveStructLayout {
    /// Number of fields.
    pub fn field_count(&self) -> usize {
        self.0.field_count()
    }

    /// Access a field by index.
    pub fn field(&self, i: u16) -> Option<MoveTypeLayout> {
        self.0.field(i)
    }

    /// Iterate over all fields as layouts.
    pub fn fields(&self) -> impl ExactSizeIterator<Item = MoveTypeLayout> {
        self.0.fields()
    }

    /// Returns `true` iff `self` and `other` describe the same struct type,
    /// regardless of pool ordering.
    pub fn equivalent(&self, other: &Self) -> bool {
        self.0.equivalent(&other.0)
    }
}

// --- MoveDatatypeLayout ---

impl MoveDatatypeLayout {
    /// Returns `true` iff `self` and `other` describe the same datatype,
    /// regardless of pool ordering.
    pub fn equivalent(&self, other: &Self) -> bool {
        match (self, other) {
            (MoveDatatypeLayout::Struct(a), MoveDatatypeLayout::Struct(b)) => a.equivalent(b),
            (MoveDatatypeLayout::Enum(a), MoveDatatypeLayout::Enum(b)) => a.equivalent(b),
            _ => false,
        }
    }
}

impl fmt::Display for MoveStructLayout {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() {
            write!(f, "struct {:#}", self.0)
        } else {
            write!(f, "struct {}", self.0)
        }
    }
}

impl fmt::Display for MoveFieldsLayout {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use DebugAsDisplay as DD;

        let mut map = f.debug_map();
        for (i, field) in self.fields.iter().enumerate() {
            map.entry(
                &i,
                &DD(&MoveTypeLayout {
                    pool: self.pool.clone(),
                    root: *field,
                }),
            );
        }
        map.finish()
    }
}

// --- MoveEnumLayout ---

impl MoveEnumLayout {
    /// Number of variants.
    pub fn variant_count(&self) -> usize {
        self.variants.len()
    }

    /// Access a variant by index.
    pub fn variant(&self, i: VariantTag) -> Option<&VariantLayout> {
        self.variants.get(i as usize)
    }

    /// Iterate over all variants.
    pub fn variants(&self) -> &[VariantLayout] {
        &self.variants
    }

    /// Returns `true` iff `self` and `other` describe the same enum type,
    /// regardless of pool ordering. Variants must match positionally
    /// (same Known/Unknown disposition, equivalent fields when Known).
    pub fn equivalent(&self, other: &Self) -> bool {
        if self.variants.len() != other.variants.len() {
            return false;
        }
        self.variants
            .iter()
            .zip(other.variants.iter())
            .all(|pair| match pair {
                (VariantLayout::Unknown, VariantLayout::Unknown) => true,
                (VariantLayout::Known(a), VariantLayout::Known(b)) => a.equivalent(b),
                _ => false,
            })
    }
}

impl fmt::Display for MoveEnumLayout {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "enum ")?;
        for (tag, vfv) in self.variants().iter().enumerate() {
            write!(f, "variant_tag: {} {{ ", tag)?;
            match vfv {
                VariantLayout::Known(fv) => {
                    for (i, field) in fv.fields().enumerate() {
                        write!(f, "{}: {}, ", i, field)?;
                    }
                }
                VariantLayout::Unknown => write!(f, "?")?,
            }
            write!(f, " }} ")?;
        }
        Ok(())
    }
}

// --- DebugAsDisplay ---

impl<T: fmt::Display> fmt::Debug for DebugAsDisplay<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if f.alternate() {
            write!(f, "{:#}", self.0)
        } else {
            write!(f, "{}", self.0)
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

    pub fn struct_layout(&mut self, fields: &[LayoutHandle]) -> AResult<LayoutHandle> {
        let field_refs: Arc<[LayoutRef]> = fields.iter().map(|h| h.0).collect();
        self.add_node(MoveTypeNode::Struct(MoveStructNode { fields: field_refs }))
    }

    pub fn enum_layout(
        &mut self,
        variants: Vec<Option<Vec<LayoutHandle>>>,
    ) -> AResult<LayoutHandle> {
        let variant_refs: Arc<[Option<Arc<[LayoutRef]>>]> = variants
            .into_iter()
            .map(|v_opt| v_opt.map(|v| v.iter().map(|h| h.0).collect::<Arc<[LayoutRef]>>()))
            .collect();
        self.add_node(MoveTypeNode::Enum(MoveEnumNode {
            variants: variant_refs,
        }))
    }

    /// Recursively intern a tree-based layout, deduplicating shared subtrees.
    /// Tree-based enum layouts always have known variants, so all variants
    /// are wrapped in `Some`.
    pub fn from_tree(&mut self, layout: &RV::MoveTypeLayout) -> AResult<LayoutHandle> {
        Ok(match layout {
            RV::MoveTypeLayout::Bool => self.bool(),
            RV::MoveTypeLayout::U8 => self.u8(),
            RV::MoveTypeLayout::U16 => self.u16(),
            RV::MoveTypeLayout::U32 => self.u32(),
            RV::MoveTypeLayout::U64 => self.u64(),
            RV::MoveTypeLayout::U128 => self.u128(),
            RV::MoveTypeLayout::U256 => self.u256(),
            RV::MoveTypeLayout::Address => self.address(),
            RV::MoveTypeLayout::Signer => self.signer(),
            RV::MoveTypeLayout::Vector(inner) => {
                let inner_h = self.from_tree(inner)?;
                self.vector(inner_h)?
            }
            RV::MoveTypeLayout::Struct(s) => {
                let fields = s
                    .fields()
                    .iter()
                    .map(|f| self.from_tree(f))
                    .collect::<AResult<Vec<_>>>()?;
                self.struct_layout(&fields)?
            }
            RV::MoveTypeLayout::Enum(e) => {
                let variant_handles =
                    e.0.iter()
                        .map(|v| {
                            v.iter()
                                .map(|f| self.from_tree(f))
                                .collect::<AResult<Vec<_>>>()
                                .map(Some)
                        })
                        .collect::<AResult<Vec<_>>>()?;
                self.enum_layout(variant_handles)?
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
            MoveTypeNode::Vector(inner) => MoveLayoutView::Vector(Box::new(MoveTypeLayout {
                pool: pool.clone(),
                root: *inner,
            })),
            MoveTypeNode::Struct(s) => {
                MoveLayoutView::Struct(Box::new(MoveStructLayout(MoveFieldsLayout {
                    pool: pool.clone(),
                    fields: s.fields.clone(),
                })))
            }
            MoveTypeNode::Enum(e) => MoveLayoutView::Enum(Box::new(MoveEnumLayout {
                variants: e
                    .variants
                    .iter()
                    .map(|v| {
                        v.as_ref().map(|fields| MoveFieldsLayout {
                            pool: pool.clone(),
                            fields: fields.clone(),
                        })
                    })
                    .map(|v| match v {
                        Some(fields) => VariantLayout::Known(fields),
                        None => VariantLayout::Unknown,
                    })
                    .collect(),
            })),
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
                    sa.fields.len() == sb.fields.len()
                        && sa
                            .fields
                            .iter()
                            .zip(sb.fields.iter())
                            .all(|(a, b)| nodes_equivalent(pool_a, *a, pool_b, *b, memo))
                }
                (MoveTypeNode::Enum(ea), MoveTypeNode::Enum(eb)) => {
                    ea.variants.len() == eb.variants.len()
                        && ea
                            .variants
                            .iter()
                            .zip(eb.variants.iter())
                            .all(|pair| match pair {
                                (None, None) => true,
                                (Some(fa), Some(fb)) => {
                                    fa.len() == fb.len()
                                        && fa.iter().zip(fb.iter()).all(|(a, b)| {
                                            nodes_equivalent(pool_a, *a, pool_b, *b, memo)
                                        })
                                }
                                _ => false,
                            })
                }
                _ => false,
            }
        }
        _ => false,
    }
}
