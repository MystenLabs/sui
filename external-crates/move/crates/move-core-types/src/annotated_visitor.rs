// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::io::{Cursor, Read};

use crate::{
    account_address::AccountAddress,
    annotated_value::{MoveEnumLayout, MoveFieldLayout, MoveStructLayout, MoveTypeLayout},
    identifier::IdentStr,
    u256::U256,
    VARIANT_COUNT_MAX,
};

/// Visitors can be used for building values out of a serialized Move struct or value.
pub trait Visitor<'b, 'l> {
    type Value;

    /// Visitors can return any error as long as it can represent an error from the visitor itself.
    /// The easiest way to achieve this is to use `thiserror`:
    ///
    /// ```rust,no_doc
    /// #[derive(thiserror::Error)]
    /// enum Error {
    ///     #[error(transparent)]
    ///     Visitor(#[from] annotated_visitor::Error)
    ///
    ///     // Custom error variants ...
    /// }
    /// ```
    type Error: From<Error>;

    fn visit_u8(
        &mut self,
        driver: &ValueDriver<'_, 'b, 'l>,
        value: u8,
    ) -> Result<Self::Value, Self::Error>;

    fn visit_u16(
        &mut self,
        driver: &ValueDriver<'_, 'b, 'l>,
        value: u16,
    ) -> Result<Self::Value, Self::Error>;

    fn visit_u32(
        &mut self,
        driver: &ValueDriver<'_, 'b, 'l>,
        value: u32,
    ) -> Result<Self::Value, Self::Error>;

    fn visit_u64(
        &mut self,
        driver: &ValueDriver<'_, 'b, 'l>,
        value: u64,
    ) -> Result<Self::Value, Self::Error>;

    fn visit_u128(
        &mut self,
        driver: &ValueDriver<'_, 'b, 'l>,
        value: u128,
    ) -> Result<Self::Value, Self::Error>;

    fn visit_u256(
        &mut self,
        driver: &ValueDriver<'_, 'b, 'l>,
        value: U256,
    ) -> Result<Self::Value, Self::Error>;

    fn visit_bool(
        &mut self,
        driver: &ValueDriver<'_, 'b, 'l>,
        value: bool,
    ) -> Result<Self::Value, Self::Error>;

