// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::annotated_value as AV;
pub use crate::compressed::LayoutHandle;
use crate::compressed::{LayoutRef, LeafType, ResolvedRef, VariantTag};
use crate::identifier::Identifier;
use crate::language_storage::{StructTag, TypeTag};
use anyhow::Result as AResult;
use indexmap::IndexSet;
use std::fmt;
use std::sync::Arc;

static EMPTY_POOL: std::sync::LazyLock<Arc<MoveTypeLayoutPool>> =
    std::sync::LazyLock::new(|| Arc::from(Vec::<MoveTypeNode>::new()));

// --- Node types ---

/// A named field entry: field name paired with its layout reference.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AnnotatedFieldEntry {
    pub name: Identifier,
    pub layout: LayoutRef,
}

/// A single variant entry in an enum node.
/// `None` fields means the variant exists but its field layout is unknown.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AnnotatedVariantEntry {
    pub name: Identifier,
    pub tag: VariantTag,
    pub fields: Option<Box<[AnnotatedFieldEntry]>>,
}

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

/// A deduplicated, flat representation of an annotated [`AV::MoveTypeLayout`] tree.
/// Names and type tags are stored inline in nodes. Cloning is cheap — the
/// node table is shared via `Arc`.
///
/// NOTE: `Eq`/`PartialEq`/`Hash` are intentionally not derived. Two layouts
/// representing the same type may have different pool orderings (node
/// permutations), so structural equality on the raw fields would produce
/// false negatives. Compare by inflating to tree form or by comparing views.
#[derive(Debug, Clone)]
pub struct MoveTypeLayout {
    pool: Arc<MoveTypeLayoutPool>,
    root: LayoutRef,
}

/// Borrowed compact annotated layout backed by an existing node pool.
#[derive(Debug, Clone, Copy)]
pub struct MoveTypeLayoutRef<'a> {
    pool: &'a Arc<MoveTypeLayoutPool>,
    root: LayoutRef,
}

// --- View types (all borrowed, Copy) ---

/// A resolved view of an annotated layout node.
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
    Vector(MoveTypeLayoutRef<'a>),
    Struct(MoveStructLayout<'a>),
    Enum(MoveEnumLayout<'a>),
}

/// A compressed layout that is known to be a struct or enum (not a primitive
/// or vector). This mirrors the tree-based [`crate::annotated_value::MoveDatatypeLayout`].
#[derive(Debug, Clone, Copy)]
pub enum MoveDatatypeLayout<'a> {
    Struct(MoveStructLayout<'a>),
    Enum(MoveEnumLayout<'a>),
}

/// The enum layout with type tag and named variants, as a view into a shared pool.
#[derive(Debug, Clone, Copy)]
pub struct MoveEnumLayout<'a> {
    type_: &'a StructTag,
    variants: &'a [AnnotatedVariantEntry],
    pool: &'a Arc<MoveTypeLayoutPool>,
}

/// The struct layout with type tag and named fields, as a view into a shared pool.
#[derive(Debug, Clone, Copy)]
pub struct MoveStructLayout<'a> {
    type_: &'a StructTag,
    fields: MoveFieldsLayout<'a>,
}

/// The result of looking up a variant in an annotated enum layout.
#[derive(Debug, Clone, Copy)]
pub enum VariantLayout<'a> {
    /// The variant's field layout is known.
    Known {
        name: &'a Identifier,
        tag: VariantTag,
        fields: MoveFieldsLayout<'a>,
    },
    /// The variant exists but its field layout is not available.
    Unknown {
        name: &'a Identifier,
        tag: VariantTag,
    },
}

/// The field layout of a struct or enum variant, as a view into a shared pool.
#[derive(Debug, Clone, Copy)]
pub struct MoveFieldsLayout<'a> {
    pool: &'a Arc<MoveTypeLayoutPool>,
    fields: &'a [AnnotatedFieldEntry],
}

// --- Builder type ---

/// Incrementally builds an annotated [`MoveTypeLayout`] with automatic
/// deduplication of nodes.
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
        self.as_ref().node_count()
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

    /// Borrow this layout without cloning the pool.
    pub fn as_ref(&self) -> MoveTypeLayoutRef<'_> {
        MoveTypeLayoutRef {
            pool: &self.pool,
            root: self.root,
        }
    }

    /// Create a resolved view for navigating this layout.
    pub fn as_view(&self) -> MoveLayoutView<'_> {
        self.as_ref().as_view()
    }

    /// Inflate back into a tree-based [`AV::MoveTypeLayout`].
    pub fn inflate(&self) -> AResult<AV::MoveTypeLayout> {
        self.as_ref().inflate()
    }
}

impl fmt::Display for MoveTypeLayout {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() {
            write!(f, "{:#}", self.as_ref())
        } else {
            write!(f, "{}", self.as_ref())
        }
    }
}

