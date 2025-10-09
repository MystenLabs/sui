// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
};

/// A writer that tracks an output budget (measured in bytes) shared across multiple writers.
/// Writes will fail once the total number of bytes sent to be written, across all writers, exceeds
/// the maximum, and will continue to fail from there on out.
///
/// Once a write is attempted, the budget is decremented, even if the write fails, meaning that the
/// error is sticky and not recoverable.
pub(crate) struct BoundedWriter<'u> {
    output: String,
    used: &'u AtomicUsize,
    max: usize,
}

impl<'u> BoundedWriter<'u> {
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

impl fmt::Write for BoundedWriter<'_> {
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
        let mut w = BoundedWriter::new(&used, 100);

        write!(w, "Hello").unwrap();
        write!(w, " ").unwrap();
        write!(w, "World").unwrap();

        assert_eq!(w.finish(), "Hello World");
        assert_eq!(used.load(Ordering::Relaxed), 11);
    }

    #[test]
    fn test_bounded_writer_at_limit() {
        let used = AtomicUsize::new(0);
        let mut w = BoundedWriter::new(&used, 5);

        write!(w, "Hello").unwrap();

        assert_eq!(w.finish(), "Hello");
        assert_eq!(used.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn test_bounded_writer_exceeds_limit() {
        let used = AtomicUsize::new(0);
        let mut w = BoundedWriter::new(&used, 5);

        write!(w, "Hello").unwrap();
        write!(w, " World").unwrap_err();
        assert!(used.load(Ordering::Relaxed) > 5);
    }

    #[test]
    fn test_bounded_writer_shared_counter() {
        let used = AtomicUsize::new(0);

        let mut w = BoundedWriter::new(&used, 20);
        write!(w, "First").unwrap();
        assert_eq!(w.finish(), "First");
        assert_eq!(used.load(Ordering::Relaxed), 5);

        let mut w = BoundedWriter::new(&used, 20);
        write!(w, "Second").unwrap();
        assert_eq!(w.finish(), "Second");
        assert_eq!(used.load(Ordering::Relaxed), 11);
    }

    #[test]
    fn test_bounded_writer_shared_counter_exceeds_limit() {
        let used = AtomicUsize::new(0);

        let mut w = BoundedWriter::new(&used, 10);
        write!(w, "First").unwrap();

        let mut w = BoundedWriter::new(&used, 10);
        write!(w, "Second").unwrap_err();

        assert!(used.load(Ordering::Relaxed) > 10);
    }

    #[test]
    fn test_bounded_writer_sticky_error() {
        let used = AtomicUsize::new(0);
        let mut writer = BoundedWriter::new(&used, 6);

        write!(writer, "Hello").unwrap();
        write!(writer, " World").unwrap_err();
        write!(writer, "!").unwrap_err();
    }

    #[test]
    fn test_bounded_writer_multiple_shared_writers() {
        let used = AtomicUsize::new(0);
        let limit = 20;

        let mut w0 = BoundedWriter::new(&used, limit);
        let mut w1 = BoundedWriter::new(&used, limit);
        let mut w2 = BoundedWriter::new(&used, limit);

        write!(w0, "AAA").unwrap();
        write!(w1, "BBBB").unwrap();
        write!(w2, "CCCCC").unwrap();

        assert_eq!(used.load(Ordering::Relaxed), 12);
        assert_eq!(w0.finish(), "AAA");
        assert_eq!(w1.finish(), "BBBB");
        assert_eq!(w2.finish(), "CCCCC");
    }
}