    fn visit_address(
        &mut self,
        driver: &ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Self::Value, Self::Error>;

    fn visit_signer(
        &mut self,
        driver: &ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Self::Value, Self::Error>;

    fn visit_vector(
        &mut self,
        driver: &mut VecDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error>;

    fn visit_struct(
        &mut self,
        driver: &mut StructDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error>;

    fn visit_variant(
        &mut self,
        driver: &mut VariantDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error>;
}

/// A traversal is a special kind of visitor that doesn't return any values. The trait comes with
/// default implementations for every variant that do nothing, allowing an implementor to focus on
/// only the cases they care about.
///
/// Note that the default implementation for structs and vectors recurse down into their elements. A
/// traversal that doesn't want to look inside structs and vectors needs to provide a custom
/// implementation with an empty body:
///
/// ```rust,no_run
/// fn traverse_vector(&mut self, _: &mut VecDriver) -> Result<(), Self::Error> {
///     Ok(())
/// }
/// ```
pub trait Traversal<'b, 'l> {
    type Error: From<Error>;

    fn traverse_u8(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        _value: u8,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn traverse_u16(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        _value: u16,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn traverse_u32(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        _value: u32,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn traverse_u64(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        _value: u64,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn traverse_u128(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        _value: u128,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn traverse_u256(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        _value: U256,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn traverse_bool(
        &mut self,
        _driver: &ValueDriver<'_, 'b, 'l>,
        _value: bool,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn traverse_address(
        &mut self,
        _: &ValueDriver<'_, 'b, 'l>,
        _: AccountAddress,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn traverse_signer(
        &mut self,
        _: &ValueDriver<'_, 'b, 'l>,
        _: AccountAddress,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn traverse_vector(&mut self, driver: &mut VecDriver<'_, 'b, 'l>) -> Result<(), Self::Error> {
        while driver.next_element(self)?.is_some() {}
        Ok(())
    }

    fn traverse_struct(
        &mut self,
        driver: &mut StructDriver<'_, 'b, 'l>,
    ) -> Result<(), Self::Error> {
        while driver.next_field(self)?.is_some() {}
        Ok(())
    }

    fn traverse_variant(
        &mut self,
        driver: &mut VariantDriver<'_, 'b, 'l>,
    ) -> Result<(), Self::Error> {
        while driver.next_field(self)?.is_some() {}
        Ok(())
    }
}

/// Default implementation converting any traversal into a visitor.
impl<'b, 'l, T: Traversal<'b, 'l> + ?Sized> Visitor<'b, 'l> for T {
    type Value = ();
    type Error = T::Error;

    fn visit_u8(
        &mut self,
        driver: &ValueDriver<'_, 'b, 'l>,
        value: u8,
    ) -> Result<Self::Value, Self::Error> {
        self.traverse_u8(driver, value)
    }

    fn visit_u16(
        &mut self,
        driver: &ValueDriver<'_, 'b, 'l>,
        value: u16,
    ) -> Result<Self::Value, Self::Error> {
        self.traverse_u16(driver, value)
    }

    fn visit_u32(
        &mut self,
        driver: &ValueDriver<'_, 'b, 'l>,
        value: u32,
    ) -> Result<Self::Value, Self::Error> {
        self.traverse_u32(driver, value)
    }

    fn visit_u64(
        &mut self,
        driver: &ValueDriver<'_, 'b, 'l>,
        value: u64,
    ) -> Result<Self::Value, Self::Error> {
        self.traverse_u64(driver, value)
    }

    fn visit_u128(
        &mut self,
        driver: &ValueDriver<'_, 'b, 'l>,
        value: u128,
    ) -> Result<Self::Value, Self::Error> {
        self.traverse_u128(driver, value)
    }

    fn visit_u256(
        &mut self,
        driver: &ValueDriver<'_, 'b, 'l>,
        value: U256,
    ) -> Result<Self::Value, Self::Error> {
        self.traverse_u256(driver, value)
    }

    fn visit_bool(
        &mut self,
        driver: &ValueDriver<'_, 'b, 'l>,
        value: bool,
    ) -> Result<Self::Value, Self::Error> {
        self.traverse_bool(driver, value)
    }

    fn visit_address(
        &mut self,
        driver: &ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        self.traverse_address(driver, value)
    }

    fn visit_signer(
        &mut self,
        driver: &ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        self.traverse_signer(driver, value)
    }

    fn visit_vector(
        &mut self,
        driver: &mut VecDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        self.traverse_vector(driver)
    }

    fn visit_struct(
        &mut self,
        driver: &mut StructDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        self.traverse_struct(driver)
    }

    fn visit_variant(
        &mut self,
        driver: &mut VariantDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        self.traverse_variant(driver)
    }
}

/// Exposes information about the byte stream that the value being visited came from, namely the
/// bytes themselves, and the offset at which the value starts. Also exposes the layout of the
/// value being visited.
pub struct ValueDriver<'c, 'b, 'l> {
    bytes: &'c mut Cursor<&'b [u8]>,
    layout: Option<&'l MoveTypeLayout>,
    start: usize,
}

/// Exposes information about a vector being visited (the element layout) to a visitor
/// implementation, and allows that visitor to progress the traversal (by visiting or skipping
/// elements).
pub struct VecDriver<'c, 'b, 'l> {
    inner: ValueDriver<'c, 'b, 'l>,
    layout: &'l MoveTypeLayout,
    len: u64,
    off: u64,
}

/// Exposes information about a struct being visited (its layout, details about the next field to be
/// visited) to a visitor implementation, and allows that visitor to progress the traversal (by
/// visiting or skipping fields).
pub struct StructDriver<'c, 'b, 'l> {
    inner: ValueDriver<'c, 'b, 'l>,
    layout: &'l MoveStructLayout,
    off: u64,
}

/// Exposes information about a variant being visited (its layout, details about the next field to
/// be visited, the variant's tag, and name) to a visitor implementation, and allows that visitor
/// to progress the traversal (by visiting or skipping fields).
pub struct VariantDriver<'c, 'b, 'l> {
    inner: ValueDriver<'c, 'b, 'l>,
    layout: &'l MoveEnumLayout,
    tag: u16,
    variant_name: &'l IdentStr,
    variant_layout: &'l [MoveFieldLayout],
    off: u64,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("unexpected end of input")]
    UnexpectedEof,

    #[error("unexpected byte: {0}")]
    UnexpectedByte(u8),

    #[error("trailing {0} byte(s) at the end of input")]
    TrailingBytes(usize),

    #[error("invalid variant tag: {0}")]
    UnexpectedVariantTag(usize),

    #[error("no layout available for value")]
    NoValueLayout,
}

/// The null traversal implements `Traversal` and `Visitor` but without doing anything (does not
/// return a value, and does not modify any state). This is useful for skipping over parts of the
/// value structure.
pub struct NullTraversal;

impl<'b, 'l> Traversal<'b, 'l> for NullTraversal {
    type Error = Error;
}

impl<'c, 'b, 'l> ValueDriver<'c, 'b, 'l> {
    pub(crate) fn new(bytes: &'c mut Cursor<&'b [u8]>, layout: Option<&'l MoveTypeLayout>) -> Self {
        let start = bytes.position() as usize;
        Self {
            bytes,
            layout,
            start,
        }
    }

    /// The offset at which the value being visited starts in the byte stream.
    pub fn start(&self) -> usize {
        self.start
    }

    /// The current position in the byte stream.
    pub fn position(&self) -> usize {
        self.bytes.position() as usize
    }

    /// All the bytes in the byte stream (including the ones that have been read).
    pub fn bytes(&self) -> &'b [u8] {
        self.bytes.get_ref()
    }
    ///
    /// The bytes that haven't been consumed by the visitor yet.
    pub fn remaining_bytes(&self) -> &'b [u8] {
        &self.bytes.get_ref()[self.position()..]
    }

    /// Type layout for the value being visited. May produce an error if a layout was not supplied
    /// when the driver was created (which should only happen if the driver was created for
    /// visiting a struct specifically).
    pub fn layout(&self) -> Result<&'l MoveTypeLayout, Error> {
        self.layout.ok_or(Error::NoValueLayout)
    }

    fn read_exact<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let mut buf = [0u8; N];
        self.bytes
            .read_exact(&mut buf)
            .map_err(|_| Error::UnexpectedEof)?;
        Ok(buf)
    }

    fn read_leb128(&mut self) -> Result<u64, Error> {
        leb128::read::unsigned(self.bytes).map_err(|_| Error::UnexpectedEof)
    }
}

#[allow(clippy::len_without_is_empty)]
impl<'c, 'b, 'l> VecDriver<'c, 'b, 'l> {
    fn new(inner: ValueDriver<'c, 'b, 'l>, layout: &'l MoveTypeLayout, len: u64) -> Self {
        Self {
            inner,
            layout,
            len,
            off: 0,
        }
    }

    /// The offset at which the value being visited starts in the byte stream.
    pub fn start(&self) -> usize {
        self.inner.start()
    }

    /// The current position in the byte stream.
    pub fn position(&self) -> usize {
        self.inner.position()
    }

    /// All the bytes in the byte stream (including the ones that have been read).
    pub fn bytes(&self) -> &'b [u8] {
        self.inner.bytes()
    }

    /// The bytes that haven't been consumed by the visitor yet.
    pub fn remaining_bytes(&self) -> &'b [u8] {
        self.inner.remaining_bytes()
    }

    /// Type layout for the vector's inner type.
    pub fn element_layout(&self) -> &'l MoveTypeLayout {
        self.layout
    }

    /// The number of elements in this vector that have been visited so far.
    pub fn off(&self) -> u64 {
        self.off
    }

    /// The number of elements in this vector.
    pub fn len(&self) -> u64 {
        self.len
    }

    /// Returns whether or not there are more elements to visit in this vector.
    pub fn has_element(&self) -> bool {
        self.off < self.len
    }

    /// Visit the next element in the vector. The driver accepts a visitor to use for this element,
    /// allowing the visitor to be changed on recursive calls or even between elements in the same
    /// vector.
    ///
    /// Returns `Ok(None)` if there are no more elements in the vector, `Ok(v)` if there was an
    /// element and it was successfully visited (where `v` is the value returned by the visitor) or
    /// an error if there was an underlying deserialization error, or an error during visitation.
    pub fn next_element<V: Visitor<'b, 'l> + ?Sized>(
        &mut self,
        visitor: &mut V,
    ) -> Result<Option<V::Value>, V::Error> {
        Ok(if self.off >= self.len {
            None
        } else {
            let res = visit_value(self.inner.bytes, self.layout, visitor)?;
            self.off += 1;
            Some(res)
        })
    }

    /// Skip the next element in this vector. Returns whether there was an element to skip or not on
    /// success, or an error if there was an underlying deserialization error.
    pub fn skip_element(&mut self) -> Result<bool, Error> {
        self.next_element(&mut NullTraversal).map(|v| v.is_some())
    }
}

impl<'c, 'b, 'l> StructDriver<'c, 'b, 'l> {
    fn new(inner: ValueDriver<'c, 'b, 'l>, layout: &'l MoveStructLayout) -> Self {
        Self {
            inner,
            layout,
            off: 0,
        }
    }

    /// The offset at which the value being visited starts in the byte stream.
    pub fn start(&self) -> usize {
        self.inner.start()
    }

    /// The current position in the byte stream.
    pub fn position(&self) -> usize {
        self.inner.position()
    }

    /// All the bytes in the byte stream (including the ones that have been read).
    pub fn bytes(&self) -> &'b [u8] {
        self.inner.bytes()
    }

    /// The bytes that haven't been consumed by the visitor yet.
    pub fn remaining_bytes(&self) -> &'b [u8] {
        self.inner.remaining_bytes()
    }

