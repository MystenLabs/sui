// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::fmt::Write as _;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use sui_types::object::rpc_visitor as RV;

use crate::v2::error::FormatError;
use crate::v2::parser::Transform;
use crate::v2::value as V;

pub trait JsonValue {
    type Vec: Default;
    type Map: Default;

    fn null() -> Self;
    fn bool(value: bool) -> Self;
    // For our purposes numbers are values that can fit in a `u32`
    fn number(value: u32) -> Self;
    fn string(value: String) -> Self;
    fn array(value: Self::Vec) -> Self;
    fn object(value: Self::Map) -> Self;

    fn is_null(&self) -> bool;
    fn is_bool(&self) -> bool;
    fn is_number(&self) -> bool;
    fn is_string(&self) -> bool;
    fn is_array(&self) -> bool;
    fn is_object(&self) -> bool;

    fn as_bool(&self) -> Option<bool>;
    fn as_string(&self) -> Option<&str>;
    fn as_array(&self) -> Option<&Self::Vec>;
    fn as_object(&self) -> Option<&Self::Map>;

    fn vec_push_element(vec: &mut Self::Vec, value: Self);
    fn map_push_field(map: &mut Self::Map, key: String, value: Self);
}

impl JsonValue for serde_json::Value {
    type Vec = Vec<Self>;
    type Map = serde_json::Map<String, Self>;

    fn null() -> Self {
        serde_json::Value::Null
    }

    fn bool(value: bool) -> Self {
        serde_json::Value::Bool(value)
    }

    fn number(value: u32) -> Self {
        serde_json::Value::Number(value.into())
    }

    fn string(value: String) -> Self {
        serde_json::Value::String(value)
    }

    fn array(value: Self::Vec) -> Self {
        serde_json::Value::Array(value)
    }

    fn object(value: Self::Map) -> Self {
        serde_json::Value::Object(value)
    }

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

    fn vec_push_element(vec: &mut Self::Vec, value: Self) {
        vec.push(value);
    }

    fn map_push_field(map: &mut Self::Map, key: String, value: Self) {
        map.insert(key, value);
    }
}

/// A writer of evaluated values into JSON, tracking limits on output size and depth. A single
/// writer can be used to write multiple values concurrently, with all writers sharing the same
/// budgets.
///
/// Once a write is attempted, the budgets are decremented, even if the write fails, meaning that
/// the errors are sticky and not recoverable.
pub(crate) struct Writer {
    max_depth: usize,
    max_output_size: usize,
    used_output: AtomicUsize,
}

/// A writer of strings that tracks an output budget (measured in bytes) shared across multiple
/// writers. Writes will fail once the total number of bytes sent to be written, across all
/// writers, exceeds the maximum, and will continue to fail from there on out.
///
/// Once a write is attempted, the budget is decremented, even if the write fails, meaning that the
/// error is sticky and not recoverable.
pub(crate) struct StringWriter<'u> {
    output: String,
    used: &'u AtomicUsize,
    max: usize,
}

/// A writer of structured JSON values that tracks an output budget (measured in bytes) shared
/// across multiple writers. Writes will fail once the total number of bytes sent to be written,
/// across all writers, exceeds the maximum, and will continue to fail from there on out.
///
/// Once a write is attempted, the budget is decremented, even if the write fails, meaning that the
/// error is sticky and not recoverable.
pub(crate) struct JsonWriter<'u, V = serde_json::Value> {
    used_size: &'u AtomicUsize,
    max_size: usize,
    depth_budget: usize,
    phantom: std::marker::PhantomData<V>,
}

impl<V> Clone for JsonWriter<'_, V> {
    fn clone(&self) -> Self {
        Self {
            used_size: self.used_size,
            max_size: self.max_size,
            depth_budget: self.depth_budget,
            phantom: self.phantom,
        }
    }
}

impl<V> Copy for JsonWriter<'_, V> {}

