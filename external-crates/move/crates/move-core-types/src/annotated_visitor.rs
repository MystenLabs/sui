// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::io::Read;

use crate::{
    account_address::AccountAddress,
    annotated_value::{MoveEnumLayout, MoveFieldLayout, MoveStructLayout, MoveTypeLayout},
    identifier::IdentStr,
    u256::U256,
    VARIANT_COUNT_MAX,
};

/// Visitors can be used for building values out of a serialized Move struct or value.
pub trait Visitor {
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

    fn visit_u8(&mut self, value: u8) -> Result<Self::Value, Self::Error>;
    fn visit_u16(&mut self, value: u16) -> Result<Self::Value, Self::Error>;
    fn visit_u32(&mut self, value: u32) -> Result<Self::Value, Self::Error>;
    fn visit_u64(&mut self, value: u64) -> Result<Self::Value, Self::Error>;
    fn visit_u128(&mut self, value: u128) -> Result<Self::Value, Self::Error>;
    fn visit_u256(&mut self, value: U256) -> Result<Self::Value, Self::Error>;
    fn visit_bool(&mut self, value: bool) -> Result<Self::Value, Self::Error>;
    fn visit_address(&mut self, value: AccountAddress) -> Result<Self::Value, Self::Error>;
    fn visit_signer(&mut self, value: AccountAddress) -> Result<Self::Value, Self::Error>;

    fn visit_vector(
        &mut self,
        driver: &mut VecDriver<'_, '_, '_>,
    ) -> Result<Self::Value, Self::Error>;

    fn visit_struct(
        &mut self,
        driver: &mut StructDriver<'_, '_, '_>,
    ) -> Result<Self::Value, Self::Error>;

    fn visit_variant(
        &mut self,
        driver: &mut VariantDriver<'_, '_, '_>,
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
pub trait Traversal {
    type Error: From<Error>;

    fn traverse_u8(&mut self, _value: u8) -> Result<(), Self::Error> {
        Ok(())
    }

    fn traverse_u16(&mut self, _value: u16) -> Result<(), Self::Error> {
        Ok(())
    }

    fn traverse_u32(&mut self, _value: u32) -> Result<(), Self::Error> {
        Ok(())
    }

    fn traverse_u64(&mut self, _value: u64) -> Result<(), Self::Error> {
        Ok(())
    }

    fn traverse_u128(&mut self, _value: u128) -> Result<(), Self::Error> {
        Ok(())
    }

    fn traverse_u256(&mut self, _value: U256) -> Result<(), Self::Error> {
        Ok(())
    }

    fn traverse_bool(&mut self, _value: bool) -> Result<(), Self::Error> {
        Ok(())
    }

    fn traverse_address(&mut self, _value: AccountAddress) -> Result<(), Self::Error> {
        Ok(())
    }

    fn traverse_signer(&mut self, _value: AccountAddress) -> Result<(), Self::Error> {
        Ok(())
    }

    fn traverse_vector(&mut self, driver: &mut VecDriver<'_, '_, '_>) -> Result<(), Self::Error> {
        while driver.next_element(self)?.is_some() {}
        Ok(())
    }

    fn traverse_struct(
        &mut self,
        driver: &mut StructDriver<'_, '_, '_>,
    ) -> Result<(), Self::Error> {
        while driver.next_field(self)?.is_some() {}
        Ok(())
    }

    fn traverse_variant(
        &mut self,
        driver: &mut VariantDriver<'_, '_, '_>,
    ) -> Result<(), Self::Error> {
        while driver.next_field(self)?.is_some() {}
        Ok(())
    }
}

/// Default implementation converting any traversal into a visitor.
impl<T: Traversal + ?Sized> Visitor for T {
    type Value = ();
    type Error = T::Error;

    fn visit_u8(&mut self, value: u8) -> Result<Self::Value, Self::Error> {
        self.traverse_u8(value)
    }

    fn visit_u16(&mut self, value: u16) -> Result<Self::Value, Self::Error> {
        self.traverse_u16(value)
    }

    fn visit_u32(&mut self, value: u32) -> Result<Self::Value, Self::Error> {
        self.traverse_u32(value)
    }

    fn visit_u64(&mut self, value: u64) -> Result<Self::Value, Self::Error> {
        self.traverse_u64(value)
    }

    fn visit_u128(&mut self, value: u128) -> Result<Self::Value, Self::Error> {
        self.traverse_u128(value)
    }

    fn visit_u256(&mut self, value: U256) -> Result<Self::Value, Self::Error> {
        self.traverse_u256(value)
    }

    fn visit_bool(&mut self, value: bool) -> Result<Self::Value, Self::Error> {
        self.traverse_bool(value)
    }

    fn visit_address(&mut self, value: AccountAddress) -> Result<Self::Value, Self::Error> {
        self.traverse_address(value)
    }

    fn visit_signer(&mut self, value: AccountAddress) -> Result<Self::Value, Self::Error> {
        self.traverse_signer(value)
    }

    fn visit_vector(
        &mut self,
        driver: &mut VecDriver<'_, '_, '_>,
    ) -> Result<Self::Value, Self::Error> {
        self.traverse_vector(driver)
    }

    fn visit_struct(
        &mut self,
        driver: &mut StructDriver<'_, '_, '_>,
    ) -> Result<Self::Value, Self::Error> {
        self.traverse_struct(driver)
    }

    fn visit_variant(
        &mut self,
        driver: &mut VariantDriver<'_, '_, '_>,
    ) -> Result<Self::Value, Self::Error> {
        self.traverse_variant(driver)
    }
}

/// Exposes information about a vector being visited (the element layout) to a visitor
/// implementation, and allows that visitor to progress the traversal (by visiting or skipping
/// elements).
pub struct VecDriver<'r, 'b, 'l> {
    bytes: &'r mut &'b [u8],
    layout: &'l MoveTypeLayout,
    len: u64,
    off: u64,
}

/// Exposes information about a struct being visited (its layout, details about the next field to be
/// visited) to a visitor implementation, and allows that visitor to progress the traversal (by
/// visiting or skipping fields).
pub struct StructDriver<'r, 'b, 'l> {
    bytes: &'r mut &'b [u8],
    layout: &'l MoveStructLayout,
    off: usize,
}