    /// The layout of the struct being visited.
    pub fn struct_layout(&self) -> &'l MoveStructLayout {
        self.layout
    }

    /// The number of fields in this struct that have been visited so far.
    pub fn off(&self) -> u64 {
        self.off
    }

    /// The layout of the next field to be visited (if there is one), or `None` otherwise.
    pub fn peek_field(&self) -> Option<&'l MoveFieldLayout> {
        self.layout.fields.get(self.off as usize)
    }

    /// Visit the next field in the struct. The driver accepts a visitor to use for this field,
    /// allowing the visitor to be changed on recursive calls or even between fields in the same
    /// struct.
    ///
    /// Returns `Ok(None)` if there are no more fields in the struct, `Ok((f, v))` if there was an
    /// field and it was successfully visited (where `v` is the value returned by the visitor, and
    /// `f` is the layout of the field that was visited) or an error if there was an underlying
    /// deserialization error, or an error during visitation.
    pub fn next_field<V: Visitor<'b, 'l> + ?Sized>(
        &mut self,
        visitor: &mut V,
    ) -> Result<Option<(&'l MoveFieldLayout, V::Value)>, V::Error> {
        let Some(field) = self.peek_field() else {
            return Ok(None);
        };

        let res = visit_value(self.inner.bytes, &field.layout, visitor)?;
        self.off += 1;
        Ok(Some((field, res)))
    }

    /// Skip the next field. Returns the layout of the field that was visited if there was one, or
    /// `None` if there was none. Can return an error if there was a deserialization error.
    pub fn skip_field(&mut self) -> Result<Option<&'l MoveFieldLayout>, Error> {
        self.next_field(&mut NullTraversal)
            .map(|res| res.map(|(f, _)| f))
    }
}

