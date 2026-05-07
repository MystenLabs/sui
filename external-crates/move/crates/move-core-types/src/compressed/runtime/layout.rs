// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::compressed::{ArcPool, LayoutPool, LayoutRef, LeafType, RefPool, ResolvedRef};
use crate::runtime_value as RV;
use anyhow::Result as AResult;
use indexmap::IndexSet;
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
    pub(crate) fields: Box<[LayoutRef]>,
}

/// Enum layout node: each variant is either a known list of field
/// [`LayoutRef`]s, or `None` indicating the variant exists but its
/// field layout is not available.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MoveEnumNode {
    pub(crate) variants: Box<[Option<Box<[LayoutRef]>>]>,
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

// --- Pool resolution ---

/// Resolves layout references for a concrete pool family.
pub trait LayoutPoolResolver<'a>: LayoutPool<'a> {
    fn as_view(layout: &MoveTypeLayoutBase<'a, Self>) -> MoveLayoutView<'a, Self>
    where
        Self: Sized;
}

impl<'a> LayoutPoolResolver<'a> for ArcPool {
    fn as_view(layout: &MoveTypeLayoutBase<'a, Self>) -> MoveLayoutView<'a, Self> {
        resolve_ref_arc(layout.pool.clone(), layout.root)
    }
}

impl<'a> LayoutPoolResolver<'a> for RefPool {
    fn as_view(layout: &MoveTypeLayoutBase<'a, Self>) -> MoveLayoutView<'a, Self> {
        resolve_ref_ref(layout.pool, layout.root)
    }
}

// --- Layout and view types ---

/// A deduplicated, flat representation of a [`RV::MoveTypeLayout`] tree.
/// Cloning is cheap for owned layouts because the pool is shared via `Arc`.
///
/// NOTE: `Eq`/`PartialEq`/`Hash` are intentionally not derived. Two layouts
/// representing the same type may have different pool orderings (node
/// permutations), so structural equality on the raw fields would produce
/// false negatives. Compare by inflating to tree form or by comparing views.
#[derive(Debug, Clone)]
pub struct MoveTypeLayoutBase<'a, S: LayoutPool<'a>> {
    pool: S::Slice<MoveTypeNode>,
    root: LayoutRef,
}

/// Owned compact runtime layout. The `'static` lifetime is only a placeholder
/// for the pool-family shape; owned storage does not borrow from it.
pub type MoveTypeLayout = MoveTypeLayoutBase<'static, ArcPool>;

/// Borrowed compact runtime layout backed by an existing node pool.
pub type BorrowedMoveTypeLayout<'a> = MoveTypeLayoutBase<'a, RefPool>;

/// A resolved view of a layout node. Compound variants contain lightweight
/// layout/view values directly rather than boxed payloads.
#[derive(Debug, Clone)]
pub enum MoveLayoutView<'a, S: LayoutPool<'a>> {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Address,
    Signer,
    Vector(MoveTypeLayoutBase<'a, S>),
    Struct(MoveStructLayoutBase<'a, S>),
    Enum(MoveEnumLayoutBase<'a, S>),
}

/// Owned resolved layout view.
pub type OwnedLayoutView = MoveLayoutView<'static, ArcPool>;

/// Borrowed resolved layout view.
pub type BorrowedLayoutView<'a> = MoveLayoutView<'a, RefPool>;

/// The layout of a Move datatype, which is either a struct or an enum.
#[derive(Debug, Clone)]
pub enum MoveDatatypeLayoutBase<'a, S: LayoutPool<'a>> {
    Struct(Box<MoveStructLayoutBase<'a, S>>),
    Enum(Box<MoveEnumLayoutBase<'a, S>>),
}

pub type MoveDatatypeLayout = MoveDatatypeLayoutBase<'static, ArcPool>;
pub type MoveDatatypeLayoutRef<'a> = MoveDatatypeLayoutBase<'a, RefPool>;

/// The enum layout of an enum type, as a view into a shared pool.
#[derive(Debug, Clone)]
pub struct MoveEnumLayoutBase<'a, S: LayoutPool<'a>> {
    pub(crate) variants: Box<[VariantLayoutBase<'a, S>]>,
}

pub type MoveEnumLayout = MoveEnumLayoutBase<'static, ArcPool>;
pub type MoveEnumLayoutRef<'a> = MoveEnumLayoutBase<'a, RefPool>;

/// The struct layout of a struct type, as a view into a shared pool.
#[derive(Debug, Clone)]
pub struct MoveStructLayoutBase<'a, S: LayoutPool<'a>>(pub MoveFieldsLayoutBase<'a, S>);

pub type MoveStructLayout = MoveStructLayoutBase<'static, ArcPool>;
pub type MoveStructLayoutRef<'a> = MoveStructLayoutBase<'a, RefPool>;

/// The result of looking up a variant in an enum view.
#[derive(Debug, Clone)]
pub enum VariantLayoutBase<'a, S: LayoutPool<'a>> {
    /// The variant's field layout is known.
    Known(MoveFieldsLayoutBase<'a, S>),
    /// The variant exists but its field layout is not available.
    Unknown,
}

pub type VariantLayout = VariantLayoutBase<'static, ArcPool>;
pub type VariantLayoutRef<'a> = VariantLayoutBase<'a, RefPool>;

/// The field layout of a struct or enum variant, as a view into a shared pool.
#[derive(Debug, Clone)]
pub struct MoveFieldsLayoutBase<'a, S: LayoutPool<'a>> {
    pool: S::Slice<MoveTypeNode>,
    fields: S::Slice<LayoutRef>,
}

pub type MoveFieldsLayout = MoveFieldsLayoutBase<'static, ArcPool>;
pub type MoveFieldsLayoutRef<'a> = MoveFieldsLayoutBase<'a, RefPool>;

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

impl<'a, S> MoveTypeLayoutBase<'a, S>
where
    S: LayoutPoolResolver<'a>,
{
    /// Number of compound nodes in the table (excludes inline leaf types).
    pub fn node_count(&self) -> usize {
        self.pool.as_ref().len()
    }

    /// Create a resolved view for navigating this layout.
    pub fn as_view(&self) -> MoveLayoutView<'a, S> {
        S::as_view(self)
    }

    /// Reconstruct the equivalent tree-based layout. Returns an error
    /// if any enum variant has an unknown layout.
    pub fn inflate(&self) -> AResult<RV::MoveTypeLayout> {
        self.as_view().inflate()
    }
}

impl MoveTypeLayout {
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

    /// Borrow this compact layout without cloning its pool.
    pub fn as_ref_layout(&self) -> BorrowedMoveTypeLayout<'_> {
        BorrowedMoveTypeLayout {
            pool: self.pool.as_ref(),
            root: self.root,
        }
    }

    /// Create a borrowed resolved view for navigating this layout.
    pub fn as_view_ref(&self) -> BorrowedLayoutView<'_> {
        self.as_ref_layout().as_view()
    }
}

impl TryFrom<&RV::MoveTypeLayout> for MoveTypeLayout {
    type Error = anyhow::Error;
    fn try_from(layout: &RV::MoveTypeLayout) -> Result<Self, Self::Error> {
        let mut b = MoveTypeLayoutBuilder::new();
        let root = b.from_tree(layout)?;
        Ok(b.build(root))
    }
}

impl<'a, S> fmt::Display for MoveTypeLayoutBase<'a, S>
where
    S: LayoutPoolResolver<'a>,
{
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
            MoveLayoutView::Vector(vv) if f.alternate() => write!(f, "vector<{vv:#}>"),
            MoveLayoutView::Vector(vv) => write!(f, "vector<{vv}>"),
            MoveLayoutView::Struct(fv) if f.alternate() => write!(f, "{fv:#}"),
            MoveLayoutView::Struct(fv) => write!(f, "{fv}"),
            MoveLayoutView::Enum(ev) if f.alternate() => write!(f, "{ev:#}"),
            MoveLayoutView::Enum(ev) => write!(f, "{ev}"),
        }
    }
}

// --- MoveLayoutView ---

impl<'a, S> MoveLayoutView<'a, S>
where
    S: LayoutPoolResolver<'a>,
{
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
                        VariantLayoutBase::Known(fv) => {
                            fv.fields().map(|f| f.inflate()).collect::<AResult<_>>()
                        }
                        VariantLayoutBase::Unknown => {
                            anyhow::bail!("cannot inflate enum with unknown variant layout")
                        }
                    })
                    .collect::<AResult<_>>()?;
                RV::MoveTypeLayout::Enum(Box::new(RV::MoveEnumLayout(Box::new(variants))))
            }
        })
    }
}

