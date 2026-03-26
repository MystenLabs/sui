// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::compressed::{LayoutRef, LeafType, ResolvedRef, Shared};
use crate::runtime_value::{
    MoveEnumLayout, MoveStructLayout, MoveTypeLayout as TreeMoveTypeLayout,
};
use anyhow::Result as AResult;
use indexmap::IndexSet;
use std::fmt;

pub use crate::compressed::LayoutHandle;

static EMPTY_POOL: std::sync::LazyLock<Shared<MoveTypeLayoutPool>> =
    std::sync::LazyLock::new(|| Shared::from(Vec::<MoveTypeNode>::new()));

// =============================================================================
// Type declarations
// =============================================================================

// --- Node types ---

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

// --- Owned layout type ---

/// A deduplicated, flat representation of a [`TreeMoveTypeLayout`] tree.
/// Cloning is cheap — the pool is shared via `Arc`.
#[derive(Debug, Clone)]
pub struct MoveTypeLayout {
    pool: Shared<MoveTypeLayoutPool>,
    root: LayoutRef,
}

// --- View types ---

/// A resolved view of a layout node. Leaf types are unit variants;
/// compound types contain further views for direct navigation.
/// Resolution is lazy — only one layer is resolved at a time.
#[derive(Debug, Clone, Copy)]
pub enum MoveLayoutView<'a> {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Address,
    Signer,
    Vector(MoveVectorView<'a>),
    Struct(MoveFieldView<'a>),
    Enum(MoveEnumView<'a>),
}

/// A lazy view over a vector layout's element type.
#[derive(Debug, Clone, Copy)]
pub struct MoveVectorView<'a> {
    pool: &'a MoveTypeLayoutPool,
    element: LayoutRef,
}

/// A view over a list of typed fields (struct fields or enum variant fields).
#[derive(Debug, Clone, Copy)]
pub struct MoveFieldView<'a> {
    pool: &'a MoveTypeLayoutPool,
    fields: &'a [LayoutRef],
}

/// The result of looking up a variant in an enum view.
#[derive(Debug, Clone, Copy)]
pub enum VariantFieldView<'a> {
    /// The variant's field layout is known.
    Known(MoveFieldView<'a>),
    /// The variant exists but its field layout is not available.
    Unknown,
}

/// A view over an enum layout's variants.
#[derive(Debug, Clone, Copy)]
pub struct MoveEnumView<'a> {
    pool: &'a MoveTypeLayoutPool,
    variants: &'a [Option<Box<[LayoutRef]>>],
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

    /// Create a sub-layout rooted at a different position within
    /// the same shared pool. This is a `Shared` bump — no data is copied.
    pub(crate) fn sublayout(&self, root: LayoutRef) -> Self {
        MoveTypeLayout {
            pool: self.pool.clone(),
            root,
        }
    }

    /// Create a resolved view for navigating this layout.
    pub fn as_view(&self) -> MoveLayoutView<'_> {
        resolve_ref(&self.pool, self.root)
    }

    /// Reconstruct the equivalent tree-based layout.
    pub fn inflate(&self) -> AResult<TreeMoveTypeLayout> {
        self.as_view().inflate()
    }

    /// If this is a struct, extract a sub-layout for the field at `index`.
    /// The sub-layout shares the same backing pool (cheap `Shared` bump).
    pub fn struct_field_sublayout(&self, index: usize) -> Option<MoveTypeLayout> {
        match self.as_view() {
            MoveLayoutView::Struct(fv) => {
                let field_ref = fv.raw_field(index)?;
                Some(self.sublayout(field_ref))
            }
            _ => None,
        }
    }

    /// If this is a vector, extract a sub-layout for the element type.
    pub fn vector_element_sublayout(&self) -> Option<MoveTypeLayout> {
        match self.as_view() {
            MoveLayoutView::Vector(vv) => Some(self.sublayout(vv.raw_element())),
            _ => None,
        }
    }

    /// If this is an enum, extract a sub-layout for a variant's field.
    pub fn enum_variant_field_sublayout(
        &self,
        variant_tag: u16,
        field_index: usize,
    ) -> Option<MoveTypeLayout> {
        match self.as_view() {
            MoveLayoutView::Enum(ev) => {
                let vfv = ev.variant(variant_tag as usize)?;
                let fv = match vfv {
                    VariantFieldView::Known(fv) => fv,
                    VariantFieldView::Unknown => return None,
                };
                let field_ref = fv.raw_field(field_index)?;
                Some(self.sublayout(field_ref))
            }
            _ => None,
        }
    }
}