impl<'c, 'b, 'l> VariantDriver<'c, 'b, 'l> {
    fn new(
        inner: ValueDriver<'c, 'b, 'l>,
        layout: &'l MoveEnumLayout,
        variant_layout: &'l [MoveFieldLayout],
        variant_name: &'l IdentStr,
        tag: u16,
    ) -> Self {
        Self {
            inner,
            layout,
            tag,
            variant_name,
            variant_layout,
            off: 0,
        }
    }

    /// The offset at which the value being visited starts in the byte stream.
    pub fn start(&self) -> usize {
        self.inner.start()
    }

    /// The current position in the byte stream.
    pub fn position(&self) -> usize {
        self.inner.position()
    }

    /// All the bytes in the byte stream (including the ones that have been read).
    pub fn bytes(&self) -> &'b [u8] {
        self.inner.bytes()
    }

    /// The bytes that haven't been consumed by the visitor yet.
    pub fn remaining_bytes(&self) -> &'b [u8] {
        self.inner.remaining_bytes()
    }

    /// The layout of the enum being visited.
    pub fn enum_layout(&self) -> &'l MoveEnumLayout {
        self.layout
    }

    /// The layout of the variant being visited.
    pub fn variant_layout(&self) -> &'l [MoveFieldLayout] {
        self.variant_layout
    }

    /// The tag of the variant being visited.
    pub fn tag(&self) -> u16 {
        self.tag
    }

    /// The name of the enum variant being visited.
    pub fn variant_name(&self) -> &'l IdentStr {
        self.variant_name
    }

    /// The number of elements in this vector that have been visited so far.
    pub fn off(&self) -> u64 {
        self.off
    }

    /// The layout of the next field to be visited (if there is one), or `None` otherwise.
    pub fn peek_field(&self) -> Option<&'l MoveFieldLayout> {
        self.variant_layout.get(self.off as usize)
    }

    /// Visit the next field in the variant. The driver accepts a visitor to use for this field,
    /// allowing the visitor to be changed on recursive calls or even between fields in the same
    /// variant.
    ///
    /// Returns `Ok(None)` if there are no more fields in the variant, `Ok((f, v))` if there was an
    /// field and it was successfully visited (where `v` is the value returned by the visitor, and
    /// `f` is the layout of the field that was visited) or an error if there was an underlying
    /// deserialization error, or an error during visitation.
    pub fn next_field<V: Visitor<'b, 'l> + ?Sized>(
        &mut self,
        visitor: &mut V,
    ) -> Result<Option<(&'l MoveFieldLayout, V::Value)>, V::Error> {
        let Some(field) = self.peek_field() else {
            return Ok(None);
        };

        let res = visit_value(self.inner.bytes, &field.layout, visitor)?;
        self.off += 1;
        Ok(Some((field, res)))
    }

    /// Skip the next field. Returns the layout of the field that was visited if there was one, or
    /// `None` if there was none. Can return an error if there was a deserialization error.
    pub fn skip_field(&mut self) -> Result<Option<&'l MoveFieldLayout>, Error> {
        self.next_field(&mut NullTraversal)
            .map(|res| res.map(|(f, _)| f))
    }
}

