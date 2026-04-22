// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::compressed::backend::DefaultRuntime;
use crate::runtime_value as RV;
use anyhow::Result as AResult;
use std::fmt;

/// The default compressed-runtime layout builder. Alias so most users can just write
/// `MoveTypeLayoutBuilder::new()`.
///
/// To use a different backend, name its concrete builder type directly (e.g.
/// `RuntimeBoxPoolBuilder`) — call sites are otherwise identical.
pub use crate::compressed::backend::DefaultRuntimeBuilder as MoveTypeLayoutBuilder;

/// Handle returned by [`MoveTypeLayoutBuilder`] for nodes interned into the
/// default backend. Type alias over the builder's `Root` so consumers can
/// traffic in a single name without naming the backend's internal ref type.
pub type LayoutHandle = <MoveTypeLayoutBuilder as BackendBuilder>::Root;

// =============================================================================
// Trait: TypeLayout
// =============================================================================

/// A backing store for compressed runtime layouts. Implementors decide how
/// roots are encoded and where node data lives.
///
/// This trait is **per-flavor** rather than shared with the annotated flavor
/// because each flavor returns its own concrete `MoveLayoutView<'_, Self>`,
/// and a shared trait with a GAT `type View<'a>` runs into the stable-Rust
/// interaction between the GAT's implicit `Self: 'a` bound and HRTB use sites
/// (e.g. `Display` on the owned `MoveTypeLayout<T>`), which forces
/// `T: 'static` — too restrictive for backends that hold borrowed data.
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

/// A deduplicated, flat representation of a [`RV::MoveTypeLayout`] tree,
/// generic over the backend `T`.
///
/// NOTE: `Eq`/`PartialEq`/`Hash` are intentionally not derived. Two layouts
/// representing the same type may have different pool orderings (node
/// permutations), so structural equality on the raw fields would produce
/// false negatives. Compare by inflating to tree form or by comparing views.
#[derive(Debug, Clone)]
pub struct MoveTypeLayout<T: TypeLayout = DefaultRuntime> {
    pool: T,
    root: T::Root,
}

/// Borrowed view onto a [`MoveTypeLayout`] without cloning the pool.
#[derive(Debug)]
pub struct MoveTypeLayoutRef<'a, T: TypeLayout = DefaultRuntime> {
    pub(crate) pool: &'a T,
    pub(crate) root: &'a T::Root,
}

// =============================================================================
// View types (all borrowed, Copy)
// =============================================================================

/// A resolved view of a layout node.
#[derive(Debug)]
pub enum MoveLayoutView<'a, T: TypeLayout = DefaultRuntime> {
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

/// The enum layout of an enum type, as a view into a shared pool.
#[derive(Debug)]
pub struct MoveEnumLayout<'a, T: TypeLayout = DefaultRuntime> {
    pub(crate) pool: &'a T,
    pub(crate) variants: &'a [Option<Box<[T::Root]>>],
}

/// The struct layout of a struct type, as a view into a shared pool.
#[derive(Debug)]
pub struct MoveStructLayout<'a, T: TypeLayout = DefaultRuntime> {
    pub(crate) fields: MoveFieldsLayout<'a, T>,
}

/// The result of looking up a variant in an enum view.
#[derive(Debug)]
pub enum VariantLayout<'a, T: TypeLayout = DefaultRuntime> {
    /// The variant's field layout is known.
    Known(MoveFieldsLayout<'a, T>),
    /// The variant exists but its field layout is not available.
    Unknown,
}

/// The field layout of a struct or enum variant, as a view into a shared pool.
#[derive(Debug)]
pub struct MoveFieldsLayout<'a, T: TypeLayout = DefaultRuntime> {
    pub(crate) pool: &'a T,
    pub(crate) fields: &'a [T::Root],
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
    MoveEnumLayout,
    MoveStructLayout,
    VariantLayout,
    MoveFieldsLayout,
);

// =============================================================================
// Builder trait + generic builder
// =============================================================================

