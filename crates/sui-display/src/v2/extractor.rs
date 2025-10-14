// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use move_core_types::annotated_value as A;
use move_core_types::annotated_visitor as AV;
use move_core_types::u256::U256;

use super::address_visitor::AddressVisitor;
use super::error::FormatError;
use super::value::{Accessor, Slice, Value};

/// A visitor that follows a path of accessors, to slice out the BCS and layout for a sub-part of
/// the value.
pub(crate) struct Extractor<'v, 'p> {
    path: &'p mut Vec<Accessor<'v>>,
}

impl<'v, 'p> Extractor<'v, 'p> {
    /// Attempt to extract a value from the given slice, following the provided path of accessors.
    ///
    /// Accessors are expected to be in reverse order, i.e. the last accessor in the vector is
    /// applied first and are consumed as they are successfully applied.
    pub(crate) fn deserialize_slice(
        slice: Slice<'v>,
        path: &'p mut Vec<Accessor<'v>>,
    ) -> Result<Option<Value<'v>>, FormatError> {
        A::MoveValue::visit_deserialize(slice.bytes, slice.layout, &mut Self { path })
    }
}

impl<'v> AV::Visitor<'v, 'v> for Extractor<'v, '_> {
    type Value = Option<Value<'v>>;
    type Error = FormatError;

    fn visit_u8(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        n: u8,
    ) -> Result<Self::Value, Self::Error> {
        Ok(self.path.is_empty().then_some(Value::U8(n)))
    }

    fn visit_u16(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        n: u16,
    ) -> Result<Self::Value, Self::Error> {
        Ok(self.path.is_empty().then_some(Value::U16(n)))
    }

    fn visit_u32(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        n: u32,
    ) -> Result<Self::Value, Self::Error> {
        Ok(self.path.is_empty().then_some(Value::U32(n)))
    }

    fn visit_u64(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        n: u64,
    ) -> Result<Self::Value, Self::Error> {
        Ok(self.path.is_empty().then_some(Value::U64(n)))
    }

    fn visit_u128(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        n: u128,
    ) -> Result<Self::Value, Self::Error> {
        Ok(self.path.is_empty().then_some(Value::U128(n)))
    }

    fn visit_u256(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        n: U256,
    ) -> Result<Self::Value, Self::Error> {
        Ok(self.path.is_empty().then_some(Value::U256(n)))
    }

    fn visit_bool(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        b: bool,
    ) -> Result<Self::Value, Self::Error> {
        Ok(self.path.is_empty().then_some(Value::Bool(b)))
    }

    fn visit_address(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        a: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        match self.path.last() {
            Some(Accessor::DFIndex(_) | Accessor::DOFIndex(_)) => Ok(Some(Value::Address(a))),
            Some(_) => Ok(None),
            None => Ok(Some(Value::Address(a))),
        }
    }

    /// Sui does not produce signer values, so we can never extract them.
    fn visit_signer(
        &mut self,
        _: &AV::ValueDriver<'_, 'v, 'v>,
        _: AccountAddress,
    ) -> Result<Self::Value, Self::Error> {
        Ok(None)
    }

    fn visit_vector(
        &mut self,
        driver: &mut AV::VecDriver<'_, 'v, 'v>,
    ) -> Result<Self::Value, Self::Error> {
        let Some(accessor) = self.path.pop() else {
            while driver.skip_element()? {}
            return Ok(Some(Value::Slice(Slice {
                layout: driver.layout()?,
                bytes: &driver.bytes()[driver.start()..driver.position()],
            })));
        };

        let Some(i) = accessor.as_numeric_index() else {
            return Ok(None);
        };

        while driver.off() < i && driver.skip_element()? {}
        Ok(driver.next_element(self)?.flatten())
    }

    fn visit_struct(
        &mut self,
        driver: &mut AV::StructDriver<'_, 'v, 'v>,
    ) -> Result<Self::Value, Self::Error> {
        let Some(accessor) = self.path.last() else {
            while driver.skip_field()?.is_some() {}
            return Ok(Some(Value::Slice(Slice {
                layout: driver.layout()?,
                bytes: &driver.bytes()[driver.start()..driver.position()],
            })));
        };

        if matches!(accessor, Accessor::DFIndex(_) | Accessor::DOFIndex(_)) {
            return AddressVisitor
                .visit_struct(driver)
                .map(|a| a.map(Value::Address));
        }

        // TODO(amnn): Support vec map access

        let Some(name) = accessor.as_field_name() else {
            return Ok(None);
        };

        self.path.pop();
        while let Some(field) = driver.peek_field() {
            if field.name.as_str() == name.as_ref() {
                return Ok(driver.next_field(self)?.and_then(|(_, v)| v));
            } else {
                driver.skip_field()?;
            }
        }

        Ok(None)
    }

    fn visit_variant(
        &mut self,
        driver: &mut AV::VariantDriver<'_, 'v, 'v>,
    ) -> Result<Self::Value, Self::Error> {
        let Some(accessor) = self.path.pop() else {
            while driver.skip_field()?.is_some() {}
            return Ok(Some(Value::Slice(Slice {
                layout: driver.layout()?,
                bytes: &driver.bytes()[driver.start()..driver.position()],
            })));
        };

        let Some(name) = accessor.as_field_name() else {
            return Ok(None);
        };

        while let Some(field) = driver.peek_field() {
            if field.name.as_str() == name.as_ref() {
                return Ok(driver.next_field(self)?.and_then(|(_, v)| v));
            } else {
                driver.skip_field()?;
            }
        }

        Ok(None)
    }
}