impl Writer {
    /// Create a new writer with the given limits.
    ///
    /// `max_depth` specifies the maximum depth of the output JSON, and `max_output_size` specifies
    /// the size in bytes of the output JSON.
    pub(crate) fn new(max_depth: usize, max_output_size: usize) -> Self {
        Self {
            max_depth,
            max_output_size,
            used_output: AtomicUsize::new(0),
        }
    }

    /// Format a single strand as JSON.
    pub(crate) fn write<JSON: JsonValue>(
        &self,
        mut strands: Vec<V::Strand<'_>>,
    ) -> Result<JSON, FormatError> {
        // Detect and handle JSON transforms (single strand containing an expression with an JSON
        // transform) as a special case, because they do not always evaluate to strings.
        if matches!(&strands[..], [V::Strand::Value { transform, .. }] if *transform == Transform::Json)
        {
            let V::Strand::Value { offset, value, .. } = strands.pop().unwrap() else {
                unreachable!();
            };

            let writer = JsonWriter::new(&self.used_output, self.max_output_size, self.max_depth);
            return value
                .format_json(writer)
                .map_err(|e| e.for_expr_at_offset(offset));
        }

        // Otherwise gather the results of formatting all strands into a single string.
        let mut writer = StringWriter::new(&self.used_output, self.max_output_size);
        for strand in strands {
            match strand {
                V::Strand::Text(s) => writer
                    .write_str(s)
                    .map_err(|_| FormatError::TooMuchOutput)?,

                V::Strand::Value {
                    offset,
                    value,
                    transform,
                } => value
                    .format(transform, &mut writer)
                    .map_err(|e| e.for_expr_at_offset(offset))?,
            }
        }

        Ok(JSON::string(writer.finish()))
    }
}

impl<'u> StringWriter<'u> {
    fn new(used: &'u AtomicUsize, max: usize) -> Self {
        Self {
            output: String::new(),
            used,
            max,
        }
    }

    fn finish(self) -> String {
        self.output
    }
}

impl<'u, V> JsonWriter<'u, V> {
    pub(crate) fn new(used_size: &'u AtomicUsize, max_size: usize, depth_budget: usize) -> Self {
        Self {
            used_size,
            max_size,
            depth_budget,
            phantom: std::marker::PhantomData,
        }
    }

    fn debit(&self, size: usize) -> Result<(), FormatError> {
        let prev = self.used_size.fetch_add(size, Ordering::Relaxed);
        if prev + size > self.max_size {
            return Err(FormatError::TooBig);
        }
        Ok(())
    }
}

impl<V: JsonValue> RV::Writer for JsonWriter<'_, V> {
    type Value = V;
    type Error = FormatError;

    type Vec = V::Vec;
    type Map = V::Map;

    type Nested<'a>
        = JsonWriter<'a, V>
    where
        Self: 'a;

    fn nest(&mut self) -> Result<Self::Nested<'_>, Self::Error> {
        if self.depth_budget == 0 {
            return Err(FormatError::TooDeep);
        }

        Ok(JsonWriter {
            used_size: self.used_size,
            max_size: self.max_size,
            depth_budget: self.depth_budget - 1,
            phantom: std::marker::PhantomData,
        })
    }

    fn write_null(&mut self) -> Result<Self::Value, Self::Error> {
        self.debit("null".len())?;
        Ok(V::null())
    }

    fn write_bool(&mut self, value: bool) -> Result<Self::Value, Self::Error> {
        self.debit(if value { "true".len() } else { "false".len() })?;
        Ok(V::bool(value))
    }

    fn write_number(&mut self, value: u32) -> Result<Self::Value, Self::Error> {
        self.debit(if value == 0 { 1 } else { value.ilog10() } as usize)?;
        Ok(V::number(value))
    }

    fn write_str(&mut self, value: String) -> Result<Self::Value, Self::Error> {
        // Account for the quotes around the string.
        self.debit(2 + value.len())?;
        Ok(V::string(value))
    }

    fn write_vec(&mut self, value: Self::Vec) -> Result<Self::Value, Self::Error> {
        // Account for the opening bracket.
        self.debit(1)?;
        Ok(V::array(value))
    }

    fn write_map(&mut self, value: Self::Map) -> Result<Self::Value, Self::Error> {
        // Account for the opening brace.
        self.debit(1)?;
        Ok(V::object(value))
    }

    fn vec_push_element(
        &mut self,
        vec: &mut Self::Vec,
        val: Self::Value,
    ) -> Result<(), Self::Error> {
        // Account for comma (or closing bracket).
        self.debit(1)?;
        V::vec_push_element(vec, val);
        Ok(())
    }

    fn map_push_field(
        &mut self,
        map: &mut Self::Map,
        key: String,
        val: Self::Value,
    ) -> Result<(), Self::Error> {
        // Account for quotes, colon, and comma (or closing brace).
        self.debit(4 + key.len())?;
        V::map_push_field(map, key, val);
        Ok(())
    }
}

