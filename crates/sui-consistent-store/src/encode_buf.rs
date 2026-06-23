// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Internal thread-local scratch buffer used by encoding-side call
//! sites.
//!
//! Most operations on a typed column-family handle encode at least one
//! key, and writes encode a value alongside. RocksDB's read and write
//! APIs copy or consume the supplied byte slices synchronously, so the
//! scratch buffer can be reused across operations on the same thread.

use std::cell::RefCell;

/// Maximum capacity retained on the thread-local buffer between calls.
/// One-off encodes of values larger than this shrink back to this
/// bound on Drop so they do not pin a large allocation indefinitely.
const MAX_RETAIN_BYTES: usize = 1 << 20;

thread_local! {
    static ENCODE_BUF: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Run `f` with access to the thread-local scratch buffer.
///
/// The buffer is cleared on entry and shrunk back to
/// [`MAX_RETAIN_BYTES`] on exit if it grew past that bound. Multiple
/// values can be encoded into the same buffer in sequence: record the
/// length after each `encode_into` call to obtain non-overlapping
/// subslice offsets, then take immutable subslices once all encodes
/// finish.
pub(crate) fn with_encode_buf<R>(f: impl FnOnce(&mut Vec<u8>) -> R) -> R {
    ENCODE_BUF.with_borrow_mut(|buf| {
        buf.clear();
        let result = f(buf);
        // Clear before shrinking: `Vec::shrink_to` cannot reduce
        // capacity below the current length.
        buf.clear();
        if buf.capacity() > MAX_RETAIN_BYTES {
            buf.shrink_to(MAX_RETAIN_BYTES);
        }
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_is_cleared_on_entry() {
        with_encode_buf(|buf| {
            buf.extend_from_slice(&[1, 2, 3]);
        });
        with_encode_buf(|buf| {
            assert!(buf.is_empty());
        });
    }

    #[test]
    fn capacity_is_shrunk_after_oversized_encode() {
        with_encode_buf(|buf| {
            buf.resize(MAX_RETAIN_BYTES * 2, 0);
        });
        with_encode_buf(|buf| {
            assert!(buf.capacity() <= MAX_RETAIN_BYTES);
        });
    }

    #[test]
    fn small_capacities_are_retained_for_reuse() {
        with_encode_buf(|buf| {
            buf.reserve(1024);
            buf.extend_from_slice(&[7; 16]);
        });
        with_encode_buf(|buf| {
            assert!(buf.capacity() >= 1024);
            assert!(buf.is_empty());
        });
    }

    #[test]
    fn returns_value_from_closure() {
        let result = with_encode_buf(|buf| {
            buf.extend_from_slice(b"hello");
            buf.len()
        });
        assert_eq!(result, 5);
    }
}
