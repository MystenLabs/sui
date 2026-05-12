// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Experimental annotated visitor over the *exp* compressed layout family
//! ([`crate::compressed::annotated::ExpMoveTypeLayout`]).
//!
//! Mirrors the design of [`crate::annotated_visitor_unpacked`]: the driver
//! owns the pool `Arc` once at the top, descent is by `Copy` `ExpLayoutRef`,
//! and compound drivers cache raw pointers to the relevant field/variant
//! slices in the pool to avoid re-resolving on every step.

use std::io::{Cursor, Read};
use std::sync::Arc;

use crate::{
    VARIANT_TAG_MAX_VALUE,
    account_address::AccountAddress,
    annotated_visitor::Error,
    compressed::{
        LeafType, VariantTag,
        annotated::{
            ExpLayoutRef, ExpMoveFieldLayout, ExpMoveTypeLayout, ExpMoveTypePool,
            ExpMoveVariantLayout, ResolvedExpLayoutRef,
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
    fn visit_vector(&mut self, d: &mut VecDriver<'_, 'b>) -> Result<Self::Value, Self::Error>;
    fn visit_struct(&mut self, d: &mut StructDriver<'_, 'b>) -> Result<Self::Value, Self::Error>;
    fn visit_variant(&mut self, d: &mut VariantDriver<'_, 'b>) -> Result<Self::Value, Self::Error>;
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

pub struct ValueDriver<'b> {
    cursor: Cursor<&'b [u8]>,
    pool: Arc<ExpMoveTypePool>,
    layout: Option<ExpLayoutRef>,
    start: usize,
}

pub struct VecDriver<'p, 'b> {
    inner: &'p mut ValueDriver<'b>,
    elem: ExpLayoutRef,
    len: u64,
    off: u64,
}

pub struct StructDriver<'p, 'b> {
    inner: &'p mut ValueDriver<'b>,
    /// Cached pointer to the contiguous `pool.fields[range]` slice for this
    /// struct. The pool sits behind an `Arc` held by `inner` and is immutable
    /// for the lifetime of `&self`.
    fields_ptr: *const [ExpMoveFieldLayout],
    off: u16,
}

pub struct VariantDriver<'p, 'b> {
    inner: &'p mut ValueDriver<'b>,
    /// Cached pointer to the variant entry (for cold name/tag access).
    variant_ptr: *const ExpMoveVariantLayout,
    /// Cached pointer to the variant's fields slice (hot path).
    fields_ptr: *const [ExpMoveFieldLayout],
    tag: u16,
    off: u16,
}

// --- ValueDriver ---

impl<'b> ValueDriver<'b> {
    fn new(cursor: Cursor<&'b [u8]>, pool: Arc<ExpMoveTypePool>, layout: ExpLayoutRef) -> Self {
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

    /// Materialize an owned [`ExpMoveTypeLayout`] for the current value.
    /// One `Arc` refcount bump; only when the visitor asks.
    pub fn layout(&self) -> Result<ExpMoveTypeLayout, Error> {
        let root = self.layout.ok_or(Error::NoValueLayout)?;
        Ok(ExpMoveTypeLayout {
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
    pub fn element_layout(&self) -> ExpLayoutRef {
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

    fn fields(&self) -> &[ExpMoveFieldLayout] {
        // SAFETY: `fields_ptr` was derived from a subslice of
        // `inner.pool.fields` (a `Vec<ExpMoveFieldLayout>` held behind an
        // immutable `Arc`) before this driver was constructed. The pool
        // outlives `&self`, and there is no interior mutability anywhere in
        // the layout pool. Recursion goes through `&mut inner` and only
        // mutates disjoint `cursor`/`layout` fields of `ValueDriver`.
        unsafe { &*self.fields_ptr }
    }

    pub fn peek_field(&self) -> Option<(&Identifier, ExpLayoutRef)> {
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

    fn variant_entry(&self) -> &ExpMoveVariantLayout {
        // SAFETY: see `StructDriver::fields`.
        unsafe { &*self.variant_ptr }
    }

    pub fn variant_name(&self) -> &IdentStr {
        self.variant_entry().name.as_ident_str()
    }

    fn fields(&self) -> &[ExpMoveFieldLayout] {
        // SAFETY: see `StructDriver::fields`.
        unsafe { &*self.fields_ptr }
    }

    pub fn peek_field(&self) -> Option<(&Identifier, ExpLayoutRef)> {
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

// =============================================================================
// Top-level entry / dispatch
// =============================================================================

/// Visit `bytes` against `layout` using `visitor`. The caller's `Arc<Pool>`
/// is cloned exactly once on entry; the descent is allocation- and
/// refcount-free.
pub fn visit_deserialize<'b, V: Visitor<'b> + ?Sized>(
    bytes: &'b [u8],
    layout: &ExpMoveTypeLayout,
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
    layout_ref: ExpLayoutRef,
    visitor: &mut V,
) -> Result<V::Value, V::Error> {
    driver.layout = Some(layout_ref);
    driver.start = driver.cursor.position() as usize;

    match layout_ref.resolve() {
        ResolvedExpLayoutRef::Leaf(LeafType::Bool) => match driver.read_exact()? {
            [0] => visitor.visit_bool(driver, false),
            [1] => visitor.visit_bool(driver, true),
            [b] => Err(Error::UnexpectedByte(b).into()),
        },
        ResolvedExpLayoutRef::Leaf(LeafType::U8) => {
            let v = u8::from_le_bytes(driver.read_exact()?);
            visitor.visit_u8(driver, v)
        }
        ResolvedExpLayoutRef::Leaf(LeafType::U16) => {
            let v = u16::from_le_bytes(driver.read_exact()?);
            visitor.visit_u16(driver, v)
        }
        ResolvedExpLayoutRef::Leaf(LeafType::U32) => {
            let v = u32::from_le_bytes(driver.read_exact()?);
            visitor.visit_u32(driver, v)
        }
        ResolvedExpLayoutRef::Leaf(LeafType::U64) => {
            let v = u64::from_le_bytes(driver.read_exact()?);
            visitor.visit_u64(driver, v)
        }
        ResolvedExpLayoutRef::Leaf(LeafType::U128) => {
            let v = u128::from_le_bytes(driver.read_exact()?);
            visitor.visit_u128(driver, v)
        }
        ResolvedExpLayoutRef::Leaf(LeafType::U256) => {
            let v = U256::from_le_bytes(&driver.read_exact()?);
            visitor.visit_u256(driver, v)
        }
        ResolvedExpLayoutRef::Leaf(LeafType::Address) => {
            let v = AccountAddress::new(driver.read_exact()?);
            visitor.visit_address(driver, v)
        }
        ResolvedExpLayoutRef::Leaf(LeafType::Signer) => {
            let v = AccountAddress::new(driver.read_exact()?);
            visitor.visit_signer(driver, v)
        }
        ResolvedExpLayoutRef::Vector(idx) => {
            let elem = driver.pool.vectors[idx as usize];
            visit_vector(driver, elem, visitor)
        }
        ResolvedExpLayoutRef::Struct(idx) => visit_struct(driver, idx, visitor),
        ResolvedExpLayoutRef::Enum(idx) => visit_variant(driver, idx, visitor),
    }
}

fn visit_vector<'b, V: Visitor<'b> + ?Sized>(
    driver: &mut ValueDriver<'b>,
    elem: ExpLayoutRef,
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
    struct_idx: u16,
    visitor: &mut V,
) -> Result<V::Value, V::Error> {
    let fields_ptr: *const [ExpMoveFieldLayout] = {
        let entry = &driver.pool.structs[struct_idx as usize];
        let r = &entry.fields;
        &driver.pool.fields[r.start as usize..r.end as usize]
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
    enum_idx: u16,
    visitor: &mut V,
) -> Result<V::Value, V::Error> {
    let [tag_byte] = driver.read_exact()?;
    if tag_byte > VARIANT_TAG_MAX_VALUE as u8 {
        return Err(Error::UnexpectedVariantTag(tag_byte as usize).into());
    }
    let tag = tag_byte as VariantTag;

    let (variant_ptr, fields_ptr): (*const ExpMoveVariantLayout, *const [ExpMoveFieldLayout]) = {
        let enum_entry = &driver.pool.enums[enum_idx as usize];
        let vr = &enum_entry.variants;
        let mut found: Option<&ExpMoveVariantLayout> = None;
        for i in vr.start..vr.end {
            let v = &driver.pool.variants[i as usize];
            if v.tag == tag {
                found = Some(v);
                break;
            }
        }
        let variant = match found {
            Some(v) => v,
            None => return Err(Error::UnexpectedVariantTag(tag as usize).into()),
        };
        let fields_range = match &variant.fields {
            Some(r) => r,
            None => return Err(Error::NoValueLayout.into()),
        };
        let fslice =
            &driver.pool.fields[fields_range.start as usize..fields_range.end as usize];
        (variant as *const _, fslice as *const _)
    };

    let mut vd = VariantDriver {
        inner: driver,
        variant_ptr,
        fields_ptr,
        tag,
        off: 0,
    };
    let res = visitor.visit_variant(&mut vd)?;
    while vd.skip_field()? {}
    Ok(res)
}
