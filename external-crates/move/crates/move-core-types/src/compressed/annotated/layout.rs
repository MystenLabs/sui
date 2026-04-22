// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::annotated_value as AV;
use crate::compressed::VariantTag;
use crate::compressed::backend::DefaultAnnotated;
use crate::compressed::backend::annotated_nodes::{AnnotatedFieldEntry, AnnotatedVariantEntry};
use crate::identifier::Identifier;
use crate::language_storage::{StructTag, TypeTag};
use anyhow::Result as AResult;
use std::fmt;

/// The default compressed-annotated layout builder. Alias so most users can just write
/// `MoveTypeLayoutBuilder::new()`.
///
/// To use a different backend, name its concrete builder type directly (e.g.
/// `AnnotatedBoxPoolBuilder`) — call sites are otherwise identical.
pub use crate::compressed::backend::DefaultAnnotatedBuilder as MoveTypeLayoutBuilder;

/// Handle returned by [`MoveTypeLayoutBuilder`] for nodes interned into the
/// default backend. Type alias over the builder's `Root` so consumers can
/// traffic in a single name without naming the backend's internal ref type.
pub type LayoutHandle = <MoveTypeLayoutBuilder as BackendBuilder>::Root;

// =============================================================================
// Trait: TypeLayout
// =============================================================================

/// A backing store for compressed annotated layouts. Implementors decide how
/// roots are encoded and where node data lives.
///
/// This trait is **per-flavor** rather than shared with the runtime flavor;
/// see the doc comment on `runtime::TypeLayout` for the GAT/HRTB rationale.
pub trait TypeLayout: Sized {
    /// Cheap-to-`Clone` reference into the backend's storage. Typically `Copy`
    /// (e.g. a packed index) but not required to be — Arc-handle backends use
    /// a `Clone`-only `Root` carrying a refcounted pointer.
    type Root: Clone + fmt::Debug;

    /// Resolve a root to its resolved view at that node. `r` is borrowed so
    /// the returned view can borrow into data the `Root` keeps alive (e.g.
    /// an `Arc<TreeNode>` inside the root).
    fn realize_view<'a>(&'a self, r: &'a Self::Root) -> MoveLayoutView<'a, Self>;

    /// Number of compound nodes accessible through this backend.
    fn node_count(&self) -> usize;
}

// =============================================================================
// Owned and borrowed layout types
// =============================================================================

/// A deduplicated, flat representation of an annotated [`AV::MoveTypeLayout`]
/// tree, generic over the backend `T`.
///
/// NOTE: `Eq`/`PartialEq`/`Hash` are intentionally not derived. Two layouts
/// representing the same type may have different pool orderings (node
/// permutations), so structural equality on the raw fields would produce
/// false negatives. Compare by inflating to tree form or by comparing views.
#[derive(Debug, Clone)]
pub struct MoveTypeLayout<T: TypeLayout = DefaultAnnotated> {
    pool: T,
    root: T::Root,
}

/// Borrowed view onto a [`MoveTypeLayout`] without cloning the pool.
#[derive(Debug)]
pub struct MoveTypeLayoutRef<'a, T: TypeLayout = DefaultAnnotated> {
    pub(crate) pool: &'a T,
    pub(crate) root: &'a T::Root,
}

// =============================================================================
// View types (all borrowed, Copy)
// =============================================================================

/// A resolved view of an annotated layout node.
#[derive(Debug)]
pub enum MoveLayoutView<'a, T: TypeLayout = DefaultAnnotated> {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Address,
    Signer,
    Vector(MoveTypeLayoutRef<'a, T>),
    Struct(MoveStructLayout<'a, T>),
    Enum(MoveEnumLayout<'a, T>),
}

/// A compressed layout that is known to be a struct or enum (not a primitive
/// or vector). This mirrors the tree-based [`crate::annotated_value::MoveDatatypeLayout`].
#[derive(Debug)]
pub enum MoveDatatypeLayout<'a, T: TypeLayout = DefaultAnnotated> {
    Struct(MoveStructLayout<'a, T>),
    Enum(MoveEnumLayout<'a, T>),
}

/// The enum layout with type tag and named variants, as a view into a shared pool.
#[derive(Debug)]
pub struct MoveEnumLayout<'a, T: TypeLayout = DefaultAnnotated> {
    pub(crate) type_: &'a StructTag,
    pub(crate) variants: &'a [AnnotatedVariantEntry<T::Root>],
    pub(crate) pool: &'a T,
}

/// The struct layout with type tag and named fields, as a view into a shared pool.
#[derive(Debug)]
pub struct MoveStructLayout<'a, T: TypeLayout = DefaultAnnotated> {
    pub(crate) type_: &'a StructTag,
    pub(crate) fields: MoveFieldsLayout<'a, T>,
}

/// The result of looking up a variant in an annotated enum layout.
#[derive(Debug)]
pub enum VariantLayout<'a, T: TypeLayout = DefaultAnnotated> {
    /// The variant's field layout is known.
    Known {
        name: &'a Identifier,
        tag: VariantTag,
        fields: MoveFieldsLayout<'a, T>,
    },
    /// The variant exists but its field layout is not available.
    Unknown {
        name: &'a Identifier,
        tag: VariantTag,
    },
}

