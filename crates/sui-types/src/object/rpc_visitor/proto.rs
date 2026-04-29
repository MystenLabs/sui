// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::annotated_value as A;
use move_core_types::compressed::annotated as CA;
use prost_types::Value;

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
        layout: CA::MoveTypeLayout,
    ) -> Result<Value, RV::Error> {
        A::MoveValue::visit_deserialize(
            bytes,
            layout.as_ref(),
            &mut RV::RpcVisitor::<Value, _>::new(RV::LocalMeter::new(&mut self.bound, MAX_DEPTH)),
        )
    }

    /// Like `deserialize_value`, but draws from an externally-owned size
    /// budget so that callers rendering many values into one response can
    /// share a single aggregate cap. The budget is decremented in place; if
    /// it is exhausted, deserialization fails with `MeterError::TooBig`.
    pub fn deserialize_value_with_budget(
        bytes: &[u8],
        layout: &A::MoveTypeLayout,
        size_budget: &mut usize,
    ) -> Result<Value, RV::Error> {
        A::MoveValue::visit_deserialize(
            bytes,
            layout.as_ref(),
            &mut RV::RpcVisitor::<Value, _>::new(RV::LocalMeter::new(size_budget, MAX_DEPTH)),
        )
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use prost_types::value::Kind;

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
        let type_layout: CA::MoveTypeLayout = CA::MoveTypeLayout::try_from(&layout_(
            "0x0::foo::Bar",
            vec![
                ("a", L::U64),
                ("b", L::Vector(Box::new(L::U64))),
                ("c", layout_("0x0::foo::Baz", vec![("d", L::U64)])),
            ],
        ))
        .unwrap();

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
            .deserialize_value(&bytes, type_layout.clone())
            .unwrap();

        assert_eq!(expected, proto_value_to_json_value(deser));

        ProtoVisitor::new(bound - 1)
            .deserialize_value(&bytes, type_layout)
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
            .deserialize_value(&bytes, CA::MoveTypeLayout::try_from(&layout).unwrap())
            .unwrap();

        assert_eq!(expected, proto_value_to_json_value(deser));

        layout = layout_("0x0::foo::Bar", vec![("f", layout)]);
        value = value_("0x0::foo::Bar", vec![("f", value)]);

        let bytes = serialize(value.clone());

        let err = ProtoVisitor::new(bound)
            .deserialize_value(&bytes, CA::MoveTypeLayout::try_from(&layout).unwrap())
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
