// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Experimental annotated visitor where the driver owns the pool `Arc` once
//! at the top and descent is by `LayoutRef` (a `Copy` `u16`-wrapper). Same
//! trait shape as [`crate::annotated_visitor`]; user-visible lifetimes are
//! `'p` (the `&mut` borrow of the inner driver, elision territory) and `'b`
//! (the input bytes, only meaningful if the visitor returns refs into them).
//!
//! Goals:
//!
//! * Zero `Arc` clones in the descent loop. The one clone happens at
//!   [`visit_deserialize`] entry, when the driver takes ownership of the
//!   pool.
//! * No `'a` (pool lifetime) anywhere. The pool is owned data, not borrowed.
//! * `LayoutRef` is `Copy`. Navigation methods hand out borrows of names plus
//!   a `Copy` ref into the pool; recursion accepts the ref by value.

use std::io::{Cursor, Read};
use std::sync::Arc;

use crate::{
    VARIANT_TAG_MAX_VALUE,
    account_address::AccountAddress,
    annotated_visitor::Error,
    compressed::{
        LayoutRef, LeafType, ResolvedRef, VariantTag,
        annotated::{
            AnnotatedFieldEntry, AnnotatedVariantEntry, MoveTypeLayout, MoveTypeLayoutPool,
            MoveTypeNode,
        },
    },
    identifier::{IdentStr, Identifier},
    u256::U256,
};

// =============================================================================
// Visitor / Traversal traits
// =============================================================================