/// The field layout of a struct or enum variant, as a view into a shared pool.
#[derive(Debug)]
pub struct MoveFieldsLayout<'a, T: TypeLayout = DefaultAnnotated> {
    pub(crate) pool: &'a T,
    pub(crate) fields: &'a [AnnotatedFieldEntry<T::Root>],
}

// `#[derive(Copy, Clone)]` would over-constrain to `T: Copy`; these are all
// `&'a T` plus Copy fields, so they're unconditionally Copy.
macro_rules! impl_copy_clone {
    ($($t:ident),* $(,)?) => { $(
        impl<T: TypeLayout> Clone for $t<'_, T> { fn clone(&self) -> Self { *self } }
        impl<T: TypeLayout> Copy for $t<'_, T> {}
    )* };
}
impl_copy_clone!(
    MoveTypeLayoutRef,
    MoveLayoutView,
    MoveDatatypeLayout,
    MoveEnumLayout,
    MoveStructLayout,
    VariantLayout,
    MoveFieldsLayout,
);

// =============================================================================
// Builder trait + generic builder
// =============================================================================

/// Backend-write-side abstraction for the annotated flavor. Mirrors
/// [`crate::compressed::runtime::BackendBuilder`] but with annotated-specific
/// signatures for `struct_layout`/`enum_layout` (carry type tags + names).
///
/// `intern_tree` and `build` are provided as default methods so callers can drive
/// any backend builder directly without an extra wrapper.
pub trait BackendBuilder: Sized {
    /// Cheap-to-`Clone` reference into the backend's storage (matches
    /// `<Self::Output as TypeLayout>::Root`).
    type Root: Clone + fmt::Debug;
    /// The TypeLayout backend produced by `finalize`.
    type Output: TypeLayout<Root = Self::Root>;
    /// Errors raised by compound constructors (e.g. capacity-limit failures).
    type Error;

    fn bool(&mut self) -> Self::Root;
    fn u8(&mut self) -> Self::Root;
    fn u16(&mut self) -> Self::Root;
    fn u32(&mut self) -> Self::Root;
    fn u64(&mut self) -> Self::Root;
    fn u128(&mut self) -> Self::Root;
    fn u256(&mut self) -> Self::Root;
    fn address(&mut self) -> Self::Root;
    fn signer(&mut self) -> Self::Root;

    fn vector(&mut self, element: Self::Root) -> Result<Self::Root, Self::Error>;

    fn struct_layout(
        &mut self,
        type_tag: &StructTag,
        fields: &[(&Identifier, Self::Root)],
    ) -> Result<Self::Root, Self::Error>;

    fn enum_layout(
        &mut self,
        type_tag: &StructTag,
        variants: &[(
            &Identifier,
            VariantTag,
            Option<&[(&Identifier, Self::Root)]>,
        )],
    ) -> Result<Self::Root, Self::Error>;

    fn finalize(self, root: Self::Root) -> Self::Output;

    /// Recursively intern a tree-based annotated layout. Tree-based enum
    /// layouts always have known variants, so all variants are wrapped in `Some`.
    fn intern_tree(&mut self, layout: &AV::MoveTypeLayout) -> Result<Self::Root, Self::Error> {
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
                let inner_h = self.intern_tree(inner)?;
                self.vector(inner_h)?
            }
            AV::MoveTypeLayout::Struct(s) => {
                let fields = s
                    .fields
                    .iter()
                    .map(|f| Ok((&f.name, self.intern_tree(&f.layout)?)))
                    .collect::<Result<Vec<_>, _>>()?;
                self.struct_layout(&s.type_, &fields)?
            }
            AV::MoveTypeLayout::Enum(e) => {
                let variants = e
                    .variants
                    .iter()
                    .map(|((variant_name, tag), field_layouts)| {
                        let fields: Vec<(&Identifier, Self::Root)> = field_layouts
                            .iter()
                            .map(|f| Ok((&f.name, self.intern_tree(&f.layout)?)))
                            .collect::<Result<_, _>>()?;
                        Ok((variant_name, *tag, fields))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let variant_refs: Vec<(
                    &Identifier,
                    VariantTag,
                    Option<&[(&Identifier, Self::Root)]>,
                )> = variants
                    .iter()
                    .map(|(vn, tag, fields)| (*vn, *tag, Some(fields.as_slice())))
                    .collect();
                self.enum_layout(&e.type_, &variant_refs)?
            }
        })
    }

    /// Finalize the builder and wrap the result in a [`MoveTypeLayout`].
    fn build(self, root: Self::Root) -> MoveTypeLayout<Self::Output> {
        let pool = self.finalize(root.clone());
        MoveTypeLayout::from_parts(pool, root)
    }
}