/// Visit a serialized Move value with the provided `layout`, held in `bytes`, using the provided
/// visitor to build a value out of it. See `annoted_value::MoveValue::visit_deserialize` for
/// details.
pub(crate) fn visit_value<'c, 'b, 'l, V: Visitor<'b, 'l> + ?Sized>(
    bytes: &'c mut Cursor<&'b [u8]>,
    layout: &'l MoveTypeLayout,
    visitor: &mut V,
) -> Result<V::Value, V::Error> {
    use MoveTypeLayout as L;

    let mut driver = ValueDriver::new(bytes, Some(layout));
    match layout {
        L::Bool => match driver.read_exact()? {
            [0] => visitor.visit_bool(&driver, false),
            [1] => visitor.visit_bool(&driver, true),
            [b] => Err(Error::UnexpectedByte(b).into()),
        },

        L::U8 => {
            let v = u8::from_le_bytes(driver.read_exact()?);
            visitor.visit_u8(&driver, v)
        }

        L::U16 => {
            let v = u16::from_le_bytes(driver.read_exact()?);
            visitor.visit_u16(&driver, v)
        }

        L::U32 => {
            let v = u32::from_le_bytes(driver.read_exact()?);
            visitor.visit_u32(&driver, v)
        }

        L::U64 => {
            let v = u64::from_le_bytes(driver.read_exact()?);
            visitor.visit_u64(&driver, v)
        }

        L::U128 => {
            let v = u128::from_le_bytes(driver.read_exact()?);
            visitor.visit_u128(&driver, v)
        }

        L::U256 => {
            let v = U256::from_le_bytes(&driver.read_exact()?);
            visitor.visit_u256(&driver, v)
        }

        L::Address => {
            let v = AccountAddress::new(driver.read_exact()?);
            visitor.visit_address(&driver, v)
        }

        L::Signer => {
            let v = AccountAddress::new(driver.read_exact()?);
            visitor.visit_signer(&driver, v)
        }

        L::Vector(l) => visit_vector(driver, l.as_ref(), visitor),
        L::Struct(l) => visit_struct(driver, l, visitor),
        L::Enum(e) => visit_variant(driver, e, visitor),
    }
}