pub trait Visitor<'b> {
    type Value;
    type Error: From<Error>;

    fn visit_u8(&mut self, d: &ValueDriver<'b>, v: u8) -> Result<Self::Value, Self::Error>;
    fn visit_u16(&mut self, d: &ValueDriver<'b>, v: u16) -> Result<Self::Value, Self::Error>;
    fn visit_u32(&mut self, d: &ValueDriver<'b>, v: u32) -> Result<Self::Value, Self::Error>;
    fn visit_u64(&mut self, d: &ValueDriver<'b>, v: u64) -> Result<Self::Value, Self::Error>;
    fn visit_u128(&mut self, d: &ValueDriver<'b>, v: u128) -> Result<Self::Value, Self::Error>;
    fn visit_u256(&mut self, d: &ValueDriver<'b>, v: U256) -> Result<Self::Value, Self::Error>;
    fn visit_bool(&mut self, d: &ValueDriver<'b>, v: bool) -> Result<Self::Value, Self::Error>;
    fn visit_address(
        &mut self,
        d: &ValueDriver<'b>,
        v: AccountAddress,
    ) -> Result<Self::Value, Self::Error>;
    fn visit_signer(
        &mut self,
        d: &ValueDriver<'b>,
        v: AccountAddress,
    ) -> Result<Self::Value, Self::Error>;
    fn visit_vector(
        &mut self,
        d: &mut VecDriver<'_, 'b>,
    ) -> Result<Self::Value, Self::Error>;
    fn visit_struct(
        &mut self,
        d: &mut StructDriver<'_, 'b>,
    ) -> Result<Self::Value, Self::Error>;
    fn visit_variant(
        &mut self,
        d: &mut VariantDriver<'_, 'b>,
    ) -> Result<Self::Value, Self::Error>;
}

pub trait Traversal<'b> {
    type Error: From<Error>;

    fn traverse_u8(&mut self, _: &ValueDriver<'b>, _: u8) -> Result<(), Self::Error> {
        Ok(())
    }
    fn traverse_u16(&mut self, _: &ValueDriver<'b>, _: u16) -> Result<(), Self::Error> {
        Ok(())
    }
    fn traverse_u32(&mut self, _: &ValueDriver<'b>, _: u32) -> Result<(), Self::Error> {
        Ok(())
    }
    fn traverse_u64(&mut self, _: &ValueDriver<'b>, _: u64) -> Result<(), Self::Error> {
        Ok(())
    }
    fn traverse_u128(&mut self, _: &ValueDriver<'b>, _: u128) -> Result<(), Self::Error> {
        Ok(())
    }
    fn traverse_u256(&mut self, _: &ValueDriver<'b>, _: U256) -> Result<(), Self::Error> {
        Ok(())
    }
    fn traverse_bool(&mut self, _: &ValueDriver<'b>, _: bool) -> Result<(), Self::Error> {
        Ok(())
    }
    fn traverse_address(
        &mut self,
        _: &ValueDriver<'b>,
        _: AccountAddress,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    fn traverse_signer(
        &mut self,
        _: &ValueDriver<'b>,
        _: AccountAddress,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    fn traverse_vector(&mut self, d: &mut VecDriver<'_, 'b>) -> Result<(), Self::Error> {
        while d.next_element(self)?.is_some() {}
        Ok(())
    }
    fn traverse_struct(&mut self, d: &mut StructDriver<'_, 'b>) -> Result<(), Self::Error> {
        while d.next_field(self)?.is_some() {}
        Ok(())
    }
    fn traverse_variant(&mut self, d: &mut VariantDriver<'_, 'b>) -> Result<(), Self::Error> {
        while d.next_field(self)?.is_some() {}
        Ok(())
    }
}

impl<'b, T: Traversal<'b> + ?Sized> Visitor<'b> for T {
    type Value = ();
    type Error = T::Error;

    fn visit_u8(&mut self, d: &ValueDriver<'b>, v: u8) -> Result<(), T::Error> {
        self.traverse_u8(d, v)
    }
    fn visit_u16(&mut self, d: &ValueDriver<'b>, v: u16) -> Result<(), T::Error> {
        self.traverse_u16(d, v)
    }
    fn visit_u32(&mut self, d: &ValueDriver<'b>, v: u32) -> Result<(), T::Error> {
        self.traverse_u32(d, v)
    }
    fn visit_u64(&mut self, d: &ValueDriver<'b>, v: u64) -> Result<(), T::Error> {
        self.traverse_u64(d, v)
    }
    fn visit_u128(&mut self, d: &ValueDriver<'b>, v: u128) -> Result<(), T::Error> {
        self.traverse_u128(d, v)
    }
    fn visit_u256(&mut self, d: &ValueDriver<'b>, v: U256) -> Result<(), T::Error> {
        self.traverse_u256(d, v)
    }
    fn visit_bool(&mut self, d: &ValueDriver<'b>, v: bool) -> Result<(), T::Error> {
        self.traverse_bool(d, v)
    }
    fn visit_address(&mut self, d: &ValueDriver<'b>, v: AccountAddress) -> Result<(), T::Error> {
        self.traverse_address(d, v)
    }
    fn visit_signer(&mut self, d: &ValueDriver<'b>, v: AccountAddress) -> Result<(), T::Error> {
        self.traverse_signer(d, v)
    }
    fn visit_vector(&mut self, d: &mut VecDriver<'_, 'b>) -> Result<(), T::Error> {
        self.traverse_vector(d)
    }
    fn visit_struct(&mut self, d: &mut StructDriver<'_, 'b>) -> Result<(), T::Error> {
        self.traverse_struct(d)
    }
    fn visit_variant(&mut self, d: &mut VariantDriver<'_, 'b>) -> Result<(), T::Error> {
        self.traverse_variant(d)
    }
}

pub struct NullTraversal;

impl Traversal<'_> for NullTraversal {
    type Error = Error;
}

// =============================================================================
// Drivers
// =============================================================================

/// The root driver. Owns the cursor and the pool `Arc` for the entire visit.
/// All child drivers borrow this by `&mut`.
pub struct ValueDriver<'b> {
    cursor: Cursor<&'b [u8]>,
    pool: Arc<MoveTypeLayoutPool>,
    /// `LayoutRef` of the value currently being visited, when applicable.
    /// Cleared between primitive visits (so a stale ref isn't observable from
    /// `layout()` calls in the wrong place).
    layout: Option<LayoutRef>,
    start: usize,
}

pub struct VecDriver<'p, 'b> {
    inner: &'p mut ValueDriver<'b>,
    elem: LayoutRef,
    len: u64,
    off: u64,
}

pub struct StructDriver<'p, 'b> {
    inner: &'p mut ValueDriver<'b>,
    /// Cached pointer to the field slice in the pool. The pool sits behind
    /// an `Arc` held by `inner`; the slice is immutable and stays valid for
    /// the lifetime of `&self`. Caching it skips the per-iteration
    /// `pool[struct_idx]` index + `MoveTypeNode::Struct` match.
    fields_ptr: *const [AnnotatedFieldEntry],
    off: u16,
}

pub struct VariantDriver<'p, 'b> {
    inner: &'p mut ValueDriver<'b>,
    /// Cached pointer to the resolved variant entry. Holds the name (cold)
    /// and field slice (hot) without re-resolving through the pool.
    variant_ptr: *const AnnotatedVariantEntry,
    tag: u16,
    off: u16,
}

// --- ValueDriver ---

impl<'b> ValueDriver<'b> {
    fn new(cursor: Cursor<&'b [u8]>, pool: Arc<MoveTypeLayoutPool>, layout: LayoutRef) -> Self {
        let start = cursor.position() as usize;
        Self {
            cursor,
            pool,
            layout: Some(layout),
            start,
        }
    }

    pub fn start(&self) -> usize {
        self.start
    }
    pub fn position(&self) -> usize {
        self.cursor.position() as usize
    }
    pub fn bytes(&self) -> &'b [u8] {
        self.cursor.get_ref()
    }
    pub fn remaining_bytes(&self) -> &'b [u8] {
        &self.cursor.get_ref()[self.position()..]
    }

    /// Materialize an owned [`MoveTypeLayout`] for the current value. This is
    /// the only `Arc` clone after entry, and only when the visitor asks.
    pub fn layout(&self) -> Result<MoveTypeLayout, Error> {
        let root = self.layout.ok_or(Error::NoValueLayout)?;
        Ok(MoveTypeLayout {
            pool: self.pool.clone(),
            root,
        })
    }

    fn read_exact<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let mut buf = [0u8; N];
        self.cursor
            .read_exact(&mut buf)
            .map_err(|_| Error::UnexpectedEof)?;
        Ok(buf)
    }

    fn read_leb128(&mut self) -> Result<u64, Error> {
        leb128::read::unsigned(&mut self.cursor).map_err(|_| Error::UnexpectedEof)
    }

}

// --- VecDriver ---

#[allow(clippy::len_without_is_empty)]
impl<'p, 'b> VecDriver<'p, 'b> {
    pub fn start(&self) -> usize {
        self.inner.start()
    }
    pub fn position(&self) -> usize {
        self.inner.position()
    }
    pub fn bytes(&self) -> &'b [u8] {
        self.inner.bytes()
    }
    pub fn remaining_bytes(&self) -> &'b [u8] {
        self.inner.remaining_bytes()
    }
    pub fn off(&self) -> u64 {
        self.off
    }
    pub fn len(&self) -> u64 {
        self.len
    }
    pub fn has_element(&self) -> bool {
        self.off < self.len
    }
    pub fn element_layout(&self) -> LayoutRef {
        self.elem
    }

    pub fn next_element<V: Visitor<'b> + ?Sized>(
        &mut self,
        visitor: &mut V,
    ) -> Result<Option<V::Value>, V::Error> {
        if self.off >= self.len {
            return Ok(None);
        }
        let res = visit_value(self.inner, self.elem, visitor)?;
        self.off += 1;
        Ok(Some(res))
    }

    pub fn skip_element(&mut self) -> Result<bool, Error> {
        self.next_element(&mut NullTraversal).map(|v| v.is_some())
    }
}

