// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::annotated_value::{
    MoveEnumLayout, MoveFieldLayout, MoveStructLayout, MoveTypeLayout as TreeMoveTypeLayout,
};
pub use crate::compressed::LayoutHandle;
use crate::compressed::{LayoutRef, LeafType, ResolvedRef, Shared};
use crate::identifier::{IdentStr, Identifier};
use crate::language_storage::{StructTag, TypeTag};
use anyhow::Result as AResult;
use indexmap::IndexSet;
use std::fmt;

static EMPTY_POOL: std::sync::LazyLock<Shared<MoveTypeLayoutPool>> =
    std::sync::LazyLock::new(|| Shared::from(Vec::<MoveTypeNode>::new()));

/// A (field_name, layout_ref) pair for struct/enum fields.
pub(crate) type AnnotatedFieldEntry = (Identifier, LayoutRef);

/// A single variant entry: (variant_name, tag, optional fields).
/// `None` fields means the variant exists but its layout is unknown.
pub(crate) type AnnotatedVariantEntry = (Identifier, u16, Option<Box<[AnnotatedFieldEntry]>>);

/// Annotated struct layout node with type tag and named fields inline.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MoveStructNode {
    pub(crate) type_: StructTag,
    pub(crate) fields: Box<[AnnotatedFieldEntry]>,
}

/// Annotated enum layout node with type tag and named variants inline.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MoveEnumNode {
    pub(crate) type_: StructTag,
    pub(crate) variants: Box<[AnnotatedVariantEntry]>,
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

/// A deduplicated, flat representation of an annotated [`MoveTypeLayout`] tree.
/// Names and type tags are stored inline in nodes. Cloning is cheap — the
/// node table is shared via `Arc`.
#[derive(Debug, Clone)]
pub struct MoveTypeLayout {
    pub(crate) pool: Shared<MoveTypeLayoutPool>,
    pub(crate) root: LayoutRef,
}

/// A compressed layout that is known to be a struct or enum (not a primitive
/// or vector). This mirrors the tree-based [`crate::annotated_value::MoveDatatypeLayout`].
#[derive(Debug, Clone)]
pub struct MoveDatatypeLayout(MoveTypeLayout);

// --- View types ---

/// A resolved view of an annotated layout node. Compound types contain
/// further views with eagerly resolved type tags and field names.
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
    Struct(MoveStructView<'a>),
    Enum(MoveEnumView<'a>),
}

/// A lazy view over an annotated vector layout's element type.
#[derive(Debug, Clone, Copy)]
pub struct MoveVectorView<'a> {
    pub(crate) pool: &'a MoveTypeLayoutPool,
    pub(crate) element: LayoutRef,
}

/// A view over a list of named, typed fields (struct fields or enum variant fields).
#[derive(Debug, Clone, Copy)]
pub struct MoveFieldView<'a> {
    pub(crate) pool: &'a MoveTypeLayoutPool,
    pub(crate) fields: &'a [(Identifier, LayoutRef)],
}

/// A view over an annotated struct layout with type tag and field access.
#[derive(Debug, Clone, Copy)]
pub struct MoveStructView<'a> {
    pub(crate) type_: &'a StructTag,
    pub(crate) fields: MoveFieldView<'a>,
}

/// The result of looking up a variant in an annotated enum view.
#[derive(Debug, Clone, Copy)]
pub enum VariantFieldView<'a> {
    /// The variant's field layout is known.
    Known(MoveFieldView<'a>),
    /// The variant exists but its field layout is not available.
    Unknown,
}

/// A view over an annotated enum layout's variants.
#[derive(Debug, Clone, Copy)]
pub struct MoveEnumView<'a> {
    pub(crate) pool: &'a MoveTypeLayoutPool,
    pub(crate) type_: &'a StructTag,
    pub(crate) variants: &'a [AnnotatedVariantEntry],
}