impl TryFrom<&AV::MoveTypeLayout> for MoveTypeLayout {
    type Error = anyhow::Error;
    fn try_from(layout: &AV::MoveTypeLayout) -> Result<Self, Self::Error> {
        let mut b = MoveTypeLayoutBuilder::new();
        let root = b.from_tree(layout)?;
        Ok(b.build(root))
    }
}

// --- MoveTypeLayoutRef (borrowed root) ---

impl<'a> MoveTypeLayoutRef<'a> {
    /// Clone the underlying `Arc` to produce an owned layout. Cheap — only a
    /// refcount bump.
    pub fn to_owned(self) -> MoveTypeLayout {
        MoveTypeLayout {
            pool: self.pool.clone(),
            root: self.root,
        }
    }

    /// Number of compound nodes in the table (excludes inline leaf types).
    pub fn node_count(self) -> usize {
        self.pool.len()
    }

    /// Create a resolved view for navigating this layout.
    pub fn as_view(self) -> MoveLayoutView<'a> {
        resolve_ref(self.pool, self.root)
    }

    /// Inflate back into a tree-based [`AV::MoveTypeLayout`].
    pub fn inflate(self) -> AResult<AV::MoveTypeLayout> {
        self.as_view().inflate()
    }
}

impl<'a> From<&'a MoveTypeLayout> for MoveTypeLayoutRef<'a> {
    fn from(layout: &'a MoveTypeLayout) -> Self {
        layout.as_ref()
    }
}

impl fmt::Display for MoveTypeLayoutRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() {
            write!(f, "{:#}", self.as_view())
        } else {
            write!(f, "{}", self.as_view())
        }
    }
}

// --- MoveLayoutView ---

impl<'a> MoveLayoutView<'a> {
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
                        Ok(AV::MoveFieldLayout::new(name.clone(), layout.inflate()?))
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
                    .map(|vl| match vl {
                        VariantLayout::Known { name, tag, fields } => {
                            let field_layouts = fields
                                .fields()
                                .map(|(n, layout)| {
                                    Ok(AV::MoveFieldLayout::new(n.clone(), layout.inflate()?))
                                })
                                .collect::<AResult<_>>()?;
                            Ok(((name.clone(), tag), field_layouts))
                        }
                        VariantLayout::Unknown { .. } => {
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
    pub fn is_type_tag(&self, t: &TypeTag) -> bool {
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
            MoveLayoutView::Struct(sv) => sv.is_type_tag(t),
            MoveLayoutView::Vector(vv) => {
                if let TypeTag::Vector(inner) = t {
                    vv.as_view().is_type_tag(inner)
                } else {
                    false
                }
            }
            MoveLayoutView::Enum(ev) => ev.is_type_tag(t),
        }
    }
}

impl From<MoveLayoutView<'_>> for TypeTag {
    fn from(view: MoveLayoutView<'_>) -> TypeTag {
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
            MoveLayoutView::Vector(vv) if f.alternate() => write!(f, "vector<{vv:#}>"),
            MoveLayoutView::Vector(vv) => write!(f, "vector<{vv}>"),
            MoveLayoutView::Struct(sv) if f.alternate() => write!(f, "{sv:#}"),
            MoveLayoutView::Struct(sv) => write!(f, "{sv}"),
            MoveLayoutView::Enum(ev) if f.alternate() => write!(f, "{ev:#}"),
            MoveLayoutView::Enum(ev) => write!(f, "{ev}"),
        }
    }
}

// --- MoveDatatypeLayout ---

impl<'a> MoveDatatypeLayout<'a> {
    /// Wrap a borrowed layout that is known to be a struct or enum.
    /// Returns `None` if the layout is a primitive or vector.
    pub fn new(layout: MoveTypeLayoutRef<'a>) -> Option<Self> {
        match layout.as_view() {
            MoveLayoutView::Struct(s) => Some(MoveDatatypeLayout::Struct(s)),
            MoveLayoutView::Enum(e) => Some(MoveDatatypeLayout::Enum(e)),
            _ => None,
        }
    }