// --- StructDriver ---

impl<'p, 'b> StructDriver<'p, 'b> {
    pub fn start(&self) -> usize {
        self.inner.start()
    }
    pub fn position(&self) -> usize {
        self.inner.position()
    }
    pub fn bytes(&self) -> &'b [u8] {
        self.inner.bytes()
    }
    pub fn remaining_bytes(&self) -> &'b [u8] {
        self.inner.remaining_bytes()
    }
    pub fn off(&self) -> u16 {
        self.off
    }

    fn fields(&self) -> &[AnnotatedFieldEntry] {
        // SAFETY: `fields_ptr` was derived from a slice borrowed out of
        // `inner.pool` (an `Arc<[MoveTypeNode]>`) before this driver was
        // constructed. The pool is held by `inner` for the entire lifetime
        // of `&self` and is immutable (no interior mutability anywhere in
        // the layout pool), so the bytes pointed at remain valid and
        // unaliased-mutably. Recursion goes through `&mut inner` and only
        // mutates `inner.cursor` / `inner.layout`, which live in disjoint
        // fields of `ValueDriver`.
        unsafe { &*self.fields_ptr }
    }

    /// `(name, layout_ref)` for the next field, or `None` if exhausted.
    pub fn peek_field(&self) -> Option<(&Identifier, LayoutRef)> {
        let entry = self.fields().get(self.off as usize)?;
        Some((&entry.name, entry.layout))
    }

    pub fn next_field<V: Visitor<'b> + ?Sized>(
        &mut self,
        visitor: &mut V,
    ) -> Result<Option<V::Value>, V::Error> {
        let layout_ref = match self.fields().get(self.off as usize) {
            Some(entry) => entry.layout,
            None => return Ok(None),
        };
        let res = visit_value(self.inner, layout_ref, visitor)?;
        self.off += 1;
        Ok(Some(res))
    }

    pub fn skip_field(&mut self) -> Result<bool, Error> {
        self.next_field(&mut NullTraversal).map(|v| v.is_some())
    }
}