// =============================================================================
// Implementations
// =============================================================================

// --- MoveTypeLayout ---

impl<T: TypeLayout> MoveTypeLayout<T> {
    /// Construct a layout from its raw parts. Used by backends to build their
    /// concrete instantiations from a finalized pool + root.
    pub fn from_parts(pool: T, root: T::Root) -> Self {
        MoveTypeLayout { pool, root }
    }

    /// Number of compound nodes accessible through the backend.
    pub fn node_count(&self) -> usize {
        self.pool.node_count()
    }

    /// Borrow this layout without cloning the pool.
    #[inline]
    pub fn as_ref(&self) -> MoveTypeLayoutRef<'_, T> {
        MoveTypeLayoutRef {
            pool: &self.pool,
            root: &self.root,
        }
    }

    /// Create a resolved view for navigating this layout.
    #[inline]
    pub fn as_view(&self) -> MoveLayoutView<'_, T> {
        self.as_ref().as_view()
    }

    /// Inflate back into a tree-based [`AV::MoveTypeLayout`].
    pub fn inflate(&self) -> AResult<AV::MoveTypeLayout> {
        self.as_ref().inflate()
    }
}

impl<T: TypeLayout> fmt::Display for MoveTypeLayout<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() {
            write!(f, "{:#}", self.as_ref())
        } else {
            write!(f, "{}", self.as_ref())
        }
    }
}

// --- MoveTypeLayoutRef ---

impl<'a, T: TypeLayout> MoveTypeLayoutRef<'a, T> {
    /// Construct a borrowed layout from a pool reference and a root reference.
    #[inline]
    pub fn new(pool: &'a T, root: &'a T::Root) -> Self {
        MoveTypeLayoutRef { pool, root }
    }

    /// Clone the backend to produce an owned layout (cheap when the backend's
    /// `Clone` impl is itself cheap, e.g. an `Arc` refcount bump).
    pub fn to_owned(self) -> MoveTypeLayout<T>
    where
        T: Clone,
    {
        MoveTypeLayout {
            pool: self.pool.clone(),
            root: self.root.clone(),
        }
    }

    /// Number of compound nodes accessible through the backend.
    pub fn node_count(self) -> usize {
        self.pool.node_count()
    }

    /// Create a resolved view for navigating this layout.
    #[inline]
    pub fn as_view(self) -> MoveLayoutView<'a, T> {
        self.pool.realize_view(self.root)
    }

    /// Inflate back into a tree-based [`AV::MoveTypeLayout`].
    pub fn inflate(self) -> AResult<AV::MoveTypeLayout> {
        self.as_view().inflate()
    }
}

impl<'a, T: TypeLayout> From<&'a MoveTypeLayout<T>> for MoveTypeLayoutRef<'a, T> {
    fn from(layout: &'a MoveTypeLayout<T>) -> Self {
        layout.as_ref()
    }
}

impl<T: TypeLayout> fmt::Display for MoveTypeLayoutRef<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() {
            write!(f, "{:#}", self.as_view())
        } else {
            write!(f, "{}", self.as_view())
        }
    }
}

// --- MoveLayoutView ---

impl<T: TypeLayout> MoveLayoutView<'_, T> {
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
    #[inline]
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

