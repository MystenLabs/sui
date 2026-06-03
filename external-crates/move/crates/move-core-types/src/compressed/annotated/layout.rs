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

    /// Recursively absorb an existing compressed annotated layout (from any
    /// backend) into this builder, deduplicating shared subtrees against the
    /// builder's pool.
    fn intern_layout<U: TypeLayout>(
        &mut self,
        layout: &MoveTypeLayout<U>,
    ) -> Result<Self::Root, Self::Error> {
        self.intern_view(layout.as_view())
    }

    /// Recursively intern a [`MoveLayoutView`] (a borrowed resolved layout)
    /// into this builder.
    fn intern_view<U: TypeLayout>(
        &mut self,
        view: MoveLayoutView<'_, U>,
    ) -> Result<Self::Root, Self::Error> {
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
                let inner_h = self.intern_view(inner.as_view())?;
                self.vector(inner_h)?
            }
            MoveLayoutView::Struct(s) => {
                let type_tag = s.type_().clone();
                let fields: Vec<(Identifier, Self::Root)> = s
                    .fields()
                    .map(|(name, layout)| Ok((name.clone(), self.intern_view(layout.as_view())?)))
                    .collect::<Result<_, Self::Error>>()?;
                let field_refs: Vec<(&Identifier, Self::Root)> =
                    fields.iter().map(|(n, h)| (n, h.clone())).collect();
                self.struct_layout(&type_tag, &field_refs)?
            }
            MoveLayoutView::Enum(e) => {
                let type_tag = e.type_().clone();
                // First collect (name, tag, optional field handles) so we
                // can build the borrowed slices in a second pass.
                let variants: Vec<(Identifier, VariantTag, Option<Vec<(Identifier, Self::Root)>>)> =
                    e.variants()
                        .map(|v| match v {
                            VariantLayout::Known { name, tag, fields } => {
                                let fs: Vec<(Identifier, Self::Root)> = fields
                                    .fields()
                                    .map(|(n, l)| {
                                        Ok((n.clone(), self.intern_view(l.as_view())?))
                                    })
                                    .collect::<Result<_, Self::Error>>()?;
                                Ok((name.clone(), tag, Some(fs)))
                            }
                            VariantLayout::Unknown { name, tag } => {
                                Ok((name.clone(), tag, None))
                            }
                        })
                        .collect::<Result<_, Self::Error>>()?;
                let variant_field_refs: Vec<Option<Vec<(&Identifier, Self::Root)>>> = variants
                    .iter()
                    .map(|(_, _, fs)| {
                        fs.as_ref()
                            .map(|fs| fs.iter().map(|(n, h)| (n, h.clone())).collect())
                    })
                    .collect();
                let variant_refs: Vec<(&Identifier, VariantTag, Option<&[(&Identifier, Self::Root)]>)> =
                    variants
                        .iter()
                        .zip(variant_field_refs.iter())
                        .map(|((name, tag, _), fs)| (name, *tag, fs.as_deref()))
                        .collect();
                self.enum_layout(&type_tag, &variant_refs)?
            }
        })
    }

    /// Finalize the builder and wrap the result in a [`MoveTypeLayout`].
    fn build(self, root: Self::Root) -> MoveTypeLayout<Self::Output> {
        let pool = self.finalize(root.clone());
        MoveTypeLayout::from_parts(pool, root)
    }

    /// Construct a [`MoveTypeLayout`] by running a closure that builds up a
    /// root handle. Returns the closure's error verbatim.
    fn with_builder<F>(f: F) -> Result<MoveTypeLayout<Self::Output>, Self::Error>
    where
        Self: Default,
        F: FnOnce(&mut Self) -> Result<Self::Root, Self::Error>,
    {
        let mut b = Self::default();
        let root = f(&mut b)?;
        Ok(b.build(root))
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

    /// If this layout is a struct, return a borrowed view onto it.
    pub fn as_struct(&self) -> Option<MoveStructLayout<'_, T>> {
        match self.as_view() {
            MoveLayoutView::Struct(s) => Some(s),
            _ => None,
        }
    }

    /// If this layout is an enum, return a borrowed view onto it.
    pub fn as_enum(&self) -> Option<MoveEnumLayout<'_, T>> {
        match self.as_view() {
            MoveLayoutView::Enum(e) => Some(e),
            _ => None,
        }
    }

    /// If this layout is a struct or enum, return a borrowed datatype view.
    pub fn as_datatype(&self) -> Option<MoveDatatypeLayout<'_, T>> {
        MoveDatatypeLayout::new(self.as_ref())
    }

    /// Returns `true` iff `self` and `other` describe the same Move type,
    /// regardless of pool ordering or how subtrees are shared.
    pub fn equivalent<U: TypeLayout>(&self, other: &MoveTypeLayout<U>) -> bool {
        self.as_ref().equivalent(&other.as_ref())
    }
}

impl<T: TypeLayout> PartialEq for MoveTypeLayout<T> {
    fn eq(&self, other: &Self) -> bool {
        self.equivalent(other)
    }
}