/// A view over a single named field, mirroring the tree-based [`MoveFieldLayout`].
/// Used by driver accessor methods (`peek_field`, `next_field`, `skip_field`).
#[derive(Debug, Clone, Copy)]
pub struct MoveFieldLayoutView<'a> {
    name: &'a IdentStr,
    layout: MoveLayoutView<'a>,
}

// --- Builder type ---

/// Incrementally builds an annotated [`MoveTypeLayout`] with automatic
/// deduplication of nodes.
pub struct MoveTypeLayoutBuilder {
    nodes: IndexSet<MoveTypeNode>,
}

// --- Display helper ---

/// Helper type that uses `T`'s `Display` implementation as its own `Debug` implementation,
/// to allow other `Display` implementations to take advantage of structured formatting helpers.
#[allow(dead_code)] // TODO: remove when used
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

    /// If this is a struct, extract a sub-layout for the field at `index`.
    /// The sub-layout shares the same backing pool (cheap `Shared` bump).
    pub fn struct_field_sublayout(&self, index: usize) -> Option<MoveTypeLayout> {
        match self.as_view() {
            MoveLayoutView::Struct(sv) => {
                let (_, field_ref) = sv.fields.raw_field(index)?;
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
                let (_, vfv) = ev.variant_by_tag(variant_tag)?;
                let fv = match vfv {
                    VariantFieldView::Known(fv) => fv,
                    VariantFieldView::Unknown => return None,
                };
                let (_, field_ref) = fv.raw_field(field_index)?;
                Some(self.sublayout(field_ref))
            }
            _ => None,
        }
    }

    /// Inflate back into a tree-based [`MoveTypeLayout`].
    pub fn inflate(&self) -> AResult<TreeMoveTypeLayout> {
        self.as_view().inflate()
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

// --- MoveDatatypeLayout ---

impl MoveDatatypeLayout {
    /// Wrap a `MoveTypeLayout` that is known to be a struct or enum.
    /// Returns `None` if the layout is a primitive or vector.
    pub fn new(layout: MoveTypeLayout) -> Option<Self> {
        match layout.as_view() {
            MoveLayoutView::Struct(_) | MoveLayoutView::Enum(_) => Some(MoveDatatypeLayout(layout)),
            _ => None,
        }
    }

    /// Convert into the underlying `MoveTypeLayout`.
    pub fn into_layout(self) -> MoveTypeLayout {
        self.0
    }

    /// Borrow the underlying `MoveTypeLayout`.
    pub fn as_layout(&self) -> &MoveTypeLayout {
        &self.0
    }

    /// Create a view for navigating this layout.
    pub fn as_view(&self) -> MoveLayoutView<'_> {
        self.0.as_view()
    }

    /// Inflate back into a tree-based [`crate::annotated_value::MoveDatatypeLayout`].
    pub fn inflate(&self) -> AResult<crate::annotated_value::MoveDatatypeLayout> {
        let tree = self.0.inflate()?;
        match tree {
            TreeMoveTypeLayout::Struct(s) => {
                Ok(crate::annotated_value::MoveDatatypeLayout::Struct(s))
            }
            TreeMoveTypeLayout::Enum(e) => Ok(crate::annotated_value::MoveDatatypeLayout::Enum(e)),
            _ => anyhow::bail!("MoveDatatypeLayout contained non-datatype layout"),
        }
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
            MoveLayoutView::Struct(sv) => {
                let fields = sv
                    .fields()
                    .map(|(name, fv)| Ok(MoveFieldLayout::new(name.clone(), fv.inflate()?)))
                    .collect::<AResult<_>>()?;
                TreeMoveTypeLayout::Struct(Box::new(MoveStructLayout {
                    type_: sv.type_().clone(),
                    fields,
                }))
            }
            MoveLayoutView::Enum(ev) => {
                let variants = ev
                    .variants()
                    .map(|(variant_name, tag, vfv)| match vfv {
                        VariantFieldView::Known(fv) => {
                            let field_layouts = fv
                                .fields()
                                .map(|(name, fv)| {
                                    Ok(MoveFieldLayout::new(name.clone(), fv.inflate()?))
                                })
                                .collect::<AResult<_>>()?;
                            Ok(((variant_name.clone(), tag), field_layouts))
                        }
                        VariantFieldView::Unknown => {
                            anyhow::bail!("cannot inflate enum with unknown variant layout")
                        }
                    })
                    .collect::<AResult<_>>()?;
                TreeMoveTypeLayout::Enum(Box::new(MoveEnumLayout {
                    type_: ev.type_().clone(),
                    variants,
                }))
            }
        })
    }
}