    /// Inflate back into a tree-based [`AV::MoveDatatypeLayout`].
    pub fn inflate(self) -> AResult<AV::MoveDatatypeLayout> {
        match self {
            MoveDatatypeLayout::Struct(sv) => Ok(AV::MoveDatatypeLayout::Struct(Box::new(
                AV::MoveStructLayout {
                    type_: sv.type_().clone(),
                    fields: sv
                        .fields()
                        .map(|(name, layout)| {
                            Ok(AV::MoveFieldLayout {
                                name: name.clone(),
                                layout: layout.inflate()?,
                            })
                        })
                        .collect::<AResult<_>>()?,
                },
            ))),
            MoveDatatypeLayout::Enum(ev) => {
                let variants = ev
                    .variants()
                    .map(|vl| match vl {
                        VariantLayout::Known { name, tag, fields } => {
                            let field_layouts = fields
                                .fields()
                                .map(|(n, layout)| {
                                    Ok(AV::MoveFieldLayout {
                                        name: n.clone(),
                                        layout: layout.inflate()?,
                                    })
                                })
                                .collect::<AResult<_>>()?;
                            Ok(((name.clone(), tag), field_layouts))
                        }
                        VariantLayout::Unknown { name, tag } => anyhow::bail!(
                            "cannot inflate enum with unknown variant layout: {} (tag {})",
                            name,
                            tag
                        ),
                    })
                    .collect::<AResult<_>>()?;
                Ok(AV::MoveDatatypeLayout::Enum(Box::new(AV::MoveEnumLayout {
                    type_: ev.type_().clone(),
                    variants,
                })))
            }
        }
    }
}

// --- MoveFieldsLayout ---

impl<'a> MoveFieldsLayout<'a> {
    /// Number of fields.
    pub fn field_count(self) -> usize {
        self.fields.len()
    }

    /// Access a field by index, returning `(name, layout)`.
    pub fn field(self, i: usize) -> Option<(&'a Identifier, MoveTypeLayoutRef<'a>)> {
        self.fields.get(i).map(|entry| {
            (
                &entry.name,
                MoveTypeLayoutRef {
                    pool: self.pool,
                    root: entry.layout,
                },
            )
        })
    }

    /// Look up a field by name, returning its layout.
    pub fn field_by_name(self, name: &str) -> Option<MoveTypeLayoutRef<'a>> {
        self.fields
            .iter()
            .find(|entry| entry.name.as_str() == name)
            .map(|entry| MoveTypeLayoutRef {
                pool: self.pool,
                root: entry.layout,
            })
    }

    /// Iterate over all fields as `(name, layout)` pairs.
    pub fn fields(self) -> impl ExactSizeIterator<Item = (&'a Identifier, MoveTypeLayoutRef<'a>)> {
        let pool = self.pool;
        self.fields.iter().map(move |entry| {
            (
                &entry.name,
                MoveTypeLayoutRef {
                    pool,
                    root: entry.layout,
                },
            )
        })
    }
}

// --- MoveStructLayout ---

impl<'a> MoveStructLayout<'a> {
    /// The struct's type tag.
    pub fn type_(self) -> &'a StructTag {
        self.type_
    }

    /// Check whether this struct's type tag matches the given [`TypeTag`].
    pub fn is_type_tag(self, t: &TypeTag) -> bool {
        matches!(t, TypeTag::Struct(s) if &**s == self.type_)
    }

    /// Access the fields layout.
    pub fn fields_layout(self) -> MoveFieldsLayout<'a> {
        self.fields
    }

    /// Number of fields.
    pub fn field_count(self) -> usize {
        self.fields.field_count()
    }

    /// Access a field by index, returning `(name, layout)`.
    pub fn field(self, i: usize) -> Option<(&'a Identifier, MoveTypeLayoutRef<'a>)> {
        self.fields.field(i)
    }

    /// Iterate over all fields as `(name, layout)` pairs.
    pub fn fields(self) -> impl ExactSizeIterator<Item = (&'a Identifier, MoveTypeLayoutRef<'a>)> {
        self.fields.fields()
    }
}

impl fmt::Display for MoveStructLayout<'_> {
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

// --- VariantLayout ---

impl<'a> VariantLayout<'a> {
    /// The variant's name.
    pub fn name(self) -> &'a Identifier {
        match self {
            VariantLayout::Known { name, .. } => name,
            VariantLayout::Unknown { name, .. } => name,
        }
    }

    /// The variant's tag.
    pub fn tag(self) -> VariantTag {
        match self {
            VariantLayout::Known { tag, .. } => tag,
            VariantLayout::Unknown { tag, .. } => tag,
        }
    }

    /// The variant's fields, or `None` if the layout is unknown.
    pub fn fields(self) -> Option<MoveFieldsLayout<'a>> {
        match self {
            VariantLayout::Known { fields, .. } => Some(fields),
            VariantLayout::Unknown { .. } => None,
        }
    }
}

// --- MoveEnumLayout ---

