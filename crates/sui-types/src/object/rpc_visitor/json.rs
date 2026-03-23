// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde_json::Map;
use serde_json::Value as Json;

use crate::object::rpc_visitor::Format;
use crate::object::rpc_visitor::Meter;
use crate::object::rpc_visitor::MeterError;

impl Format for Json {
    type Vec = Vec<Self>;
    type Map = Map<String, Self>;

    fn is_null(&self) -> bool {
        self.is_null()
    }

    fn is_bool(&self) -> bool {
        self.is_boolean()
    }

    fn is_number(&self) -> bool {
        self.is_number()
    }

    fn is_string(&self) -> bool {
        self.is_string()
    }

    fn is_array(&self) -> bool {
        self.is_array()
    }

    fn is_object(&self) -> bool {
        self.is_object()
    }

    fn as_bool(&self) -> Option<bool> {
        self.as_bool()
    }

    fn as_string(&self) -> Option<&str> {
        self.as_str()
    }

    fn as_array(&self) -> Option<&Self::Vec> {
        self.as_array()
    }

    fn as_object(&self) -> Option<&Self::Map> {
        self.as_object()
    }

    fn null<M: Meter>(meter: &mut M) -> Result<Self, MeterError> {
        meter.charge("null".len())?;
        Ok(Json::Null)
    }

    fn bool<M: Meter>(meter: &mut M, value: bool) -> Result<Self, MeterError> {
        meter.charge(if value { "true".len() } else { "false".len() })?;
        Ok(Json::Bool(value))
    }

    fn number<M: Meter>(meter: &mut M, value: u32) -> Result<Self, MeterError> {
        meter.charge(if value == 0 { 1 } else { value.ilog10() + 1 } as usize)?;
        Ok(Json::Number(value.into()))
    }

    fn string<M: Meter>(meter: &mut M, value: String) -> Result<Self, MeterError> {
        // Account for the quotes around the string
        meter.charge(2 + value.len())?;
        Ok(Json::String(value))
    }

    fn vec<M: Meter>(meter: &mut M, value: Self::Vec) -> Result<Self, MeterError> {
        // Account for the opening bracket
        meter.charge(1)?;
        Ok(Json::Array(value))
    }

    fn map<M: Meter>(meter: &mut M, value: Self::Map) -> Result<Self, MeterError> {
        // Account for the opening brace
        meter.charge(1)?;
        Ok(Json::Object(value))
    }

    fn vec_push_element<M: Meter>(
        meter: &mut M,
        vec: &mut Self::Vec,
        value: Self,
    ) -> Result<(), MeterError> {
        // Account for the comma (or closing bracket)
        meter.charge(1)?;
        vec.push(value);
        Ok(())
    }

    fn map_push_field<M: Meter>(
        meter: &mut M,
        map: &mut Self::Map,
        key: String,
        value: Self,
    ) -> Result<(), MeterError> {
        // Account for quotes, colon, and comma (or closing brace)
        meter.charge(4 + key.len())?;
        map.insert(key, value);
        Ok(())
    }
}
