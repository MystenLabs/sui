// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::annotated_value as A;
use prost_types::Struct;
use prost_types::Value;
use prost_types::value::Kind;

use crate::object::rpc_visitor as RV;

/// This is the maximum depth of a proto message.
/// The maximum depth of a proto message is 100. Given this value may be nested itself somewhere,
/// we'll conservatively cap this to ~80% of that.
const MAX_DEPTH: usize = 80;

pub struct ProtoVisitor {
    /// Budget to spend on visiting.
    bound: usize,
}

impl ProtoVisitor {
    pub fn new(bound: usize) -> Self {
        Self { bound }
    }

    /// Deserialize `bytes` as a `MoveValue` with layout `layout`.
    pub fn deserialize_value(
        mut self,
        bytes: &[u8],
        layout: &A::MoveTypeLayout,
    ) -> Result<Value, RV::Error> {
        A::MoveValue::visit_deserialize(
            bytes,
            layout,
            &mut RV::RpcVisitor::<Value, _>::new(RV::LocalMeter::new(&mut self.bound, MAX_DEPTH)),
        )
    }
}

impl RV::Format for Value {
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

    fn null<M: RV::Meter>(meter: &mut M) -> Result<Self, RV::MeterError> {
        meter.charge(std::mem::size_of::<Value>())?;
        Ok(Kind::NullValue(0).into())
    }

    fn bool<M: RV::Meter>(meter: &mut M, value: bool) -> Result<Self, RV::MeterError> {
        meter.charge(std::mem::size_of::<Value>())?;
        Ok(Self::from(value))
    }

    fn number<M: RV::Meter>(meter: &mut M, value: u32) -> Result<Self, RV::MeterError> {
        meter.charge(std::mem::size_of::<Value>())?;
        Ok(Self::from(value))
    }

    fn string<M: RV::Meter>(meter: &mut M, value: String) -> Result<Self, RV::MeterError> {
        meter.charge(std::mem::size_of::<Value>() + value.len())?;
        Ok(Self::from(value))
    }

    fn vec<M: RV::Meter>(meter: &mut M, value: Self::Vec) -> Result<Self, RV::MeterError> {
        meter.charge(std::mem::size_of::<Value>())?;
        Ok(Self::from(value))
    }

    fn map<M: RV::Meter>(meter: &mut M, value: Self::Map) -> Result<Self, RV::MeterError> {
        meter.charge(std::mem::size_of::<Value>())?;
        Ok(Self::from(Kind::StructValue(value)))
    }

    fn vec_push_element<M: RV::Meter>(
        _meter: &mut M,
        vec: &mut Self::Vec,
        value: Self,
    ) -> Result<(), RV::MeterError> {
        vec.push(value);
        Ok(())
    }

    fn map_push_field<M: RV::Meter>(
        meter: &mut M,
        map: &mut Self::Map,
        key: String,
        value: Self,
    ) -> Result<(), RV::MeterError> {
        meter.charge(key.len())?;
        map.fields.insert(key, value);
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::object::rpc_visitor::proto::*;

    use crate::object::bounded_visitor::tests::layout_;
    use crate::object::bounded_visitor::tests::serialize;
    use crate::object::bounded_visitor::tests::value_;
    use expect_test::expect;
    use serde_json::json;

    use A::MoveTypeLayout as L;
    use A::MoveValue as V;

    #[test]
    fn test_simple() {
        let type_layout = layout_(
            "0x0::foo::Bar",
            vec![
                ("a", L::U64),
                ("b", L::Vector(Box::new(L::U64))),
                ("c", layout_("0x0::foo::Baz", vec![("d", L::U64)])),
            ],
        );

        let value = value_(
            "0x0::foo::Bar",
            vec![
                ("a", V::U64(42)),
                ("b", V::Vector(vec![V::U64(43)])),
                ("c", value_("0x0::foo::Baz", vec![("d", V::U64(44))])),
            ],
        );

        let expected = json!({
            "a": "42",
            "b": ["43"],
            "c": {
                "d": "44"
            }
        });
        let bound = required_budget(&expected);

        let bytes = serialize(value.clone());

        let deser = ProtoVisitor::new(bound)
            .deserialize_value(&bytes, &type_layout)
            .unwrap();

        assert_eq!(expected, proto_value_to_json_value(deser));

        ProtoVisitor::new(bound - 1)
            .deserialize_value(&bytes, &type_layout)
            .unwrap_err();
    }

    #[test]
    fn test_too_deep() {
        let mut layout = L::U64;
        let mut value = V::U64(42);
        let mut expected = serde_json::Value::from("42");

        const DEPTH: usize = MAX_DEPTH;
        for _ in 0..DEPTH {
            layout = layout_("0x0::foo::Bar", vec![("f", layout)]);
            value = value_("0x0::foo::Bar", vec![("f", value)]);
            expected = json!({
                "f": expected
            });
        }

        let bound = required_budget(&expected);
        let bytes = serialize(value.clone());

        let deser = ProtoVisitor::new(bound)
            .deserialize_value(&bytes, &layout)
            .unwrap();

        assert_eq!(expected, proto_value_to_json_value(deser));

        layout = layout_("0x0::foo::Bar", vec![("f", layout)]);
        value = value_("0x0::foo::Bar", vec![("f", value)]);

        let bytes = serialize(value.clone());

        let err = ProtoVisitor::new(bound)
            .deserialize_value(&bytes, &layout)
            .unwrap_err();

        let expect = expect!["Exceeded maximum depth"];
        expect.assert_eq(&err.to_string());
    }

    fn proto_value_to_json_value(proto: Value) -> serde_json::Value {
        match proto.kind {
            Some(Kind::NullValue(_)) | None => serde_json::Value::Null,
            // Move doesn't support floats, so these tests can safely convert numbers back to u32.
            Some(Kind::NumberValue(n)) => serde_json::Value::from(n as u32),
            Some(Kind::StringValue(s)) => serde_json::Value::from(s),
            Some(Kind::BoolValue(b)) => serde_json::Value::from(b),
            Some(Kind::StructValue(map)) => serde_json::Value::Object(
                map.fields
                    .into_iter()
                    .map(|(k, v)| (k, proto_value_to_json_value(v)))
                    .collect(),
            ),
            Some(Kind::ListValue(list_value)) => serde_json::Value::Array(
                list_value
                    .values
                    .into_iter()
                    .map(proto_value_to_json_value)
                    .collect(),
            ),
        }
    }

    fn required_budget(json: &serde_json::Value) -> usize {
        size_of::<Value>()
            + match json {
                serde_json::Value::Null => 0,
                serde_json::Value::Bool(_) => 0,
                serde_json::Value::Number(_) => 0,
                serde_json::Value::String(s) => s.len(),
                serde_json::Value::Array(vec) => vec.iter().map(required_budget).sum(),
                serde_json::Value::Object(map) => {
                    map.iter().map(|(k, v)| k.len() + required_budget(v)).sum()
                }
            }
    }
}