impl<T: TypeLayout> Eq for MoveTypeLayout<T> {}

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

    /// Returns `true` iff the two layouts describe the same Move type,
    /// regardless of pool ordering or how subtrees are shared.
    pub fn equivalent<U: TypeLayout>(&self, other: &MoveTypeLayoutRef<'_, U>) -> bool {
        (*self).as_view().equivalent(&(*other).as_view())
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

    /// Returns `true` iff `self` and `other` describe the same Move type,
    /// regardless of pool ordering or how subtrees are shared.
    pub fn equivalent<U: TypeLayout>(&self, other: &MoveLayoutView<'_, U>) -> bool {
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

impl<T: TypeLayout> PartialEq for MoveLayoutView<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.equivalent(other)
    }
}

impl<T: TypeLayout> Eq for MoveLayoutView<'_, T> {}

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

    /// Returns `true` iff `self` and `other` describe the same datatype,
    /// regardless of pool ordering.
    pub fn equivalent<U: TypeLayout>(&self, other: &MoveDatatypeLayout<'_, U>) -> bool {
        match (self, other) {
            (MoveDatatypeLayout::Struct(a), MoveDatatypeLayout::Struct(b)) => a.equivalent(b),
            (MoveDatatypeLayout::Enum(a), MoveDatatypeLayout::Enum(b)) => a.equivalent(b),
            _ => false,
        }
    }
}

impl<T: TypeLayout> PartialEq for MoveDatatypeLayout<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.equivalent(other)
    }
}

impl<T: TypeLayout> Eq for MoveDatatypeLayout<'_, T> {}

// --- MoveFieldsLayout ---

impl<'a, T: TypeLayout> MoveFieldsLayout<'a, T> {
    /// Number of fields.
    #[inline]
    pub fn field_count(self) -> usize {
        self.fields.len()
    }

    /// Access a field by index, returning `(name, layout)`.
    #[inline]
    pub fn field(self, i: u16) -> Option<(&'a Identifier, MoveTypeLayoutRef<'a, T>)> {
        self.fields.get(i as usize).map(|entry| {
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

    /// Returns `true` iff the two field-lists describe the same fields
    /// (same arity, pairwise-equivalent layouts, identical names),
    /// regardless of pool ordering.
    pub fn equivalent<U: TypeLayout>(&self, other: &MoveFieldsLayout<'_, U>) -> bool {
        if self.field_count() != other.field_count() {
            return false;
        }
        self.fields()
            .zip(other.fields())
            .all(|((na, la), (nb, lb))| na == nb && la.equivalent(&lb))
    }
}

impl<T: TypeLayout> PartialEq for MoveFieldsLayout<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.equivalent(other)
    }
}

impl<T: TypeLayout> Eq for MoveFieldsLayout<'_, T> {}

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
    pub fn field(self, i: u16) -> Option<(&'a Identifier, MoveTypeLayoutRef<'a, T>)> {
        self.fields.field(i)
    }

    /// Iterate over all fields as `(name, layout)` pairs.
    #[inline]
    pub fn fields(
        self,
    ) -> impl ExactSizeIterator<Item = (&'a Identifier, MoveTypeLayoutRef<'a, T>)> {
        self.fields.fields()
    }

    /// Returns `true` iff `self` and `other` describe the same struct type
    /// (same type tag, same fields), regardless of pool ordering.
    pub fn equivalent<U: TypeLayout>(&self, other: &MoveStructLayout<'_, U>) -> bool {
        self.type_ == other.type_ && self.fields.equivalent(&other.fields)
    }
}

impl<T: TypeLayout> PartialEq for MoveStructLayout<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.equivalent(other)
    }
}

impl<T: TypeLayout> Eq for MoveStructLayout<'_, T> {}

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

    /// Access a variant by its tag.
    #[inline]
    pub fn variant(self, tag: VariantTag) -> Option<VariantLayout<'a, T>> {
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

    /// Returns `true` iff `self` and `other` describe the same enum type
    /// (same type tag, same variants with matching names/tags/fields),
    /// regardless of pool ordering.
    pub fn equivalent<U: TypeLayout>(&self, other: &MoveEnumLayout<'_, U>) -> bool {
        if self.type_ != other.type_ {
            return false;
        }
        if self.variant_count() != other.variant_count() {
            return false;
        }
        self.variants().zip(other.variants()).all(|pair| match pair {
            (
                VariantLayout::Unknown {
                    name: na,
                    tag: ta,
                },
                VariantLayout::Unknown {
                    name: nb,
                    tag: tb,
                },
            ) => na == nb && ta == tb,
            (
                VariantLayout::Known {
                    name: na,
                    tag: ta,
                    fields: fa,
                },
                VariantLayout::Known {
                    name: nb,
                    tag: tb,
                    fields: fb,
                },
            ) => na == nb && ta == tb && fa.equivalent(&fb),
            _ => false,
        })
    }
}

impl<T: TypeLayout> PartialEq for MoveEnumLayout<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.equivalent(other)
    }
}

impl<T: TypeLayout> Eq for MoveEnumLayout<'_, T> {}

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
