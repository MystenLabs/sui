// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::fmt::Write as _;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use sui_types::object::rpc_visitor as RV;
use sui_types::object::rpc_visitor::Meter as _;
use sui_types::object::rpc_visitor::Unmetered;

use crate::v2::error::FormatError;
use crate::v2::parser::Transform;
use crate::v2::value as V;

/// Shared output meter for Display rendering. Budget usage is sticky across all render operations
/// that share the same `used_size` counter.
pub(crate) struct Meter<'a> {
    used_size: &'a AtomicUsize,
    max_size: usize,
    depth_budget: usize,
}

/// A writer of strings backed by a shared output meter.
pub(crate) struct StringWriter<'a> {
    output: String,
    meter: Meter<'a>,
}

impl<'a> Meter<'a> {
    pub(crate) fn new(used_size: &'a AtomicUsize, max_size: usize, depth_budget: usize) -> Self {
        Self {
            used_size,
            max_size,
            depth_budget,
        }
    }

    pub(crate) fn reborrow(&mut self) -> Meter<'_> {
        Meter::new(self.used_size, self.max_size, self.depth_budget)
    }
}

impl<'a> StringWriter<'a> {
    fn new(meter: Meter<'a>) -> Self {
        Self {
            output: String::new(),
            meter,
        }
    }

    fn finish<F: RV::Format>(self) -> F {
        // The original meter has already been charged for the output, so do not apply metering
        // again when formatting as a string.
        F::string(&mut Unmetered, self.output).unwrap()
    }
}

impl RV::Meter for Meter<'_> {
    type Nested<'a>
        = Meter<'a>
    where
        Self: 'a;

    fn nest(&mut self) -> Result<Self::Nested<'_>, RV::MeterError> {
        if self.depth_budget == 0 {
            Err(RV::MeterError::TooDeep)
        } else {
            Ok(Meter::new(
                self.used_size,
                self.max_size,
                self.depth_budget - 1,
            ))
        }
    }

    fn charge(&mut self, amount: usize) -> Result<(), RV::MeterError> {
        let prev = self.used_size.fetch_add(amount, Ordering::Relaxed);
        if prev + amount > self.max_size {
            Err(RV::MeterError::TooBig)
        } else {
            Ok(())
        }
    }
}

impl fmt::Write for StringWriter<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.meter.charge(s.len()).map_err(|_| std::fmt::Error)?;
        self.output.push_str(s);
        Ok(())
    }
}

pub(crate) fn write<F: RV::Format>(
    meter: Meter<'_>,
    mut strands: Vec<V::Strand<'_>>,
) -> Result<F, FormatError> {
    if matches!(&strands[..], [V::Strand::Value { transform, .. }] if *transform == Transform::Json)
    {
        let V::Strand::Value { offset, value, .. } = strands.pop().unwrap() else {
            unreachable!();
        };

        return value
            .format_json(meter)
            .map_err(|e| e.for_expr_at_offset(offset));
    }

    let mut writer = StringWriter::new(meter);
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

    Ok(writer.finish())
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use serde_json::Value as Json;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_bounded_writer_under_limit() {
        let used = AtomicUsize::new(0);
        let mut w = StringWriter::new(Meter::new(&used, 100, usize::MAX));

        write!(w, "Hello").unwrap();
        write!(w, " ").unwrap();
        write!(w, "World").unwrap();

        let output: Json = w.finish();
        assert_eq!(output, json!("Hello World"));
        assert_eq!(used.load(Ordering::Relaxed), 11);
    }

    #[test]
    fn test_bounded_writer_at_limit() {
        let used = AtomicUsize::new(0);
        let mut w = StringWriter::new(Meter::new(&used, 5, usize::MAX));

        write!(w, "Hello").unwrap();

        let output: Json = w.finish();
        assert_eq!(output, json!("Hello"));
        assert_eq!(used.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn test_bounded_writer_exceeds_limit() {
        let used = AtomicUsize::new(0);
        let mut w = StringWriter::new(Meter::new(&used, 5, usize::MAX));

        write!(w, "Hello").unwrap();
        write!(w, " World").unwrap_err();
        assert!(used.load(Ordering::Relaxed) > 5);
    }

    #[test]
    fn test_bounded_writer_shared_counter() {
        let used = AtomicUsize::new(0);

        let mut w = StringWriter::new(Meter::new(&used, 20, usize::MAX));
        write!(w, "First").unwrap();
        let output: Json = w.finish();
        assert_eq!(output, json!("First"));
        assert_eq!(used.load(Ordering::Relaxed), 5);

        let mut w = StringWriter::new(Meter::new(&used, 20, usize::MAX));
        write!(w, "Second").unwrap();
        let output: Json = w.finish();
        assert_eq!(output, json!("Second"));
        assert_eq!(used.load(Ordering::Relaxed), 11);
    }

    #[test]
    fn test_bounded_writer_shared_counter_exceeds_limit() {
        let used = AtomicUsize::new(0);

        let mut w = StringWriter::new(Meter::new(&used, 10, usize::MAX));
        write!(w, "First").unwrap();

        let mut w = StringWriter::new(Meter::new(&used, 10, usize::MAX));
        write!(w, "Second").unwrap_err();

        assert!(used.load(Ordering::Relaxed) > 10);
    }

    #[test]
    fn test_bounded_writer_sticky_error() {
        let used = AtomicUsize::new(0);
        let mut writer = StringWriter::new(Meter::new(&used, 6, usize::MAX));

        write!(writer, "Hello").unwrap();
        write!(writer, " World").unwrap_err();
        write!(writer, "!").unwrap_err();
    }

    #[test]
    fn test_bounded_writer_multiple_shared_writers() {
        let used = AtomicUsize::new(0);
        let limit = 20;

        let mut w0 = StringWriter::new(Meter::new(&used, limit, usize::MAX));
        let mut w1 = StringWriter::new(Meter::new(&used, limit, usize::MAX));
        let mut w2 = StringWriter::new(Meter::new(&used, limit, usize::MAX));

        write!(w0, "AAA").unwrap();
        write!(w1, "BBBB").unwrap();
        write!(w2, "CCCCC").unwrap();

        assert_eq!(used.load(Ordering::Relaxed), 12);
        assert_eq!(w0.finish::<Json>(), json!("AAA"));
        assert_eq!(w1.finish::<Json>(), json!("BBBB"));
        assert_eq!(w2.finish::<Json>(), json!("CCCCC"));
    }
}