// --- VariantDriver ---

impl<'p, 'b> VariantDriver<'p, 'b> {
    pub fn start(&self) -> usize {
        self.inner.start()
    }
    pub fn position(&self) -> usize {
        self.inner.position()
    }
    pub fn bytes(&self) -> &'b [u8] {
        self.inner.bytes()
    }
    pub fn remaining_bytes(&self) -> &'b [u8] {
        self.inner.remaining_bytes()
    }
    pub fn off(&self) -> u16 {
        self.off
    }
    pub fn tag(&self) -> u16 {
        self.tag
    }

    fn variant_entry(&self) -> &AnnotatedVariantEntry {
        // SAFETY: see `StructDriver::fields` — the entry sits inside
        // `inner.pool` (an immutable `Arc`-shared slice) and is alive for
        // the lifetime of `&self`.
        unsafe { &*self.variant_ptr }
    }

    pub fn variant_name(&self) -> &IdentStr {
        self.variant_entry().name.as_ident_str()
    }

    fn fields(&self) -> Option<&[AnnotatedFieldEntry]> {
        self.variant_entry().fields.as_deref()
    }

    pub fn peek_field(&self) -> Option<(&Identifier, LayoutRef)> {
        let entry = self.fields()?.get(self.off as usize)?;
        Some((&entry.name, entry.layout))
    }

    pub fn next_field<V: Visitor<'b> + ?Sized>(
        &mut self,
        visitor: &mut V,
    ) -> Result<Option<V::Value>, V::Error> {
        let layout_ref = match self.fields() {
            Some(fs) => match fs.get(self.off as usize) {
                Some(entry) => entry.layout,
                None => return Ok(None),
            },
            None => return Ok(None),
        };
        let res = visit_value(self.inner, layout_ref, visitor)?;
        self.off += 1;
        Ok(Some(res))
    }

    pub fn skip_field(&mut self) -> Result<bool, Error> {
        self.next_field(&mut NullTraversal).map(|v| v.is_some())
    }
}

// =============================================================================
// Top-level entry / dispatch
// =============================================================================

/// Visit `bytes` against `layout` using `visitor`. The caller's `Arc<Pool>`
/// is cloned exactly once on entry; the descent is allocation- and
/// refcount-free.
pub fn visit_deserialize<'b, V: Visitor<'b> + ?Sized>(
    bytes: &'b [u8],
    layout: &MoveTypeLayout,
    visitor: &mut V,
) -> Result<V::Value, V::Error> {
    let cursor = Cursor::new(bytes);
    let mut driver = ValueDriver::new(cursor, layout.pool.clone(), layout.root);
    visit_value(&mut driver, layout.root, visitor)
}