impl MoveLayoutView<'_> {
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
                    vv.element().is_type(inner)
                } else {
                    false
                }
            }
            MoveLayoutView::Enum(ev) => ev.is_type(t),
        }
    }
}

impl<'a> From<MoveLayoutView<'a>> for TypeTag {
    fn from(view: MoveLayoutView<'a>) -> TypeTag {
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
            MoveLayoutView::Vector(vv) => TypeTag::Vector(Box::new(TypeTag::from(vv.element()))),
            MoveLayoutView::Struct(sv) => TypeTag::Struct(Box::new(sv.type_().clone())),
            MoveLayoutView::Enum(ev) => TypeTag::Struct(Box::new(ev.type_().clone())),
        }
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
            MoveLayoutView::Struct(sv) if f.alternate() => write!(f, "{sv:#}"),
            MoveLayoutView::Struct(sv) => write!(f, "{sv}"),
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

    /// The raw element ref (for sublayout).
    pub(crate) fn raw_element(&self) -> LayoutRef {
        self.element
    }
}

// --- MoveStructView ---

impl<'a> MoveStructView<'a> {
    /// The struct's type tag.
    pub fn type_(&self) -> &'a StructTag {
        self.type_
    }

    pub fn is_type(&self, t: &TypeTag) -> bool {
        matches!(t, TypeTag::Struct(s) if &**s == self.type_())
    }

    /// A field view for iterating/accessing the struct's fields.
    pub fn field_view(&self) -> MoveFieldView<'a> {
        self.fields
    }

    /// Number of fields.
    pub fn field_count(&self) -> usize {
        self.fields.field_count()
    }

    /// Access a field by index, returning `(name, layout_view)`.
    pub fn field(&self, i: usize) -> Option<(&'a Identifier, MoveLayoutView<'a>)> {
        self.fields.field(i)
    }

    /// Iterate over all fields as `(name, layout_view)` pairs.
    pub fn fields(
        &self,
    ) -> impl ExactSizeIterator<Item = (&'a Identifier, MoveLayoutView<'a>)> + 'a {
        let pool = self.fields.pool;
        self.fields
            .fields
            .iter()
            .map(move |(name, layout_ref)| (name, resolve_ref(pool, *layout_ref)))
    }
}

impl fmt::Display for MoveStructView<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {{ ", self.type_)?;
        for (i, (name, layout)) in self.fields().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            if f.alternate() {
                write!(f, "{name}: {layout:#}")?;
            } else {
                write!(f, "{name}: {layout}")?;
            }
        }
        write!(f, " }}")
    }
}

// --- MoveFieldView ---

impl<'a> MoveFieldView<'a> {
    /// Number of fields.
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    /// Access a field by index, returning `(name, layout_view)`.
    pub fn field(&self, i: usize) -> Option<(&'a Identifier, MoveLayoutView<'a>)> {
        self.fields
            .get(i)
            .map(|(name, layout_ref)| (name, resolve_ref(self.pool, *layout_ref)))
    }

    /// Access a field's raw ref by index (for sublayout).
    pub(crate) fn raw_field(&self, i: usize) -> Option<(&'a Identifier, LayoutRef)> {
        self.fields
            .get(i)
            .map(|(name, layout_ref)| (name, *layout_ref))
    }

