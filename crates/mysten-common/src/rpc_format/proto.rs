// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prost_types::Struct;
use prost_types::Value;
use prost_types::value::Kind;

use crate::rpc_format::Format;
use crate::rpc_format::Meter;
use crate::rpc_format::MeterError;

impl Format for Value {
    type Vec = Vec<Value>;
    type Map = Struct;

    fn is_null(&self) -> bool {
        matches!(self.kind, Some(Kind::NullValue(_)))
    }

    fn is_bool(&self) -> bool {
        self.as_bool().is_some()
    }

    fn is_number(&self) -> bool {
        matches!(self.kind, Some(Kind::NumberValue(_)))
    }

    fn is_string(&self) -> bool {
        self.as_string().is_some()
    }

    fn is_array(&self) -> bool {
        self.as_array().is_some()
    }

    fn is_object(&self) -> bool {
        self.as_object().is_some()
    }

    fn as_bool(&self) -> Option<bool> {
        if let Some(Kind::BoolValue(b)) = &self.kind {
            Some(*b)
        } else {
            None
        }
    }

    fn as_string(&self) -> Option<&str> {
        if let Some(Kind::StringValue(value)) = &self.kind {
            Some(value)
        } else {
            None
        }
    }

    fn as_array(&self) -> Option<&Self::Vec> {
        if let Some(Kind::ListValue(value)) = &self.kind {
            Some(&value.values)
        } else {
            None
        }
    }

    fn as_object(&self) -> Option<&Self::Map> {
        if let Some(Kind::StructValue(value)) = &self.kind {
            Some(value)
        } else {
            None
        }
    }

    fn null<M: Meter>(meter: &mut M) -> Result<Self, MeterError> {
        meter.charge(std::mem::size_of::<Value>())?;
        Ok(Kind::NullValue(0).into())
    }

    fn bool<M: Meter>(meter: &mut M, value: bool) -> Result<Self, MeterError> {
        meter.charge(std::mem::size_of::<Value>())?;
        Ok(Self::from(value))
    }

    fn number<M: Meter>(meter: &mut M, value: u32) -> Result<Self, MeterError> {
        meter.charge(std::mem::size_of::<Value>())?;
        Ok(Self::from(value))
    }

    fn string<M: Meter>(meter: &mut M, value: String) -> Result<Self, MeterError> {
        meter.charge(std::mem::size_of::<Value>() + value.len())?;
        Ok(Self::from(value))
    }

    fn vec<M: Meter>(meter: &mut M, value: Self::Vec) -> Result<Self, MeterError> {
        meter.charge(std::mem::size_of::<Value>())?;
        Ok(Self::from(value))
    }

    fn map<M: Meter>(meter: &mut M, value: Self::Map) -> Result<Self, MeterError> {
        meter.charge(std::mem::size_of::<Value>())?;
        Ok(Self::from(Kind::StructValue(value)))
    }

    fn vec_push_element<M: Meter>(
        _meter: &mut M,
        vec: &mut Self::Vec,
        value: Self,
    ) -> Result<(), MeterError> {
        vec.push(value);
        Ok(())
    }

    fn map_push_field<M: Meter>(
        meter: &mut M,
        map: &mut Self::Map,
        key: String,
        value: Self,
    ) -> Result<(), MeterError> {
        meter.charge(key.len())?;
        map.fields.insert(key, value);
        Ok(())
    }
}
