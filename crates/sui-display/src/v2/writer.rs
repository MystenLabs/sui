// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use sui_types::object::rpc_visitor as RV;

use crate::v2::error::FormatError;

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
#[derive(Copy, Clone)]
pub(crate) struct JsonWriter<'u> {
    used_size: &'u AtomicUsize,
    max_size: usize,
    depth_budget: usize,
}

impl<'u> StringWriter<'u> {
    pub(crate) fn new(used: &'u AtomicUsize, max: usize) -> Self {
        Self {
            output: String::new(),
            used,
            max,
        }
    }

    pub(crate) fn finish(self) -> String {
        self.output
    }
}

impl<'u> JsonWriter<'u> {
    pub(crate) fn new(used_size: &'u AtomicUsize, max_size: usize, depth_budget: usize) -> Self {
        Self {
            used_size,
            max_size,
            depth_budget,
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

impl RV::Writer for JsonWriter<'_> {
    type Value = serde_json::Value;
    type Error = FormatError;

    type Vec = Vec<Self::Value>;
    type Map = serde_json::Map<String, Self::Value>;

    type Nested<'a>
        = JsonWriter<'a>
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
        })
    }

    fn write_null(&mut self) -> Result<Self::Value, Self::Error> {
        self.debit("null".len())?;
        Ok(serde_json::Value::Null)
    }

    fn write_bool(&mut self, value: bool) -> Result<Self::Value, Self::Error> {
        self.debit(if value { "true".len() } else { "false".len() })?;
        Ok(serde_json::Value::Bool(value))
    }

    fn write_number(&mut self, value: u32) -> Result<Self::Value, Self::Error> {
        self.debit(if value == 0 { 1 } else { value.ilog10() } as usize)?;
        Ok(serde_json::Value::Number(value.into()))
    }

    fn write_str(&mut self, value: String) -> Result<Self::Value, Self::Error> {
        // Account for the quotes around the string.
        self.debit(2 + value.len())?;
        Ok(serde_json::Value::String(value))
    }

    fn write_vec(&mut self, value: Self::Vec) -> Result<Self::Value, Self::Error> {
        // Account for the opening bracket.
        self.debit(1)?;
        Ok(serde_json::Value::Array(value))
    }

    fn write_map(&mut self, value: Self::Map) -> Result<Self::Value, Self::Error> {
        // Account for the opening brace.
        self.debit(1)?;
        Ok(serde_json::Value::Object(value))
    }

    fn vec_push_element(
        &mut self,
        vec: &mut Self::Vec,
        val: Self::Value,
    ) -> Result<(), Self::Error> {
        // Account for comma (or closing bracket).
        self.debit(1)?;
        vec.push(val);
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
        map.insert(key, val);
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