    /// Look up a field by name, returning its layout view.
    pub fn field_by_name(&self, name: &str) -> Option<MoveLayoutView<'a>> {
        self.fields
            .iter()
            .find(|(field_name, _)| field_name.as_str() == name)
            .map(|(_, layout_ref)| resolve_ref(self.pool, *layout_ref))
    }

    /// Iterate over all fields as `(name, layout_view)` pairs.
    pub fn fields(
        &self,
    ) -> impl ExactSizeIterator<Item = (&'a Identifier, MoveLayoutView<'a>)> + '_ {
        let pool = self.pool;
        self.fields
            .iter()
            .map(move |(name, layout_ref)| (name, resolve_ref(pool, *layout_ref)))
    }
}

// --- MoveFieldLayoutView ---

impl<'a> MoveFieldLayoutView<'a> {
    pub fn new(name: &'a IdentStr, layout: MoveLayoutView<'a>) -> Self {
        Self { name, layout }
    }

    pub fn name(&self) -> &'a IdentStr {
        self.name
    }

    pub fn layout(&self) -> MoveLayoutView<'a> {
        self.layout
    }
}

// --- MoveEnumView ---

impl<'a> MoveEnumView<'a> {
    /// The enum's type tag.
    pub fn type_(&self) -> &'a StructTag {
        self.type_
    }

    pub fn is_type(&self, t: &TypeTag) -> bool {
        matches!(t, TypeTag::Struct(s) if **s == *self.type_())
    }

    /// Number of variants.
    pub fn variant_count(&self) -> usize {
        self.variants.len()
    }

    /// Access a variant by position index. Returns `None` if out of bounds.
    pub fn variant(&self, i: usize) -> Option<(&'a Identifier, u16, VariantFieldView<'a>)> {
        self.variants.get(i).map(|(name, tag, fields)| {
            let vfv = match fields {
                Some(fields) => VariantFieldView::Known(MoveFieldView {
                    pool: self.pool,
                    fields,
                }),
                None => VariantFieldView::Unknown,
            };
            (name, *tag, vfv)
        })
    }

    /// Find a variant by its tag value.
    pub fn variant_by_tag(&self, tag: u16) -> Option<(&'a Identifier, VariantFieldView<'a>)> {
        self.variants
            .iter()
            .find(|(_, t, _)| *t == tag)
            .map(|(name, _, fields)| {
                let vfv = match fields {
                    Some(fields) => VariantFieldView::Known(MoveFieldView {
                        pool: self.pool,
                        fields,
                    }),
                    None => VariantFieldView::Unknown,
                };
                (name, vfv)
            })
    }

    /// Iterate over all variants as `(name, tag, field_view)` tuples.
    pub fn variants(
        &self,
    ) -> impl ExactSizeIterator<Item = (&'a Identifier, u16, VariantFieldView<'a>)> + 'a {
        let pool = self.pool;
        self.variants.iter().map(move |(name, tag, fields)| {
            let vfv = match fields {
                Some(fields) => VariantFieldView::Known(MoveFieldView { pool, fields }),
                None => VariantFieldView::Unknown,
            };
            (name, *tag, vfv)
        })
    }
}

impl fmt::Display for MoveEnumView<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {{ ", self.type_)?;
        for (i, (variant_name, _tag, vfv)) in self.variants().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{variant_name}(")?;
            match vfv {
                VariantFieldView::Known(fv) => {
                    for (j, (name, layout)) in fv.fields().enumerate() {
                        if j > 0 {
                            write!(f, ", ")?;
                        }
                        if f.alternate() {
                            write!(f, "{name}: {layout:#}")?;
                        } else {
                            write!(f, "{name}: {layout}")?;
                        }
                    }
                }
                VariantFieldView::Unknown => write!(f, "?")?,
            }
            write!(f, ")")?;
        }
        write!(f, " }}")
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