impl<'a> MoveEnumLayout<'a> {
    /// The enum's type tag.
    pub fn type_(self) -> &'a StructTag {
        self.type_
    }

    /// Check whether this enum's type tag matches the given [`TypeTag`].
    pub fn is_type_tag(self, t: &TypeTag) -> bool {
        matches!(t, TypeTag::Struct(s) if **s == *self.type_)
    }

    /// Number of variants.
    pub fn variant_count(self) -> usize {
        self.variants.len()
    }

    /// Access a variant by position index.
    pub fn variant(self, i: usize) -> Option<VariantLayout<'a>> {
        self.variants.get(i).map(|v| variant_view(self.pool, v))
    }

    /// Find a variant by its tag value.
    pub fn variant_by_tag(self, tag: VariantTag) -> Option<VariantLayout<'a>> {
        self.variants
            .iter()
            .find(|v| v.tag == tag)
            .map(|v| variant_view(self.pool, v))
    }

    /// Iterate over all variants.
    pub fn variants(self) -> impl ExactSizeIterator<Item = VariantLayout<'a>> {
        let pool = self.pool;
        self.variants.iter().map(move |v| variant_view(pool, v))
    }
}

fn variant_view<'a>(
    pool: &'a Arc<MoveTypeLayoutPool>,
    v: &'a AnnotatedVariantEntry,
) -> VariantLayout<'a> {
    match &v.fields {
        Some(fields) => VariantLayout::Known {
            name: &v.name,
            tag: v.tag,
            fields: MoveFieldsLayout { pool, fields },
        },
        None => VariantLayout::Unknown {
            name: &v.name,
            tag: v.tag,
        },
    }
}

impl fmt::Display for MoveEnumLayout<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {{ ", self.type_)?;
        for (i, vl) in self.variants().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}(", vl.name())?;
            match vl.fields() {
                Some(fields) => {
                    for (j, (name, layout)) in fields.fields().enumerate() {
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
                None => write!(f, "?")?,
            }
            write!(f, ")")?;
        }
        write!(f, " }}")
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
        type_tag: &StructTag,
        fields: &[(&Identifier, LayoutHandle)],
    ) -> AResult<LayoutHandle> {
        let fields: Box<[AnnotatedFieldEntry]> = fields
            .iter()
            .map(|(name, h)| AnnotatedFieldEntry {
                name: (*name).clone(),
                layout: h.0,
            })
            .collect();
        self.add_node(MoveTypeNode::Struct(MoveStructNode {
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
        variants: &[(
            &Identifier,
            VariantTag,
            Option<&[(&Identifier, LayoutHandle)]>,
        )],
    ) -> AResult<LayoutHandle> {
        let variant_entries: Box<[AnnotatedVariantEntry]> = variants
            .iter()
            .map(|(vn, tag, fields)| {
                let field_entries = fields.map(|fields| {
                    fields
                        .iter()
                        .map(|(fn_name, h)| AnnotatedFieldEntry {
                            name: (*fn_name).clone(),
                            layout: h.0,
                        })
                        .collect()
                });
                AnnotatedVariantEntry {
                    name: (*vn).clone(),
                    tag: *tag,
                    fields: field_entries,
                }
            })
            .collect();
        self.add_node(MoveTypeNode::Enum(MoveEnumNode {
            type_: type_tag.clone(),
            variants: variant_entries,
        }))
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
            AV::MoveTypeLayout::Struct(s) => {
                let fields = s
                    .fields
                    .iter()
                    .map(|f| Ok((&f.name, self.from_tree(&f.layout)?)))
                    .collect::<AResult<Vec<_>>>()?;
                self.struct_layout(&s.type_, &fields)?
            }
            AV::MoveTypeLayout::Enum(e) => {
                let variants = e
                    .variants
                    .iter()
                    .map(|((variant_name, tag), field_layouts)| {
                        let fields: Vec<(&Identifier, LayoutHandle)> = field_layouts
                            .iter()
                            .map(|f| Ok((&f.name, self.from_tree(&f.layout)?)))
                            .collect::<AResult<_>>()?;
                        Ok((variant_name, *tag, fields))
                    })
                    .collect::<AResult<Vec<_>>>()?;
                let variant_refs: Vec<(
                    &Identifier,
                    VariantTag,
                    Option<&[(&Identifier, LayoutHandle)]>,
                )> = variants
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

fn leaf_to_layout_view<'a>(leaf: LeafType) -> MoveLayoutView<'a> {
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
fn resolve_ref<'a>(pool: &'a Arc<MoveTypeLayoutPool>, r: LayoutRef) -> MoveLayoutView<'a> {
    match r.resolve() {
        ResolvedRef::Leaf(leaf) => leaf_to_layout_view(leaf),
        ResolvedRef::Index(idx) => match &pool[idx] {
            MoveTypeNode::Vector(inner) => {
                MoveLayoutView::Vector(MoveTypeLayoutRef { pool, root: *inner })
            }
            MoveTypeNode::Struct(s) => MoveLayoutView::Struct(MoveStructLayout {
                type_: &s.type_,
                fields: MoveFieldsLayout {
                    pool,
                    fields: &s.fields,
                },
            }),
            MoveTypeNode::Enum(e) => MoveLayoutView::Enum(MoveEnumLayout {
                type_: &e.type_,
                variants: &e.variants,
                pool,
            }),
        },
    }
}