/// Like `visit_value` but specialized to visiting a vector (where the `bytes` is known to be a
/// serialized move vector), and the layout is the vector's element's layout.
fn visit_vector<'c, 'b, 'l, V: Visitor<'b, 'l> + ?Sized>(
    mut inner: ValueDriver<'c, 'b, 'l>,
    layout: &'l MoveTypeLayout,
    visitor: &mut V,
) -> Result<V::Value, V::Error> {
    let len = inner.read_leb128()?;
    let mut driver = VecDriver::new(inner, layout, len);
    let res = visitor.visit_vector(&mut driver)?;
    while driver.skip_element()? {}
    Ok(res)
}

/// Like `visit_value` but specialized to visiting a struct (where the `bytes` is known to be a
/// serialized move struct), and the layout is a struct layout.
pub(crate) fn visit_struct<'c, 'b, 'l, V: Visitor<'b, 'l> + ?Sized>(
    inner: ValueDriver<'c, 'b, 'l>,
    layout: &'l MoveStructLayout,
    visitor: &mut V,
) -> Result<V::Value, V::Error> {
    let mut driver = StructDriver::new(inner, layout);
    let res = visitor.visit_struct(&mut driver)?;
    while driver.skip_field()?.is_some() {}
    Ok(res)
}

/// Like `visit_struct` but specialized to visiting a variant (where the `bytes` is known to be a
/// serialized move variant), and the layout is an enum layout.
fn visit_variant<'c, 'b, 'l, V: Visitor<'b, 'l> + ?Sized>(
    mut inner: ValueDriver<'c, 'b, 'l>,
    layout: &'l MoveEnumLayout,
    visitor: &mut V,
) -> Result<V::Value, V::Error> {
    // Since variants are bounded at 127, we can read the tag as a single byte.
    // When we add true ULEB encoding for enum variants switch to this:
    // let tag = inner.read_leb128()?;
    let [tag] = inner.read_exact()?;
    if tag >= VARIANT_COUNT_MAX as u8 {
        return Err(Error::UnexpectedVariantTag(tag as usize).into());
    }
    let variant_layout = layout
        .variants
        .iter()
        .find(|((_, vtag), _)| *vtag == tag as u16)
        .ok_or(Error::UnexpectedVariantTag(tag as usize))?;

    let mut driver = VariantDriver::new(
        inner,
        layout,
        variant_layout.1,
        &variant_layout.0 .0,
        tag as u16,
    );
    let res = visitor.visit_variant(&mut driver)?;
    while driver.skip_field()?.is_some() {}
    Ok(res)
}