/// Visit a single value at `layout_ref`, reusing the parent driver's cursor +
/// pool. This is the workhorse called by every recursion step.
pub fn visit_value<'b, V: Visitor<'b> + ?Sized>(
    driver: &mut ValueDriver<'b>,
    layout_ref: LayoutRef,
    visitor: &mut V,
) -> Result<V::Value, V::Error> {
    driver.layout = Some(layout_ref);
    driver.start = driver.cursor.position() as usize;

    match layout_ref.resolve() {
        ResolvedRef::Leaf(LeafType::Bool) => match driver.read_exact()? {
            [0] => visitor.visit_bool(driver, false),
            [1] => visitor.visit_bool(driver, true),
            [b] => Err(Error::UnexpectedByte(b).into()),
        },
        ResolvedRef::Leaf(LeafType::U8) => {
            let v = u8::from_le_bytes(driver.read_exact()?);
            visitor.visit_u8(driver, v)
        }
        ResolvedRef::Leaf(LeafType::U16) => {
            let v = u16::from_le_bytes(driver.read_exact()?);
            visitor.visit_u16(driver, v)
        }
        ResolvedRef::Leaf(LeafType::U32) => {
            let v = u32::from_le_bytes(driver.read_exact()?);
            visitor.visit_u32(driver, v)
        }
        ResolvedRef::Leaf(LeafType::U64) => {
            let v = u64::from_le_bytes(driver.read_exact()?);
            visitor.visit_u64(driver, v)
        }
        ResolvedRef::Leaf(LeafType::U128) => {
            let v = u128::from_le_bytes(driver.read_exact()?);
            visitor.visit_u128(driver, v)
        }
        ResolvedRef::Leaf(LeafType::U256) => {
            let v = U256::from_le_bytes(&driver.read_exact()?);
            visitor.visit_u256(driver, v)
        }
        ResolvedRef::Leaf(LeafType::Address) => {
            let v = AccountAddress::new(driver.read_exact()?);
            visitor.visit_address(driver, v)
        }
        ResolvedRef::Leaf(LeafType::Signer) => {
            let v = AccountAddress::new(driver.read_exact()?);
            visitor.visit_signer(driver, v)
        }
        ResolvedRef::Index(idx) => {
            // Inspect the node, copying out the small bits we need before
            // doing further work that needs `&mut driver`.
            let kind = match &driver.pool[idx] {
                MoveTypeNode::Vector(elem) => CompoundKind::Vector(*elem),
                MoveTypeNode::Struct(_) => CompoundKind::Struct,
                MoveTypeNode::Enum(_) => CompoundKind::Enum,
            };
            match kind {
                CompoundKind::Vector(elem) => visit_vector(driver, elem, visitor),
                CompoundKind::Struct => visit_struct(driver, idx, visitor),
                CompoundKind::Enum => visit_variant(driver, idx, visitor),
            }
        }
    }
}

enum CompoundKind {
    Vector(LayoutRef),
    Struct,
    Enum,
}

fn visit_vector<'b, V: Visitor<'b> + ?Sized>(
    driver: &mut ValueDriver<'b>,
    elem: LayoutRef,
    visitor: &mut V,
) -> Result<V::Value, V::Error> {
    let len = driver.read_leb128()?;
    let mut vd = VecDriver {
        inner: driver,
        elem,
        len,
        off: 0,
    };
    let res = visitor.visit_vector(&mut vd)?;
    while vd.skip_element()? {}
    Ok(res)
}

fn visit_struct<'b, V: Visitor<'b> + ?Sized>(
    driver: &mut ValueDriver<'b>,
    struct_idx: usize,
    visitor: &mut V,
) -> Result<V::Value, V::Error> {
    // Resolve the field slice once. The borrow ends here; we keep only the
    // raw pointer, which `StructDriver::fields` derefs under the documented
    // safety contract.
    let fields_ptr: *const [AnnotatedFieldEntry] = match &driver.pool[struct_idx] {
        MoveTypeNode::Struct(s) => &*s.fields,
        _ => unreachable!("visit_struct called with non-struct node"),
    };
    let mut sd = StructDriver {
        inner: driver,
        fields_ptr,
        off: 0,
    };
    let res = visitor.visit_struct(&mut sd)?;
    while sd.skip_field()? {}
    Ok(res)
}

fn visit_variant<'b, V: Visitor<'b> + ?Sized>(
    driver: &mut ValueDriver<'b>,
    enum_idx: usize,
    visitor: &mut V,
) -> Result<V::Value, V::Error> {
    let [tag_byte] = driver.read_exact()?;
    if tag_byte > VARIANT_TAG_MAX_VALUE as u8 {
        return Err(Error::UnexpectedVariantTag(tag_byte as usize).into());
    }
    let tag = tag_byte as VariantTag;
    // Resolve the variant entry once and cache a pointer to it. The borrow
    // ends here; `VariantDriver::variant_entry` derefs the pointer under
    // the safety contract documented there.
    let variant_ptr: *const AnnotatedVariantEntry = {
        let MoveTypeNode::Enum(e) = &driver.pool[enum_idx] else {
            unreachable!("visit_variant called with non-enum node");
        };
        let entry = match e.variants.iter().find(|v| v.tag == tag) {
            Some(v) => v,
            None => return Err(Error::UnexpectedVariantTag(tag as usize).into()),
        };
        if entry.fields.is_none() {
            return Err(Error::NoValueLayout.into());
        }
        entry as *const _
    };
    let mut vd = VariantDriver {
        inner: driver,
        variant_ptr,
        tag,
        off: 0,
    };
    let res = visitor.visit_variant(&mut vd)?;
    while vd.skip_field()? {}
    Ok(res)
}