impl<T: TypeLayout> From<MoveLayoutView<'_, T>> for TypeTag {
    fn from(view: MoveLayoutView<'_, T>) -> TypeTag {
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

impl<T: TypeLayout> fmt::Display for MoveLayoutView<'_, T> {
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

impl<'a, T: TypeLayout> MoveDatatypeLayout<'a, T> {
    /// Wrap a borrowed layout that is known to be a struct or enum.
    /// Returns `None` if the layout is a primitive or vector.
    #[inline]
    pub fn new(layout: MoveTypeLayoutRef<'a, T>) -> Option<Self> {
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

impl<'a, T: TypeLayout> MoveFieldsLayout<'a, T> {
    /// Number of fields.
    #[inline]
    pub fn field_count(self) -> usize {
        self.fields.len()
    }

    /// Access a field by index, returning `(name, layout)`.
    #[inline]
    pub fn field(self, i: usize) -> Option<(&'a Identifier, MoveTypeLayoutRef<'a, T>)> {
        self.fields.get(i).map(|entry| {
            (
                &entry.name,
                MoveTypeLayoutRef {
                    pool: self.pool,
                    root: &entry.layout,
                },
            )
        })
    }

    /// Look up a field by name, returning its layout.
    #[inline]
    pub fn field_by_name(self, name: &str) -> Option<MoveTypeLayoutRef<'a, T>> {
        self.fields
            .iter()
            .find(|entry| entry.name.as_str() == name)
            .map(|entry| MoveTypeLayoutRef {
                pool: self.pool,
                root: &entry.layout,
            })
    }

    /// Iterate over all fields as `(name, layout)` pairs.
    #[inline]
    pub fn fields(
        self,
    ) -> impl ExactSizeIterator<Item = (&'a Identifier, MoveTypeLayoutRef<'a, T>)> {
        let pool = self.pool;
        self.fields.iter().map(move |entry| {
            (
                &entry.name,
                MoveTypeLayoutRef {
                    pool,
                    root: &entry.layout,
                },
            )
        })
    }
}

// --- MoveStructLayout ---

impl<'a, T: TypeLayout> MoveStructLayout<'a, T> {
    /// The struct's type tag.
    #[inline]
    pub fn type_(self) -> &'a StructTag {
        self.type_
    }

    /// Check whether this struct's type tag matches the given [`TypeTag`].
    #[inline]
    pub fn is_type_tag(self, t: &TypeTag) -> bool {
        matches!(t, TypeTag::Struct(s) if &**s == self.type_)
    }

    /// Access the fields layout.
    #[inline]
    pub fn fields_layout(self) -> MoveFieldsLayout<'a, T> {
        self.fields
    }

    /// Number of fields.
    #[inline]
    pub fn field_count(self) -> usize {
        self.fields.field_count()
    }

    /// Access a field by index, returning `(name, layout)`.
    #[inline]
    pub fn field(self, i: usize) -> Option<(&'a Identifier, MoveTypeLayoutRef<'a, T>)> {
        self.fields.field(i)
    }

    /// Iterate over all fields as `(name, layout)` pairs.
    #[inline]
    pub fn fields(
        self,
    ) -> impl ExactSizeIterator<Item = (&'a Identifier, MoveTypeLayoutRef<'a, T>)> {
        self.fields.fields()
    }
}

impl<T: TypeLayout> fmt::Display for MoveStructLayout<'_, T> {
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

impl<'a, T: TypeLayout> VariantLayout<'a, T> {
    /// The variant's name.
    #[inline]
    pub fn name(self) -> &'a Identifier {
        match self {
            VariantLayout::Known { name, .. } => name,
            VariantLayout::Unknown { name, .. } => name,
        }
    }

    /// The variant's tag.
    #[inline]
    pub fn tag(self) -> VariantTag {
        match self {
            VariantLayout::Known { tag, .. } => tag,
            VariantLayout::Unknown { tag, .. } => tag,
        }
    }

    /// The variant's fields, or `None` if the layout is unknown.
    #[inline]
    pub fn fields(self) -> Option<MoveFieldsLayout<'a, T>> {
        match self {
            VariantLayout::Known { fields, .. } => Some(fields),
            VariantLayout::Unknown { .. } => None,
        }
    }
}

// --- MoveEnumLayout ---

impl<'a, T: TypeLayout> MoveEnumLayout<'a, T> {
    /// The enum's type tag.
    #[inline]
    pub fn type_(self) -> &'a StructTag {
        self.type_
    }

    /// Check whether this enum's type tag matches the given [`TypeTag`].
    #[inline]
    pub fn is_type_tag(self, t: &TypeTag) -> bool {
        matches!(t, TypeTag::Struct(s) if **s == *self.type_)
    }

    /// Number of variants.
    #[inline]
    pub fn variant_count(self) -> usize {
        self.variants.len()
    }

    /// Access a variant by position index.
    #[inline]
    pub fn variant(self, i: usize) -> Option<VariantLayout<'a, T>> {
        self.variants.get(i).map(|v| variant_view(self.pool, v))
    }

    /// Find a variant by its tag value.
    #[inline]
    pub fn variant_by_tag(self, tag: VariantTag) -> Option<VariantLayout<'a, T>> {
        self.variants
            .iter()
            .find(|v| v.tag == tag)
            .map(|v| variant_view(self.pool, v))
    }

    /// Iterate over all variants.
    #[inline]
    pub fn variants(self) -> impl ExactSizeIterator<Item = VariantLayout<'a, T>> {
        let pool = self.pool;
        self.variants.iter().map(move |v| variant_view(pool, v))
    }
}

#[inline]
fn variant_view<'a, T: TypeLayout>(
    pool: &'a T,
    v: &'a AnnotatedVariantEntry<T::Root>,
) -> VariantLayout<'a, T> {
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

impl<T: TypeLayout> fmt::Display for MoveEnumLayout<'_, T> {
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