impl TryFrom<&TreeMoveTypeLayout> for MoveTypeLayout {
    type Error = anyhow::Error;
    fn try_from(layout: &TreeMoveTypeLayout) -> Result<Self, Self::Error> {
        let mut b = MoveTypeLayoutBuilder::new();
        let root = b.intern_tree(layout)?;
        Ok(b.build(root))
    }
}

// --- MoveLayoutView ---

impl<'a> MoveLayoutView<'a> {
    /// Reconstruct the equivalent tree-based layout. Returns an error
    /// if any enum variant has an unknown layout.
    pub fn inflate(&self) -> AResult<TreeMoveTypeLayout> {
        Ok(match self {
            MoveLayoutView::Bool => TreeMoveTypeLayout::Bool,
            MoveLayoutView::U8 => TreeMoveTypeLayout::U8,
            MoveLayoutView::U16 => TreeMoveTypeLayout::U16,
            MoveLayoutView::U32 => TreeMoveTypeLayout::U32,
            MoveLayoutView::U64 => TreeMoveTypeLayout::U64,
            MoveLayoutView::U128 => TreeMoveTypeLayout::U128,
            MoveLayoutView::U256 => TreeMoveTypeLayout::U256,
            MoveLayoutView::Address => TreeMoveTypeLayout::Address,
            MoveLayoutView::Signer => TreeMoveTypeLayout::Signer,
            MoveLayoutView::Vector(vv) => {
                TreeMoveTypeLayout::Vector(Box::new(vv.element().inflate()?))
            }
            MoveLayoutView::Struct(fv) => {
                let fields = fv.fields().map(|f| f.inflate()).collect::<AResult<_>>()?;
                TreeMoveTypeLayout::Struct(Box::new(MoveStructLayout::new(fields)))
            }
            MoveLayoutView::Enum(ev) => {
                let variants = ev
                    .variants()
                    .map(|vfv| match vfv {
                        VariantFieldView::Known(fv) => {
                            fv.fields().map(|f| f.inflate()).collect::<AResult<_>>()
                        }
                        VariantFieldView::Unknown => {
                            anyhow::bail!("cannot inflate enum with unknown variant layout")
                        }
                    })
                    .collect::<AResult<_>>()?;
                TreeMoveTypeLayout::Enum(Box::new(MoveEnumLayout(Box::new(variants))))
            }
        })
    }
}

impl fmt::Display for MoveLayoutView<'_> {
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
            MoveLayoutView::Vector(vv) if f.alternate() => write!(f, "vector<{:#}>", vv.element()),
            MoveLayoutView::Vector(vv) => write!(f, "vector<{}>", vv.element()),
            MoveLayoutView::Struct(fv) if f.alternate() => write!(f, "{fv:#}"),
            MoveLayoutView::Struct(fv) => write!(f, "{fv}"),
            MoveLayoutView::Enum(ev) if f.alternate() => write!(f, "{ev:#}"),
            MoveLayoutView::Enum(ev) => write!(f, "{ev}"),
        }
    }
}

// --- MoveVectorView ---

impl<'a> MoveVectorView<'a> {
    /// Resolve the element type.
    pub fn element(&self) -> MoveLayoutView<'a> {
        resolve_ref(self.pool, self.element)
    }

    /// The raw element ref (for `sublayout`).
    pub(crate) fn raw_element(&self) -> LayoutRef {
        self.element
    }
}

// --- MoveFieldView ---

impl<'a> MoveFieldView<'a> {
    /// Number of fields.
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    /// Access a field by index.
    pub fn field(&self, i: usize) -> Option<MoveLayoutView<'a>> {
        self.fields.get(i).map(|&r| resolve_ref(self.pool, r))
    }

    /// Access a field's raw ref by index (for `sublayout`).
    pub(crate) fn raw_field(&self, i: usize) -> Option<LayoutRef> {
        self.fields.get(i).copied()
    }

    /// Iterate over all fields as layout views.
    pub fn fields(&self) -> impl ExactSizeIterator<Item = MoveLayoutView<'a>> + '_ {
        let pool = self.pool;
        self.fields.iter().map(move |&r| resolve_ref(pool, r))
    }
}

impl fmt::Display for MoveFieldView<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use DebugAsDisplay as DD;

        write!(f, "struct ")?;
        let mut map = f.debug_map();
        for (i, field) in self.fields().enumerate() {
            map.entry(&i, &DD(&field));
        }
        map.finish()
    }
}

// --- MoveEnumView ---

impl<'a> MoveEnumView<'a> {
    /// Number of variants.
    pub fn variant_count(&self) -> usize {
        self.variants.len()
    }