impl<'a, S> fmt::Display for MoveLayoutView<'a, S>
where
    S: LayoutPoolResolver<'a>,
{
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
            MoveLayoutView::Vector(vv) if f.alternate() => write!(f, "vector<{vv:#}>"),
            MoveLayoutView::Vector(vv) => write!(f, "vector<{vv}>"),
            MoveLayoutView::Struct(fv) if f.alternate() => write!(f, "{fv:#}"),
            MoveLayoutView::Struct(fv) => write!(f, "{fv}"),
            MoveLayoutView::Enum(ev) if f.alternate() => write!(f, "{ev:#}"),
            MoveLayoutView::Enum(ev) => write!(f, "{ev}"),
        }
    }
}

// --- MoveFieldsLayout ---

impl<'a, S> MoveFieldsLayoutBase<'a, S>
where
    S: LayoutPool<'a>,
{
    /// Number of fields.
    pub fn field_count(&self) -> usize {
        self.fields.as_ref().len()
    }

    /// Access a field by index.
    pub fn field(&self, i: usize) -> Option<MoveTypeLayoutBase<'a, S>> {
        self.fields.as_ref().get(i).map(|f| MoveTypeLayoutBase {
            pool: self.pool.clone(),
            root: *f,
        })
    }

    /// Iterate over all fields as layout views.
    pub fn fields(&self) -> impl ExactSizeIterator<Item = MoveTypeLayoutBase<'a, S>> + '_ {
        self.fields
            .as_ref()
            .iter()
            .map(move |f| MoveTypeLayoutBase {
                pool: self.pool.clone(),
                root: *f,
            })
    }
}

