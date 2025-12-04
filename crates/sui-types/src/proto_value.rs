// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::annotated_value as A;
use move_core_types::annotated_visitor as AV;
use prost_types::Struct;
use prost_types::Value;
use prost_types::value::Kind;

use crate::object::option_visitor as OV;
use crate::rpc_visitor::Error as RpcVisitorError;
use crate::rpc_visitor::RpcVisitor;
use crate::rpc_visitor::Writer;

/// This is the maximum depth of a proto message
/// The maximum depth of a proto message is 100. Given this value may be nested itself somewhere
/// we'll conservitively cap this to ~80% of that.
const MAX_DEPTH: usize = 80;

pub struct ProtoVisitorBuilder {
    /// Budget to spend on visiting.
    bound: usize,
}

pub struct ProtoWriter<'b> {
    bound: &'b mut usize,
    depth: usize,
}

pub type ProtoVisitor<'b> = RpcVisitor<ProtoWriter<'b>>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Visitor(#[from] AV::Error),

    #[error("Deserialized value too large")]
    OutOfBudget,

    #[error("Exceeded maximum depth")]
    TooNested,

    #[error("Unexpected type")]
    UnexpectedType,
}

impl ProtoVisitorBuilder {
    pub fn new(bound: usize) -> Self {
        Self { bound }
    }

    /// Deserialize `bytes` as a `MoveValue` with layout `layout`. Can fail if the bytes do not
    /// represent a value with this layout, or if the deserialized value exceeds the field/type size
    /// budget.
    pub fn deserialize_value(
        mut self,
        bytes: &[u8],
        layout: &A::MoveTypeLayout,
    ) -> Result<Value, Error> {
        A::MoveValue::visit_deserialize(
            bytes,
            layout,
            &mut RpcVisitor::new(ProtoWriter {
                bound: &mut self.bound,
                depth: 0,
            }),
        )
    }
}

impl ProtoWriter<'_> {
    fn debit(&mut self, size: usize) -> Result<(), Error> {
        if *self.bound < size {
            Err(Error::OutOfBudget)
        } else {
            *self.bound -= size;
            Ok(())
        }
    }

    fn debit_value(&mut self) -> Result<(), Error> {
        self.debit(size_of::<Value>())
    }

    fn debit_str(&mut self, s: &str) -> Result<(), Error> {
        self.debit(s.len())
    }

    fn debit_string_value(&mut self, s: &str) -> Result<(), Error> {
        self.debit_str(s)?;
        self.debit_value()
    }
}

impl<'b> Writer for ProtoWriter<'b> {
    type Value = Value;
    type Error = Error;

    type Vec = Vec<Value>;
    type Map = Struct;

    type Nested<'a>
        = ProtoWriter<'a>
    where
        Self: 'a;

    fn nest(&mut self) -> Result<Self::Nested<'_>, Self::Error> {
        if self.depth >= MAX_DEPTH {
            Err(Error::TooNested)
        } else {
            Ok(ProtoWriter {
                bound: self.bound,
                depth: self.depth + 1,
            })
        }
    }

    fn write_null(&mut self) -> Result<Self::Value, Self::Error> {
        self.debit_value()?;
        Ok(Kind::NullValue(0).into())
    }

    fn write_bool(&mut self, value: bool) -> Result<Self::Value, Self::Error> {
        self.debit_value()?;
        Ok(Kind::BoolValue(value).into())
    }

    fn write_number(&mut self, value: u32) -> Result<Self::Value, Self::Error> {
        self.debit_value()?;
        Ok(Value::from(value))
    }

    fn write_str(&mut self, value: String) -> Result<Self::Value, Self::Error> {
        self.debit_string_value(&value)?;
        Ok(Kind::StringValue(value).into())
    }

    fn write_vec(&mut self, value: Self::Vec) -> Result<Self::Value, Self::Error> {
        self.debit_value()?;
        Ok(Value::from(value))
    }

    fn write_map(&mut self, value: Self::Map) -> Result<Self::Value, Self::Error> {
        self.debit_value()?;
        Ok(Value::from(Kind::StructValue(value)))
    }

    fn vec_push_element(
        &mut self,
        vec: &mut Self::Vec,
        val: Self::Value,
    ) -> Result<(), Self::Error> {
        vec.push(val);
        Ok(())
    }

    fn map_push_field(
        &mut self,
        map: &mut Self::Map,
        key: String,
        val: Self::Value,
    ) -> Result<(), Self::Error> {
        self.debit_str(&key)?;
        map.fields.insert(key, val);
        Ok(())
    }
}

impl From<RpcVisitorError> for Error {
    fn from(RpcVisitorError: RpcVisitorError) -> Self {
        Error::UnexpectedType
    }
}

impl From<OV::Error> for Error {
    fn from(OV::Error: OV::Error) -> Self {
        Error::UnexpectedType
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

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

        let deser = ProtoVisitorBuilder::new(bound)
            .deserialize_value(&bytes, &type_layout)
            .unwrap();

        assert_eq!(expected, proto_value_to_json_value(deser));

        ProtoVisitorBuilder::new(bound - 1)
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

        let deser = ProtoVisitorBuilder::new(bound)
            .deserialize_value(&bytes, &layout)
            .unwrap();

        assert_eq!(expected, proto_value_to_json_value(deser));

        // One deeper
        layout = layout_("0x0::foo::Bar", vec![("f", layout)]);
        value = value_("0x0::foo::Bar", vec![("f", value)]);

        let bytes = serialize(value.clone());

        let err = ProtoVisitorBuilder::new(bound)
            .deserialize_value(&bytes, &layout)
            .unwrap_err();

        let expect = expect!["Exceeded maximum depth"];
        expect.assert_eq(&err.to_string());
    }

    fn proto_value_to_json_value(proto: Value) -> serde_json::Value {
        match proto.kind {
            Some(Kind::NullValue(_)) | None => serde_json::Value::Null,
            // Move doesn't support floats so for these tests can do a convert to u32
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