    /// Build a struct layout node.
    /// `fields` is a list of (field_name, field_layout) pairs.
    pub fn struct_layout(
        &mut self,
        type_tag: &StructTag,
        fields: &[(&Identifier, LayoutHandle)],
    ) -> AResult<LayoutHandle> {
        let fields: Box<[AnnotatedFieldEntry]> = fields
            .iter()
            .map(|(name, h)| ((*name).clone(), h.0))
            .collect();
        self.intern(MoveTypeNode::Struct(MoveStructNode {
            type_: type_tag.clone(),
            fields,
        }))
    }

    /// Build an enum layout node.
    /// Each variant is `(variant_name, tag, fields)` where fields is
    /// `None` for unknown layout or `Some(&[(field_name, layout)])` for known.
    pub fn enum_layout(
        &mut self,
        type_tag: &StructTag,
        variants: &[(&Identifier, u16, Option<&[(&Identifier, LayoutHandle)]>)],
    ) -> AResult<LayoutHandle> {
        let variant_entries: Box<[AnnotatedVariantEntry]> = variants
            .iter()
            .map(|(vn, tag, fields)| {
                let field_entries = fields.map(|fields| {
                    fields
                        .iter()
                        .map(|(fn_name, h)| ((*fn_name).clone(), h.0))
                        .collect()
                });
                ((*vn).clone(), *tag, field_entries)
            })
            .collect();
        self.intern(MoveTypeNode::Enum(MoveEnumNode {
            type_: type_tag.clone(),
            variants: variant_entries,
        }))
    }

    /// Recursively intern a tree-based annotated layout.
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
                    .fields
                    .iter()
                    .map(|f| Ok((&f.name, self.intern_tree(&f.layout)?)))
                    .collect::<AResult<Vec<_>>>()?;
                self.struct_layout(&s.type_, &fields)?
            }
            TreeMoveTypeLayout::Enum(e) => {
                let variants = e
                    .variants
                    .iter()
                    .map(|((variant_name, tag), field_layouts)| {
                        let fields: Vec<(&Identifier, LayoutHandle)> = field_layouts
                            .iter()
                            .map(|f| Ok((&f.name, self.intern_tree(&f.layout)?)))
                            .collect::<AResult<_>>()?;
                        Ok((variant_name, *tag, fields))
                    })
                    .collect::<AResult<Vec<_>>>()?;
                let variant_refs: Vec<(&Identifier, u16, Option<&[(&Identifier, LayoutHandle)]>)> =
                    variants
                        .iter()
                        .map(|(vn, tag, fields)| (*vn, *tag, Some(fields.as_slice())))
                        .collect();
                self.enum_layout(&e.type_, &variant_refs)?
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

/// Resolve a [`LayoutRef`] against the pool into a
/// [`MoveLayoutView`] with eagerly resolved type tags and field names.
///
/// Panics if the reference points to an out-of-bounds table index.
pub(crate) fn resolve_ref<'a>(pool: &'a MoveTypeLayoutPool, r: LayoutRef) -> MoveLayoutView<'a> {
    match r.resolve() {
        ResolvedRef::Leaf(leaf) => leaf_to_layout_view(leaf),
        ResolvedRef::Index(idx) => match &pool[idx] {
            MoveTypeNode::Vector(inner) => MoveLayoutView::Vector(MoveVectorView {
                pool,
                element: *inner,
            }),
            MoveTypeNode::Struct(s) => MoveLayoutView::Struct(MoveStructView {
                type_: &s.type_,
                fields: MoveFieldView {
                    pool,
                    fields: &s.fields,
                },
            }),
            MoveTypeNode::Enum(e) => MoveLayoutView::Enum(MoveEnumView {
                pool,
                type_: &e.type_,
                variants: &e.variants,
            }),
        },
    }
}
