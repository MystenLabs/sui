// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    account_address::AccountAddress, annotated_value as A, annotated_visitor as AV,
    language_storage::TypeTag,
};

/// Elements are components of paths that select values from the sub-structure of other values.
/// They are split into two categories:
///
/// - Selectors, which recurse into the sub-structure.
/// - Filters, which check properties of the value at that position in the sub-structure.
#[derive(Debug, Clone)]
pub enum Element<'e> {
    // Selectors
    /// Select a named field, assuming the value in question is a struct or an enum variant.
    Field(&'e str),

    /// Select a positional element. This can be the element of a vector, or it can be a positional
    /// field in an enum or a struct.
    Index(u64),

    // Filters
    /// Confirm that the current value has a certain type.
    Type(&'e TypeTag),

    /// Confirm that the current value is an enum and its variant has this name. Note that to
    /// filter on both the enum type and the variant name, the path must contain the Type first,
    /// and then the Variant. Otherwise the type filter will be assumed
    Variant(&'e str),
}

/// An Extractor is an [`AV::Visitor`] that deserializes a sub-structure of the value. The
/// sub-structure is found at the end of a path of [`Element`]s which select fields from structs,
/// indices from vectors, and variants from enums. Deserialization is delegated to another visitor,
/// of type `V`, with the Extractor returning `Option<V::Value>`:
///
/// - `Some(v)` if the given path exists in the value, or
/// - `None` if the path did not exist,
/// - Or an error if the underlying visitor failed for some reason.
///
/// At every stage, the path can optionally start with an [`Element::Type`], which restricts the
/// type of the top-level value being deserialized. From there, the elements expected are driven by
/// the layout being deserialized:
///
/// - When deserializing a vector, the next element must be an [`Element::Index`] which selects the
///   offset into the vector that the extractor recurses into.
/// - When deserializing a struct, the next element may be an [`Element::Field`] which selects the
///   field of the struct that the extractor recurses into by name, or an [`Element::Index`] which
///   selects the field by its offset.
/// - When deserializing a variant, the next elements may optionally be an [`Element::Variant`]
///   which expects a particular variant of the enum, followed by either an [`Element::Field`] or
///   an [`Element::Index`], similar to a struct.
pub struct Extractor<'p, 'v, V> {
    inner: &'v mut V,
    path: &'p [Element<'p>],
}

impl<'p, 'v, 'b, 'l, V: AV::Visitor<'b, 'l>> Extractor<'p, 'v, V>
where
    V::Error: std::error::Error + Send + Sync + 'static,
{
    pub fn new(inner: &'v mut V, path: &'p [Element<'p>]) -> Self {
        Self { inner, path }
    }

    pub fn deserialize_value(
        bytes: &'b [u8],
        layout: &'l A::MoveTypeLayout,
        inner: &'v mut V,
        path: Vec<Element<'p>>,
    ) -> anyhow::Result<Option<V::Value>> {
        let mut extractor = Extractor::new(inner, &path);
        A::MoveValue::visit_deserialize(bytes, layout, &mut extractor)
    }

    pub fn deserialize_struct(
        bytes: &'b [u8],
        layout: &'l A::MoveStructLayout,
        inner: &'v mut V,
        path: Vec<Element<'p>>,
    ) -> anyhow::Result<Option<V::Value>> {
        let mut extractor = Extractor::new(inner, &path);
        A::MoveStruct::visit_deserialize(bytes, layout, &mut extractor)
    }
}

impl<'p, 'v, 'b, 'l, V: AV::Visitor<'b, 'l>> AV::Visitor<'b, 'l> for Extractor<'p, 'v, V> {
    type Value = Option<V::Value>;
    type Error = V::Error;

    fn visit_u8(
        &mut self,
        driver: &AV::ValueDriver<'_, 'b, 'l>,
        value: u8,
    ) -> Result<Self::Value, Self::Error> {
        Ok(match self.path {
            [] | [Element::Type(&TypeTag::U8)] => Some(self.inner.visit_u8(driver, value)?),
            _ => None,
        })
    }

    fn visit_u16(
        &mut self,
        driver: &AV::ValueDriver<'_, 'b, 'l>,
        value: u16,
    ) -> Result<Self::Value, Self::Error> {
        Ok(match self.path {
            [] | [Element::Type(&TypeTag::U16)] => Some(self.inner.visit_u16(driver, value)?),
            _ => None,
        })
    }

    fn visit_u32(
        &mut self,
        driver: &AV::ValueDriver<'_, 'b, 'l>,
        value: u32,
    ) -> Result<Self::Value, Self::Error> {
        Ok(match self.path {
            [] | [Element::Type(&TypeTag::U32)] => Some(self.inner.visit_u32(driver, value)?),
            _ => None,
        })
    }

    fn visit_u64(
        &mut self,
        driver: &AV::ValueDriver<'_, 'b, 'l>,
        value: u64,
    ) -> Result<Self::Value, Self::Error> {
        Ok(match self.path {
            [] | [Element::Type(&TypeTag::U64)] => Some(self.inner.visit_u64(driver, value)?),
            _ => None,
        })
    }

    fn visit_u128(
        &mut self,
        driver: &AV::ValueDriver<'_, 'b, 'l>,
        value: u128,
    ) -> Result<Self::Value, Self::Error> {
        Ok(match self.path {
            [] | [Element::Type(&TypeTag::U128)] => Some(self.inner.visit_u128(driver, value)?),
            _ => None,
        })
    }

    fn visit_u256(
        &mut self,
        driver: &AV::ValueDriver<'_, 'b, 'l>,
        value: crate::u256::U256,
    ) -> Result<Self::Value, Self::Error> {
        Ok(match self.path {
            [] | [Element::Type(&TypeTag::U256)] => Some(self.inner.visit_u256(driver, value)?),
            _ => None,
        })
    }

    fn visit_bool(
        &mut self,
        driver: &AV::ValueDriver<'_, 'b, 'l>,
        value: bool,
    ) -> Result<Self::Value, Self::Error> {
        Ok(match self.path {
            [] | [Element::Type(&TypeTag::Bool)] => Some(self.inner.visit_bool(driver, value)?),
            _ => None,
        })
    }

    fn visit_address(
        &mut self,
        driver: &AV::ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        Ok(match self.path {
            [] | [Element::Type(&TypeTag::Address)] => {
                Some(self.inner.visit_address(driver, value)?)
            }
            _ => None,
        })
    }

    fn visit_signer(
        &mut self,
        driver: &AV::ValueDriver<'_, 'b, 'l>,
        value: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        Ok(match self.path {
            [] | [Element::Type(&TypeTag::Signer)] => Some(self.inner.visit_signer(driver, value)?),
            _ => None,
        })
    }

    fn visit_vector(
        &mut self,
        driver: &mut AV::VecDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        use Element as E;
        use TypeTag as T;

        // If there is a type element, check that it is a vector type with the correct element
        // type, and remove it from the path.
        let path = if let [E::Type(t), path @ ..] = self.path {
            if !matches!(t, T::Vector(t) if driver.element_layout().is_type(t)) {
                return Ok(None);
            }
            path
        } else {
            self.path
        };

        // If there are no further path elements, we can delegate to the inner visitor.
        let [index, path @ ..] = path else {
            return Ok(Some(self.inner.visit_vector(driver)?));
        };

        // Visiting a vector, the next part of the path must be an index -- anything else is
        // guaranteed to fail.
        let E::Index(i) = index else {
            return Ok(None);
        };

        // Skip all the elements before the index, and then recurse.
        while driver.off() < *i && driver.skip_element()? {}
        Ok(driver
            .next_element(&mut Extractor {
                inner: self.inner,
                path,
            })?
            .flatten())
    }

    fn visit_struct(
        &mut self,
        driver: &mut AV::StructDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        use Element as E;
        use TypeTag as T;

        // If there is a type element, check that it is a struct type with the correct struct tag,
        // and remove it from the path.
        let path = if let [E::Type(t), path @ ..] = self.path {
            if !matches!(t, T::Struct(t) if driver.struct_layout().is_type(t)) {
                return Ok(None);
            }
            path
        } else {
            self.path
        };

        // If there are no further path elements, we can delegate to the inner visitor.
        let [field, path @ ..] = path else {
            return Ok(Some(self.inner.visit_struct(driver)?));
        };

        match field {
            // Skip over mismatched fields by name.
            E::Field(f) => {
                while matches!(driver.peek_field(), Some(l) if l.name.as_str() != *f) {
                    driver.skip_field()?;
                }
            }

            // Skip over fields by offset.
            E::Index(i) => while driver.off() < *i && driver.skip_field()?.is_some() {},

            // Any other element is invalid in this position.
            _ => return Ok(None),
        }

        Ok(driver
            .next_field(&mut Extractor {
                inner: self.inner,
                path,
            })?
            .and_then(|(_, v)| v))
    }

    fn visit_variant(
        &mut self,
        driver: &mut AV::VariantDriver<'_, 'b, 'l>,
    ) -> Result<Self::Value, Self::Error> {
        use Element as E;
        use TypeTag as T;

        // If there is a type element, check that it is a struct type with the correct struct tag,
        // and remove it from the path.
        let path = if let [E::Type(t), path @ ..] = self.path {
            if !matches!(t, T::Struct(t) if driver.enum_layout().is_type(t)) {
                return Ok(None);
            }
            path
        } else {
            self.path
        };

        // If there is a variant element, check that it matches and remove it from the path.
        let path = if let [E::Variant(v), path @ ..] = path {
            if driver.variant_name().as_str() != *v {
                return Ok(None);
            }
            path
        } else {
            path
        };

        // If there are no further path elements, we can delegate to the inner visitor.
        let [field, path @ ..] = path else {
            return Ok(Some(self.inner.visit_variant(driver)?));
        };

        match field {
            // Skip over mismatched fields by name.
            E::Field(f) => {
                while matches!(driver.peek_field(), Some(l) if l.name.as_str() != *f) {
                    driver.skip_field()?;
                }
            }

            // Skip over fields by offset.
            E::Index(i) => while driver.off() < *i && driver.skip_field()?.is_some() {},

            // Any other element is invalid in this position.
            _ => return Ok(None),
        }

        Ok(driver
            .next_field(&mut Extractor {
                inner: self.inner,
                path,
            })?
            .and_then(|(_, v)| v))
    }
}