/// Backend-write-side abstraction: each method constructs a layout root for
/// one Move type-constructor. Each concrete backend (see e.g.
/// [`crate::compressed::backend::arc_pool`]) decides how to allocate, encode,
/// or deduplicate the root and where to store any compound node data.
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
    fn struct_layout(&mut self, fields: &[Self::Root]) -> Result<Self::Root, Self::Error>;
    fn enum_layout(
        &mut self,
        variants: &[Option<&[Self::Root]>],
    ) -> Result<Self::Root, Self::Error>;

    /// Finalize the builder into an immutable [`TypeLayout`] backend.
    fn finalize(self, root: Self::Root) -> Self::Output;

    /// Recursively intern a tree-based layout, deduplicating shared subtrees.
    /// Tree-based enum layouts always have known variants, so all variants are
    /// wrapped in `Some`.
    fn intern_tree(&mut self, layout: &RV::MoveTypeLayout) -> Result<Self::Root, Self::Error> {
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
                let inner_h = self.intern_tree(inner)?;
                self.vector(inner_h)?
            }
            RV::MoveTypeLayout::Struct(s) => {
                let fields = s
                    .fields()
                    .iter()
                    .map(|f| self.intern_tree(f))
                    .collect::<Result<Vec<_>, _>>()?;
                self.struct_layout(&fields)?
            }
            RV::MoveTypeLayout::Enum(e) => {
                let variant_handles =
                    e.0.iter()
                        .map(|v| {
                            v.iter()
                                .map(|f| self.intern_tree(f))
                                .collect::<Result<Vec<_>, _>>()
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                let variant_refs: Vec<Option<&[Self::Root]>> =
                    variant_handles.iter().map(|v| Some(v.as_slice())).collect();
                self.enum_layout(&variant_refs)?
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
// Display helper
// =============================================================================

struct DebugAsDisplay<'a, T>(&'a T);

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
}

impl<T: TypeLayout> MoveTypeLayout<T> {
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

    /// Reconstruct the equivalent tree-based layout. Returns an error
    /// if any enum variant has an unknown layout.
    pub fn inflate(&self) -> AResult<RV::MoveTypeLayout> {
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

    /// Reconstruct the equivalent tree-based layout. Returns an error
    /// if any enum variant has an unknown layout.
    pub fn inflate(self) -> AResult<RV::MoveTypeLayout> {
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
            MoveLayoutView::Struct(sv) => {
                let fields = sv
                    .fields
                    .fields()
                    .map(|f| f.inflate())
                    .collect::<AResult<_>>()?;
                RV::MoveTypeLayout::Struct(Box::new(RV::MoveStructLayout::new(fields)))
            }
            MoveLayoutView::Enum(ev) => {
                let variants = ev
                    .variants()
                    .map(|vl| match vl {
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

// --- MoveFieldsLayout ---

impl<'a, T: TypeLayout> MoveFieldsLayout<'a, T> {
    /// Number of fields.
    #[inline]
    pub fn field_count(self) -> usize {
        self.fields.len()
    }

    /// Access a field by index.
    #[inline]
    pub fn field(self, i: usize) -> Option<MoveTypeLayoutRef<'a, T>> {
        self.fields.get(i).map(|f| MoveTypeLayoutRef {
            pool: self.pool,
            root: f,
        })
    }

    /// Iterate over all fields as layouts.
    #[inline]
    pub fn fields(self) -> impl ExactSizeIterator<Item = MoveTypeLayoutRef<'a, T>> {
        self.fields.iter().map(move |f| MoveTypeLayoutRef {
            pool: self.pool,
            root: f,
        })
    }
}

// --- MoveStructLayout ---

impl<'a, T: TypeLayout> MoveStructLayout<'a, T> {
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

    /// Access a field by index.
    #[inline]
    pub fn field(self, i: usize) -> Option<MoveTypeLayoutRef<'a, T>> {
        self.fields.field(i)
    }

    /// Iterate over all fields as layouts.
    #[inline]
    pub fn fields(self) -> impl ExactSizeIterator<Item = MoveTypeLayoutRef<'a, T>> {
        self.fields.fields()
    }
}

impl<T: TypeLayout> fmt::Display for MoveStructLayout<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "struct {}", self.fields)
    }
}

impl<T: TypeLayout> fmt::Display for MoveFieldsLayout<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use DebugAsDisplay as DD;

        let mut map = f.debug_map();
        for (i, field) in self.fields.iter().enumerate() {
            map.entry(
                &i,
                &DD(&MoveTypeLayoutRef {
                    pool: self.pool,
                    root: field,
                }),
            );
        }
        map.finish()
    }
}

// --- MoveEnumLayout ---

impl<'a, T: TypeLayout> MoveEnumLayout<'a, T> {
    /// Number of variants.
    #[inline]
    pub fn variant_count(self) -> usize {
        self.variants.len()
    }

    /// Access a variant by index.
    #[inline]
    pub fn variant(self, i: usize) -> Option<VariantLayout<'a, T>> {
        self.variants.get(i).map(|v| make_variant(self.pool, v))
    }

    /// Iterate over all variants.
    #[inline]
    pub fn variants(self) -> impl ExactSizeIterator<Item = VariantLayout<'a, T>> {
        let pool = self.pool;
        self.variants.iter().map(move |v| make_variant(pool, v))
    }
}

#[inline]
fn make_variant<'a, T: TypeLayout>(
    pool: &'a T,
    v: &'a Option<Box<[T::Root]>>,
) -> VariantLayout<'a, T> {
    match v {
        Some(fields) => VariantLayout::Known(MoveFieldsLayout { pool, fields }),
        None => VariantLayout::Unknown,
    }
}

impl<T: TypeLayout> fmt::Display for MoveEnumLayout<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "enum ")?;
        for (tag, vl) in self.variants().enumerate() {
            write!(f, "variant_tag: {} {{ ", tag)?;
            match vl {
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