/// Exposes information about a variant being visited (its layout, details about the next field to
/// be visited, the variant's tag, and name) to a visitor implementation, and allows that visitor
/// to progress the traversal (by visiting or skipping fields).
pub struct VariantDriver<'r, 'b, 'l> {
    bytes: &'r mut &'b [u8],
    layout: &'l MoveEnumLayout,
    tag: u16,
    variant_name: &'l IdentStr,
    variant_layout: &'l [MoveFieldLayout],
    off: usize,
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
}

/// The null traversal implements `Traversal` and `Visitor` but without doing anything (does not
/// return a value, and does not modify any state). This is useful for skipping over parts of the
/// value structure.
pub struct NullTraversal;

impl Traversal for NullTraversal {
    type Error = Error;
}

#[allow(clippy::len_without_is_empty)]
impl<'r, 'b, 'l> VecDriver<'r, 'b, 'l> {
    fn new(bytes: &'r mut &'b [u8], layout: &'l MoveTypeLayout, len: u64) -> Self {
        Self {
            bytes,
            layout,
            len,
            off: 0,
        }
    }

    /// Type layout for the vector's inner type.
    pub fn element_layout(&self) -> &'l MoveTypeLayout {
        self.layout
    }

    /// The number of elements in this vector
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
    pub fn next_element<V: Visitor + ?Sized>(
        &mut self,
        visitor: &mut V,
    ) -> Result<Option<V::Value>, V::Error> {
        Ok(if self.off >= self.len {
            None
        } else {
            let res = visit_value(self.bytes, self.layout, visitor)?;
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

impl<'r, 'b, 'l> StructDriver<'r, 'b, 'l> {
    fn new(bytes: &'r mut &'b [u8], layout: &'l MoveStructLayout) -> Self {
        Self {
            bytes,
            layout,
            off: 0,
        }
    }

    /// The layout of the struct being visited.
    pub fn struct_layout(&self) -> &'l MoveStructLayout {
        self.layout
    }

    /// The layout of the next field to be visited (if there is one), or `None` otherwise.
    pub fn peek_field(&self) -> Option<&'l MoveFieldLayout> {
        self.layout.fields.get(self.off)
    }

    /// Visit the next field in the struct. The driver accepts a visitor to use for this field,
    /// allowing the visitor to be changed on recursive calls or even between fields in the same
    /// struct.
    ///
    /// Returns `Ok(None)` if there are no more fields in the struct, `Ok((f, v))` if there was an
    /// field and it was successfully visited (where `v` is the value returned by the visitor, and
    /// `f` is the layout of the field that was visited) or an error if there was an underlying
    /// deserialization error, or an error during visitation.
    pub fn next_field<V: Visitor + ?Sized>(
        &mut self,
        visitor: &mut V,
    ) -> Result<Option<(&'l MoveFieldLayout, V::Value)>, V::Error> {
        let Some(field) = self.peek_field() else {
            return Ok(None);
        };

        let res = visit_value(self.bytes, &field.layout, visitor)?;
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

impl<'r, 'b, 'l> VariantDriver<'r, 'b, 'l> {
    fn new(
        bytes: &'r mut &'b [u8],
        layout: &'l MoveEnumLayout,
        variant_layout: &'l [MoveFieldLayout],
        variant_name: &'l IdentStr,
        tag: u16,
    ) -> Self {
        Self {
            bytes,
            layout,
            tag,
            variant_name,
            variant_layout,
            off: 0,
        }
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

    /// The layout of the next field to be visited (if there is one), or `None` otherwise.
    pub fn peek_field(&self) -> Option<&'l MoveFieldLayout> {
        self.variant_layout.get(self.off)
    }

    /// Visit the next field in the variant. The driver accepts a visitor to use for this field,
    /// allowing the visitor to be changed on recursive calls or even between fields in the same
    /// variant.
    ///
    /// Returns `Ok(None)` if there are no more fields in the variant, `Ok((f, v))` if there was an
    /// field and it was successfully visited (where `v` is the value returned by the visitor, and
    /// `f` is the layout of the field that was visited) or an error if there was an underlying
    /// deserialization error, or an error during visitation.
    pub fn next_field<V: Visitor + ?Sized>(
        &mut self,
        visitor: &mut V,
    ) -> Result<Option<(&'l MoveFieldLayout, V::Value)>, V::Error> {
        let Some(field) = self.peek_field() else {
            return Ok(None);
        };

        let res = visit_value(self.bytes, &field.layout, visitor)?;
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
pub(crate) fn visit_value<V: Visitor + ?Sized>(
    bytes: &mut &[u8],
    layout: &MoveTypeLayout,
    visitor: &mut V,
) -> Result<V::Value, V::Error> {
    use MoveTypeLayout as L;

    match layout {
        L::Bool => match read_exact::<1>(bytes)? {
            [0] => visitor.visit_bool(false),
            [1] => visitor.visit_bool(true),
            [b] => Err(Error::UnexpectedByte(b).into()),
        },

        L::U8 => visitor.visit_u8(u8::from_le_bytes(read_exact::<1>(bytes)?)),
        L::U16 => visitor.visit_u16(u16::from_le_bytes(read_exact::<2>(bytes)?)),
        L::U32 => visitor.visit_u32(u32::from_le_bytes(read_exact::<4>(bytes)?)),
        L::U64 => visitor.visit_u64(u64::from_le_bytes(read_exact::<8>(bytes)?)),
        L::U128 => visitor.visit_u128(u128::from_le_bytes(read_exact::<16>(bytes)?)),
        L::U256 => visitor.visit_u256(U256::from_le_bytes(&read_exact::<32>(bytes)?)),
        L::Address => visitor.visit_address(AccountAddress::new(read_exact::<32>(bytes)?)),
        L::Signer => visitor.visit_signer(AccountAddress::new(read_exact::<32>(bytes)?)),

        L::Vector(l) => {
            let len = leb128::read::unsigned(bytes).map_err(|_| Error::UnexpectedEof)?;
            let mut driver = VecDriver::new(bytes, l.as_ref(), len);
            let res = visitor.visit_vector(&mut driver)?;
            while driver.skip_element()? {}
            Ok(res)
        }
        L::Enum(e) => visit_variant(bytes, e, visitor),
        L::Struct(l) => visit_struct(bytes, l, visitor),
    }
}

/// Like `visit_value` but specialized to visiting a struct (where the `bytes` is known to be a
/// serialized move struct), and the layout is a struct layout.
pub(crate) fn visit_struct<V: Visitor + ?Sized>(
    bytes: &mut &[u8],
    layout: &MoveStructLayout,
    visitor: &mut V,
) -> Result<V::Value, V::Error> {
    let mut driver = StructDriver::new(bytes, layout);
    let res = visitor.visit_struct(&mut driver)?;
    while driver.skip_field()?.is_some() {}
    Ok(res)
}

/// Like `visit_struct` but specialized to visiting a variant (where the `bytes` is known to be a
/// serialized move variant), and the layout is an enum layout.
pub(crate) fn visit_variant<V: Visitor + ?Sized>(
    bytes: &mut &[u8],
    layout: &MoveEnumLayout,
    visitor: &mut V,
) -> Result<V::Value, V::Error> {
    // Since variants are bounded at 127, we can read the tag as a single byte.
    // When we add true ULEB encoding for enum variants switch to this:
    // let tag = leb128::read::unsigned(bytes).map_err(|_| Error::UnexpectedEof)?;
    let [tag] = read_exact::<1>(bytes)?;
    if tag >= VARIANT_COUNT_MAX as u8 {
        return Err(Error::UnexpectedVariantTag(tag as usize).into());
    }
    let variant_layout = layout
        .variants
        .iter()
        .find(|((_, vtag), _)| *vtag == tag as u16)
        .ok_or(Error::UnexpectedVariantTag(tag as usize))?;

    let mut driver = VariantDriver::new(
        bytes,
        layout,
        variant_layout.1,
        &variant_layout.0 .0,
        tag as u16,
    );
    let res = visitor.visit_variant(&mut driver)?;
    while driver.skip_field()?.is_some() {}
    Ok(res)
}

fn read_exact<const N: usize>(bytes: &mut &[u8]) -> Result<[u8; N], Error> {
    let mut buf = [0u8; N];
    bytes
        .read_exact(&mut buf)
        .map_err(|_| Error::UnexpectedEof)?;
    Ok(buf)
}