// --- MoveStructLayout ---

impl<'a, S> MoveStructLayoutBase<'a, S>
where
    S: LayoutPool<'a>,
{
    /// Number of fields.
    pub fn field_count(&self) -> usize {
        self.0.field_count()
    }

    /// Access a field by index.
    pub fn field(&self, i: usize) -> Option<MoveTypeLayoutBase<'a, S>> {
        self.0.field(i)
    }

    /// Iterate over all fields as layouts.
    pub fn fields(&self) -> impl ExactSizeIterator<Item = MoveTypeLayoutBase<'a, S>> + '_ {
        self.0.fields()
    }
}

impl<'a, S> fmt::Display for MoveStructLayoutBase<'a, S>
where
    S: LayoutPoolResolver<'a>,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "struct {}", self.0)
    }
}

impl<'a, S> fmt::Display for MoveFieldsLayoutBase<'a, S>
where
    S: LayoutPoolResolver<'a>,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use DebugAsDisplay as DD;

        let mut map = f.debug_map();
        for (i, field) in self.fields.as_ref().iter().enumerate() {
            map.entry(
                &i,
                &DD(&MoveTypeLayoutBase::<S> {
                    pool: self.pool.clone(),
                    root: *field,
                }),
            );
        }
        map.finish()
    }
}

// --- MoveEnumLayout ---

impl<'a, S> MoveEnumLayoutBase<'a, S>
where
    S: LayoutPool<'a>,
{
    /// Number of variants.
    pub fn variant_count(&self) -> usize {
        self.variants.len()
    }

    /// Access a variant by index.
    pub fn variant(&self, i: usize) -> Option<&VariantLayoutBase<'a, S>> {
        self.variants.get(i)
    }

    /// Iterate over all variants.
    pub fn variants(&self) -> &[VariantLayoutBase<'a, S>] {
        &self.variants
    }
}