impl fmt::Write for StringWriter<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let prev = self.used.fetch_add(s.len(), Ordering::Relaxed);
        if prev + s.len() > self.max {
            return Err(std::fmt::Error);
        }

        self.output.push_str(s);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write;

    #[test]
    fn test_bounded_writer_under_limit() {
        let used = AtomicUsize::new(0);
        let mut w = StringWriter::new(&used, 100);

        write!(w, "Hello").unwrap();
        write!(w, " ").unwrap();
        write!(w, "World").unwrap();

        assert_eq!(w.finish(), "Hello World");
        assert_eq!(used.load(Ordering::Relaxed), 11);
    }

    #[test]
    fn test_bounded_writer_at_limit() {
        let used = AtomicUsize::new(0);
        let mut w = StringWriter::new(&used, 5);

        write!(w, "Hello").unwrap();

        assert_eq!(w.finish(), "Hello");
        assert_eq!(used.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn test_bounded_writer_exceeds_limit() {
        let used = AtomicUsize::new(0);
        let mut w = StringWriter::new(&used, 5);

        write!(w, "Hello").unwrap();
        write!(w, " World").unwrap_err();
        assert!(used.load(Ordering::Relaxed) > 5);
    }

    #[test]
    fn test_bounded_writer_shared_counter() {
        let used = AtomicUsize::new(0);

        let mut w = StringWriter::new(&used, 20);
        write!(w, "First").unwrap();
        assert_eq!(w.finish(), "First");
        assert_eq!(used.load(Ordering::Relaxed), 5);

        let mut w = StringWriter::new(&used, 20);
        write!(w, "Second").unwrap();
        assert_eq!(w.finish(), "Second");
        assert_eq!(used.load(Ordering::Relaxed), 11);
    }

    #[test]
    fn test_bounded_writer_shared_counter_exceeds_limit() {
        let used = AtomicUsize::new(0);

        let mut w = StringWriter::new(&used, 10);
        write!(w, "First").unwrap();

        let mut w = StringWriter::new(&used, 10);
        write!(w, "Second").unwrap_err();

        assert!(used.load(Ordering::Relaxed) > 10);
    }

    #[test]
    fn test_bounded_writer_sticky_error() {
        let used = AtomicUsize::new(0);
        let mut writer = StringWriter::new(&used, 6);

        write!(writer, "Hello").unwrap();
        write!(writer, " World").unwrap_err();
        write!(writer, "!").unwrap_err();
    }

    #[test]
    fn test_bounded_writer_multiple_shared_writers() {
        let used = AtomicUsize::new(0);
        let limit = 20;

        let mut w0 = StringWriter::new(&used, limit);
        let mut w1 = StringWriter::new(&used, limit);
        let mut w2 = StringWriter::new(&used, limit);

        write!(w0, "AAA").unwrap();
        write!(w1, "BBBB").unwrap();
        write!(w2, "CCCCC").unwrap();

        assert_eq!(used.load(Ordering::Relaxed), 12);
        assert_eq!(w0.finish(), "AAA");
        assert_eq!(w1.finish(), "BBBB");
        assert_eq!(w2.finish(), "CCCCC");
    }
}