    /// Access a variant by index. Returns `None` if out of bounds,
    /// `Some(VariantFieldView::Unknown)` if the variant exists but has
    /// no known layout, or `Some(VariantFieldView::Known(fields))`.
    pub fn variant(&self, i: usize) -> Option<VariantFieldView<'a>> {
        self.variants.get(i).map(|v| match v {
            Some(fields) => VariantFieldView::Known(MoveFieldView {
                pool: self.pool,
                fields,
            }),
            None => VariantFieldView::Unknown,
        })
    }

    /// Iterate over all variants.
    pub fn variants(&self) -> impl ExactSizeIterator<Item = VariantFieldView<'a>> + 'a {
        let pool = self.pool;
        self.variants.iter().map(move |v| match v {
            Some(fields) => VariantFieldView::Known(MoveFieldView { pool, fields }),
            None => VariantFieldView::Unknown,
        })
    }
}

impl fmt::Display for MoveEnumView<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "enum ")?;
        for (tag, vfv) in self.variants().enumerate() {
            write!(f, "variant_tag: {} {{ ", tag)?;
            match vfv {
                VariantFieldView::Known(fv) => {
                    for (i, field) in fv.fields().enumerate() {
                        write!(f, "{}: {}, ", i, field)?;
                    }
                }
                VariantFieldView::Unknown => write!(f, "?")?,
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

    fn intern(&mut self, node: MoveTypeNode) -> AResult<LayoutHandle> {
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
        self.intern(MoveTypeNode::Vector(element.0))
    }

    pub fn struct_layout(&mut self, fields: &[LayoutHandle]) -> AResult<LayoutHandle> {
        let field_refs: Box<[LayoutRef]> = fields.iter().map(|h| h.0).collect();
        self.intern(MoveTypeNode::Struct(MoveStructNode { fields: field_refs }))
    }

    pub fn enum_layout(&mut self, variants: &[Option<&[LayoutHandle]>]) -> AResult<LayoutHandle> {
        let variant_refs: Box<[Option<Box<[LayoutRef]>>]> = variants
            .iter()
            .map(|v| v.map(|fields| fields.iter().map(|h| h.0).collect()))
            .collect();
        self.intern(MoveTypeNode::Enum(MoveEnumNode {
            variants: variant_refs,
        }))
    }

    /// Recursively intern a tree-based layout, deduplicating shared subtrees.
    /// Tree-based enum layouts always have known variants, so all variants
    /// are wrapped in `Some`.
    pub fn intern_tree(&mut self, layout: &TreeMoveTypeLayout) -> AResult<LayoutHandle> {
        Ok(match layout {
            TreeMoveTypeLayout::Bool => self.bool(),
            TreeMoveTypeLayout::U8 => self.u8(),
            TreeMoveTypeLayout::U16 => self.u16(),
            TreeMoveTypeLayout::U32 => self.u32(),
            TreeMoveTypeLayout::U64 => self.u64(),
            TreeMoveTypeLayout::U128 => self.u128(),
            TreeMoveTypeLayout::U256 => self.u256(),
            TreeMoveTypeLayout::Address => self.address(),
            TreeMoveTypeLayout::Signer => self.signer(),
            TreeMoveTypeLayout::Vector(inner) => {
                let inner_h = self.intern_tree(inner)?;
                self.vector(inner_h)?
            }
            TreeMoveTypeLayout::Struct(s) => {
                let fields = s
                    .fields()
                    .iter()
                    .map(|f| self.intern_tree(f))
                    .collect::<AResult<Vec<_>>>()?;
                self.struct_layout(&fields)?
            }
            TreeMoveTypeLayout::Enum(e) => {
                let variant_handles =
                    e.0.iter()
                        .map(|v| {
                            v.iter()
                                .map(|f| self.intern_tree(f))
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
            pool: Shared::from(nodes),
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

fn leaf_to_layout_view(leaf: LeafType) -> MoveLayoutView<'static> {
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
fn resolve_ref<'a>(pool: &'a MoveTypeLayoutPool, r: LayoutRef) -> MoveLayoutView<'a> {
    match r.resolve() {
        ResolvedRef::Leaf(leaf) => leaf_to_layout_view(leaf),
        ResolvedRef::Index(idx) => match &pool[idx] {
            MoveTypeNode::Vector(inner) => MoveLayoutView::Vector(MoveVectorView {
                pool,
                element: *inner,
            }),
            MoveTypeNode::Struct(s) => MoveLayoutView::Struct(MoveFieldView {
                pool,
                fields: &s.fields,
            }),
            MoveTypeNode::Enum(e) => MoveLayoutView::Enum(MoveEnumView {
                pool,
                variants: &e.variants,
            }),
        },
    }
}