impl<'a, S> fmt::Display for MoveEnumLayoutBase<'a, S>
where
    S: LayoutPoolResolver<'a>,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "enum ")?;
        for (tag, vfv) in self.variants().iter().enumerate() {
            write!(f, "variant_tag: {} {{ ", tag)?;
            match vfv {
                VariantLayoutBase::Known(fv) => {
                    for (i, field) in fv.fields().enumerate() {
                        write!(f, "{}: {}, ", i, field)?;
                    }
                }
                VariantLayoutBase::Unknown => write!(f, "?")?,
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
        let field_refs: Box<[LayoutRef]> = fields.iter().map(|h| h.0).collect();
        self.add_node(MoveTypeNode::Struct(MoveStructNode { fields: field_refs }))
    }

    pub fn enum_layout(&mut self, variants: &[Option<&[LayoutHandle]>]) -> AResult<LayoutHandle> {
        let variant_refs: Box<[Option<Box<[LayoutRef]>>]> = variants
            .iter()
            .map(|v| v.map(|fields| fields.iter().map(|h| h.0).collect()))
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
                        })
                        .collect::<AResult<Vec<_>>>()?;
                let variant_refs: Vec<Option<&[LayoutHandle]>> =
                    variant_handles.iter().map(|v| Some(v.as_slice())).collect();
                self.enum_layout(&variant_refs)?
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
}

impl Default for MoveTypeLayoutBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Free functions
// =============================================================================

fn leaf_to_layout_view<'a, S: LayoutPool<'a>>(leaf: LeafType) -> MoveLayoutView<'a, S> {
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

/// Resolve a [`LayoutRef`] against an owned pool into an [`OwnedLayoutView`].
///
/// Panics if the reference points to an out-of-bounds table index.
fn resolve_ref_arc<'a>(pool: Arc<MoveTypeLayoutPool>, r: LayoutRef) -> MoveLayoutView<'a, ArcPool> {
    match r.resolve() {
        ResolvedRef::Leaf(leaf) => leaf_to_layout_view(leaf),
        ResolvedRef::Index(idx) => match &pool[idx] {
            MoveTypeNode::Vector(inner) => MoveLayoutView::Vector(MoveTypeLayoutBase {
                pool: pool.clone(),
                root: *inner,
            }),
            MoveTypeNode::Struct(s) => {
                MoveLayoutView::Struct(MoveStructLayoutBase(MoveFieldsLayoutBase {
                    pool: pool.clone(),
                    fields: Arc::from(s.fields.clone()),
                }))
            }
            MoveTypeNode::Enum(e) => MoveLayoutView::Enum(MoveEnumLayoutBase {
                variants: e
                    .variants
                    .iter()
                    .map(|v| match v {
                        Some(fields) => VariantLayoutBase::Known(MoveFieldsLayoutBase {
                            pool: pool.clone(),
                            fields: Arc::from(fields.clone()),
                        }),
                        None => VariantLayoutBase::Unknown,
                    })
                    .collect(),
            }),
        },
    }
}

/// Resolve a [`LayoutRef`] against a borrowed pool into a borrowed [`BorrowedLayoutView`].
///
/// Panics if the reference points to an out-of-bounds table index.
fn resolve_ref_ref<'a>(pool: &'a MoveTypeLayoutPool, r: LayoutRef) -> BorrowedLayoutView<'a> {
    match r.resolve() {
        ResolvedRef::Leaf(leaf) => leaf_to_layout_view(leaf),
        ResolvedRef::Index(idx) => match &pool[idx] {
            MoveTypeNode::Vector(inner) => {
                MoveLayoutView::Vector(BorrowedMoveTypeLayout { pool, root: *inner })
            }
            MoveTypeNode::Struct(s) => {
                MoveLayoutView::Struct(MoveStructLayoutBase(MoveFieldsLayoutRef {
                    pool,
                    fields: s.fields.as_ref(),
                }))
            }
            MoveTypeNode::Enum(e) => MoveLayoutView::Enum(MoveEnumLayoutRef {
                variants: e
                    .variants
                    .iter()
                    .map(|v| match v {
                        Some(fields) => VariantLayoutBase::Known(MoveFieldsLayoutRef {
                            pool,
                            fields: fields.as_ref(),
                        }),
                        None => VariantLayoutBase::Unknown,
                    })
                    .collect(),
            }),
        },
    }
}
