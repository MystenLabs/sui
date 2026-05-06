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
    Vector(Box<MoveTypeLayout>),
    Struct(Box<MoveStructLayout>),
    Enum(Box<MoveEnumLayout>),
}

/// A compressed layout that is known to be a struct or enum (not a primitive
/// or vector). This mirrors the tree-based [`crate::annotated_value::MoveDatatypeLayout`].
#[derive(Debug, Clone)]
pub(crate) enum MoveDatatypeLayout_ {
    Struct(Box<MoveStructLayout>),
    Enum(Box<MoveEnumLayout>),
}

/// Datatype layout with a reference to the original layout for inflation and conversion.
#[derive(Debug, Clone)]
pub struct MoveDatatypeLayout {
    self_layout: MoveTypeLayout,
    inner: MoveDatatypeLayout_,
}

/// The enum layout with type tag and named variants, as a view into a shared pool.
#[derive(Debug, Clone)]
pub struct MoveEnumLayout {
    type_: StructTag,
    pub(crate) variants: Box<[VariantLayout]>,
}

/// The struct layout with type tag and named fields, as a view into a shared pool.
#[derive(Debug, Clone)]
pub struct MoveStructLayout {
    type_: StructTag,
    pub(crate) fields: MoveFieldsLayout,
}

/// The result of looking up a variant in an annotated enum layout.
#[derive(Debug, Clone)]
pub enum VariantLayout {
    /// The variant's field layout is known.
    Known {
        name: Identifier,
        tag: VariantTag,
        fields: MoveFieldsLayout,
    },
    /// The variant exists but its field layout is not available.
    Unknown { name: Identifier, tag: VariantTag },
}

/// The field layout of a struct or enum variant, as a view into a shared pool.
#[derive(Debug, Clone)]
pub struct MoveFieldsLayout {
    pool: Arc<MoveTypeLayoutPool>,
    fields: Box<[AnnotatedFieldEntry]>,
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

impl TryFrom<&AV::MoveTypeLayout> for MoveTypeLayout {
    type Error = anyhow::Error;
    fn try_from(layout: &AV::MoveTypeLayout) -> Result<Self, Self::Error> {
        let mut b = MoveTypeLayoutBuilder::new();
        let root = b.from_tree(layout)?;
        Ok(b.build(root))
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
                    .iter()
                    .map(|vl| match vl.fields() {
                        Some(fields) => {
                            let field_layouts = fields
                                .fields()
                                .map(|(name, layout)| {
                                    Ok(AV::MoveFieldLayout::new(name.clone(), layout.inflate()?))
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

    /// Inflate back into a tree-based [`AV::MoveDatatypeLayout`].
    pub fn inflate(&self) -> AResult<crate::annotated_value::MoveDatatypeLayout> {
        match &self.inner {
            MoveDatatypeLayout_::Struct(move_struct_layout) => Ok(AV::MoveDatatypeLayout::Struct(
                Box::new(AV::MoveStructLayout {
                    type_: move_struct_layout.type_.clone(),
                    fields: move_struct_layout
                        .fields()
                        .map(|(name, layout)| {
                            Ok(AV::MoveFieldLayout {
                                name: name.clone(),
                                layout: layout.inflate()?,
                            })
                        })
                        .collect::<AResult<_>>()?,
                }),
            )),
            MoveDatatypeLayout_::Enum(move_enum_layout) => {
                let variants = move_enum_layout
                    .variants()
                    .iter()
                    .map(|vl| match vl {
                        VariantLayout::Known { name, tag, fields } => {
                            let field_layouts = fields
                                .fields()
                                .map(|(name, layout)| {
                                    Ok(AV::MoveFieldLayout {
                                        name: name.clone(),
                                        layout: layout.inflate()?,
                                    })
                                })
                                .collect::<AResult<_>>()?;
                            Ok(((name.clone(), *tag), field_layouts))
                        }
                        VariantLayout::Unknown { name, tag } => anyhow::bail!(
                            "cannot inflate enum with unknown variant layout: {} (tag {})",
                            name,
                            tag
                        ),
                    })
                    .collect::<AResult<_>>()?;
                Ok(AV::MoveDatatypeLayout::Enum(Box::new(AV::MoveEnumLayout {
                    type_: move_enum_layout.type_.clone(),
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

    /// Access a field by index, returning `(name, layout)`.
    pub fn field(&self, i: usize) -> Option<(&Identifier, MoveTypeLayout)> {
        self.fields.get(i).map(|entry| {
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

    /// Iterate over all fields as `(name, layout)` pairs.
    pub fn fields(&self) -> impl ExactSizeIterator<Item = (&Identifier, MoveTypeLayout)> {
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
}

// --- MoveStructLayout ---

impl MoveStructLayout {
    /// The struct's type tag.
    pub fn type_(&self) -> &StructTag {
        &self.type_
    }

    /// Check whether this struct's type tag matches the given [`TypeTag`].
    pub fn is_type_tag(&self, t: &TypeTag) -> bool {
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
    pub fn field(&self, i: usize) -> Option<(&Identifier, MoveTypeLayout)> {
        self.fields.field(i)
    }

    /// Iterate over all fields as `(name, layout)` pairs.
    pub fn fields(&self) -> impl ExactSizeIterator<Item = (&Identifier, MoveTypeLayout)> {
        self.fields.fields()
    }
}

impl fmt::Display for MoveStructLayout {
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

impl VariantLayout {
    /// The variant's name.
    pub fn name(&self) -> &Identifier {
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
    pub fn is_type_tag(&self, t: &TypeTag) -> bool {
        matches!(t, TypeTag::Struct(s) if **s == *self.type_())
    }

    /// Number of variants.
    pub fn variant_count(&self) -> usize {
        self.variants.len()
    }

    /// Access a variant by position index.
    pub fn variant(&self, i: usize) -> Option<&VariantLayout> {
        self.variants.get(i)
    }

    /// Find a variant by its tag value.
    pub fn variant_by_tag(&self, tag: VariantTag) -> Option<&VariantLayout> {
        self.variants.iter().find(|vl| vl.tag() == tag)
    }

    /// Iterate over all variants.
    pub fn variants(&self) -> &[VariantLayout] {
        &self.variants
    }
}

impl fmt::Display for MoveEnumLayout {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {{ ", self.type_)?;
        for (i, vl) in self.variants().iter().enumerate() {
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
            MoveTypeNode::Struct(s) => MoveLayoutView::Struct(Box::new(MoveStructLayout {
                type_: s.type_.clone(),
                fields: MoveFieldsLayout {
                    pool: pool.clone(),
                    fields: s.fields.clone(),
                },
            })),
            MoveTypeNode::Enum(e) => MoveLayoutView::Enum(Box::new(MoveEnumLayout {
                type_: e.type_.clone(),
                variants: e
                    .variants
                    .iter()
                    .map(|entry| match &entry.fields {
                        Some(fields) => VariantLayout::Known {
                            name: entry.name.clone(),
                            tag: entry.tag,
                            fields: MoveFieldsLayout {
                                pool: pool.clone(),
                                fields: fields.clone(),
                            },
                        },
                        None => VariantLayout::Unknown {
                            name: entry.name.clone(),
                            tag: entry.tag,
                        },
                    })
                    .collect(),
            })),
        },
    }
}
