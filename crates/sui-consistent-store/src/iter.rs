// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Typed iteration over a column family.
//!
//! Two iterator types are exposed: [`Iter`] for forward iteration in
//! lexicographic key order, and [`RevIter`] for reverse iteration.
//! Both implement [`Iterator`] yielding
//! `Result<(K, V), Error>` so that decode failures are reported as
//! per-item errors rather than poisoning the entire scan; the
//! iterator stops yielding after the first error.
//!
//! Construct an iterator via [`DbMap::iter`](crate::DbMap::iter),
//! [`iter_rev`](crate::DbMap::iter_rev),
//! [`iter_prefix`](crate::DbMap::iter_prefix), or
//! [`iter_rev_prefix`](crate::DbMap::iter_rev_prefix).
//!
//! # Range and prefix iteration
//!
//! `iter` and `iter_rev` take a [`RangeBounds<K>`](std::ops::RangeBounds)
//! and produce iterators bounded by it. Use `..` for an unbounded
//! scan, `start..end` (or any combination of `Included` / `Excluded`
//! bounds) for a typed range.
//!
//! `iter_prefix` and `iter_rev_prefix` accept a separate prefix type
//! whose [`Encode`] form is treated as a byte prefix of the full
//! key's encoded form. The crate does *not* statically verify that
//! the prefix encoding really is a prefix of the key encoding; that
//! is a property of the schema's chosen encoding scheme. In practice,
//! schemas that encode compound keys with a prefix-preserving
//! representation (for example, big-endian fixed-int tuples) and
//! pass tuple prefixes get the right behavior.
//!
//! Internally, both range and prefix iteration set RocksDB's
//! `iterate_lower_bound` and `iterate_upper_bound` to the
//! corresponding byte sequences, so the underlying scan stops cleanly
//! at the bound without per-item filtering on the Rust side. The
//! same byte-level helper backs all four entry points.
//!
//! # Cursor methods
//!
//! [`Iter`] and [`RevIter`] expose lower-level cursor methods —
//! [`seek`](Iter::seek), [`skip_past`](Iter::skip_past),
//! [`raw_key`](Iter::raw_key), [`raw_value`](Iter::raw_value), and
//! [`valid`](Iter::valid) — for advanced traversal patterns (for
//! example, byte-cursor pagination that does not want to round-trip
//! through `K`'s decoder).

use std::marker::PhantomData;
use std::ops::Bound;
use std::ops::RangeBounds;

use rocksdb::DBRawIterator;

use crate::Decode;
use crate::Encode;
use crate::error::EncodeError;
use crate::error::Error;

/// Forward iterator over a column family in lexicographic key order.
///
/// Constructed by [`DbMap::iter`](crate::DbMap::iter) or
/// [`DbMap::iter_prefix`](crate::DbMap::iter_prefix).
pub struct Iter<'d, K, V> {
    inner: Option<DBRawIterator<'d>>,
    _phantom: PhantomData<fn() -> (K, V)>,
}

/// Reverse iterator over a column family in reverse lexicographic
/// key order.
///
/// Constructed by [`DbMap::iter_rev`](crate::DbMap::iter_rev) or
/// [`DbMap::iter_rev_prefix`](crate::DbMap::iter_rev_prefix).
pub struct RevIter<'d, K, V> {
    inner: Option<DBRawIterator<'d>>,
    _phantom: PhantomData<fn() -> (K, V)>,
}

impl<'d, K, V> Iter<'d, K, V> {
    pub(crate) fn new(inner: DBRawIterator<'d>) -> Self {
        Self {
            inner: Some(inner),
            _phantom: PhantomData,
        }
    }

    /// An iterator that yields nothing. Used by callers that
    /// determine, before issuing any RocksDB call, that the requested
    /// range is provably empty (for example, an `Excluded(MAX)` lower
    /// bound).
    pub(crate) fn empty() -> Self {
        Self {
            inner: None,
            _phantom: PhantomData,
        }
    }

    /// Move the cursor to the first key greater than or equal to
    /// `probe`.
    ///
    /// `probe` is interpreted as raw bytes; it does not have to be
    /// produced from `K`. Useful for byte-cursor pagination.
    pub fn seek(&mut self, probe: impl AsRef<[u8]>) {
        if let Some(inner) = &mut self.inner {
            inner.seek(probe);
        }
    }

    /// Skip past every key whose bytes start with `prefix`. The
    /// cursor lands on the first key that does not start with
    /// `prefix`, or becomes invalid if no such key exists.
    pub fn skip_past(&mut self, prefix: impl AsRef<[u8]>) {
        if let Some(inner) = &mut self.inner {
            match next_prefix(prefix.as_ref()) {
                Some(end) => inner.seek(&end),
                None => {
                    // No successor: position past the end so the
                    // iterator becomes invalid.
                    inner.seek_to_last();
                    inner.next();
                }
            }
        }
    }

    /// Returns the raw bytes of the next key the iterator will yield,
    /// or `None` if the cursor is invalid.
    pub fn raw_key(&self) -> Option<&[u8]> {
        self.inner.as_ref().and_then(|i| i.key())
    }

    /// Returns the raw bytes of the next value the iterator will
    /// yield, or `None` if the cursor is invalid.
    pub fn raw_value(&self) -> Option<&[u8]> {
        self.inner.as_ref().and_then(|i| i.value())
    }

    /// Returns whether the cursor currently points at a key.
    pub fn valid(&self) -> bool {
        self.inner.as_ref().is_some_and(|i| i.valid())
    }
}

impl<'d, K, V> RevIter<'d, K, V> {
    pub(crate) fn new(inner: DBRawIterator<'d>) -> Self {
        Self {
            inner: Some(inner),
            _phantom: PhantomData,
        }
    }

    /// An iterator that yields nothing. See [`Iter::empty`].
    pub(crate) fn empty() -> Self {
        Self {
            inner: None,
            _phantom: PhantomData,
        }
    }

    /// Move the cursor to the last key less than or equal to
    /// `probe`.
    ///
    /// `probe` is interpreted as raw bytes; it does not have to be
    /// produced from `K`.
    pub fn seek(&mut self, probe: impl AsRef<[u8]>) {
        if let Some(inner) = &mut self.inner {
            inner.seek_for_prev(probe);
        }
    }

    /// Skip past every key whose bytes start with `prefix` (in
    /// reverse). The cursor lands on the last key strictly before
    /// the prefix range, or becomes invalid if no such key exists.
    pub fn skip_past(&mut self, prefix: impl AsRef<[u8]>) {
        if let Some(inner) = &mut self.inner {
            let end = prefix.as_ref();
            inner.seek_for_prev(end);
            // The only way to land on a prefix-matching key here is
            // to land on `end` itself (the prefix as an exact key).
            // Step back once if so.
            if inner.key() == Some(end) {
                inner.prev();
            }
        }
    }

    /// Returns the raw bytes of the next key the iterator will yield,
    /// or `None` if the cursor is invalid.
    pub fn raw_key(&self) -> Option<&[u8]> {
        self.inner.as_ref().and_then(|i| i.key())
    }

    /// Returns the raw bytes of the next value the iterator will
    /// yield, or `None` if the cursor is invalid.
    pub fn raw_value(&self) -> Option<&[u8]> {
        self.inner.as_ref().and_then(|i| i.value())
    }

    /// Returns whether the cursor currently points at a key.
    pub fn valid(&self) -> bool {
        self.inner.as_ref().is_some_and(|i| i.valid())
    }
}

impl<K, V> Iterator for Iter<'_, K, V>
where
    K: Decode,
    V: Decode,
{
    type Item = Result<(K, V), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        next_step(&mut self.inner, true)
    }
}

impl<K, V> Iterator for RevIter<'_, K, V>
where
    K: Decode,
    V: Decode,
{
    type Item = Result<(K, V), Error>;

    fn next(&mut self) -> Option<Self::Item> {
        next_step(&mut self.inner, false)
    }
}

/// Compute the lexicographic successor of `prefix`, treating it as a
/// variable-length byte string.
///
/// Returns `Some(succ)` such that `succ` is the smallest byte string
/// strictly greater than `prefix` in lexicographic order. Returns
/// `None` if `prefix` is composed entirely of `0xFF` bytes (and so
/// has no successor in this ordering), in which case the prefix
/// extends to the end of the key space.
pub(crate) fn next_prefix(prefix: &[u8]) -> Option<Vec<u8>> {
    let mut next = prefix.to_vec();
    while let Some(last) = next.last_mut() {
        if *last < 0xFF {
            *last += 1;
            return Some(next);
        }
        next.pop();
    }
    None
}

/// Encoded byte bounds for an iterator.
///
/// `Empty` means the requested range is provably empty (no key can
/// match) — produced today by an `Excluded(k)` lower bound whose
/// encoded `k` is non-empty all-`0xFF` (no successor exists). Callers
/// short-circuit to an empty iterator on this variant.
///
/// `Range(lower, upper)` is the usual half-open
/// `[lower, upper)` byte interval; either side may be `None` to leave
/// that side unbounded.
pub(crate) enum ByteBounds {
    Empty,
    Range(Option<Vec<u8>>, Option<Vec<u8>>),
}

/// Convert a typed [`RangeBounds<J>`] into encoded byte bounds
/// suitable for RocksDB's `iterate_lower_bound` and
/// `iterate_upper_bound` (lower inclusive, upper exclusive).
///
/// `Excluded(start)` for the lower bound is implemented by encoding
/// `start` and taking its lex successor. If `start` encodes to a
/// non-empty all-`0xFF` byte string the successor does not exist in
/// the same byte length; the result is reported as
/// [`ByteBounds::Empty`] so iteration yields nothing. (For empty
/// encodings, where `next_prefix` also returns `None`, the bound is
/// silently dropped — `Excluded([])` is degenerate input.)
///
/// `Included(end)` for the upper bound takes the lex successor as
/// the exclusive upper. If `end` encodes to all-`0xFF` (no
/// successor), the upper bound is dropped — for fixed-length
/// encodings this is exact; variable-length encodings may iterate
/// extra rows whose encoded keys extend the all-`0xFF` prefix.
pub(crate) fn range_to_byte_bounds<J, R>(range: &R) -> Result<ByteBounds, EncodeError>
where
    J: Encode,
    R: RangeBounds<J>,
{
    let lower = match range.start_bound() {
        Bound::Included(k) => Some(k.encode()?),
        Bound::Excluded(k) => {
            let encoded = k.encode()?;
            match next_prefix(&encoded) {
                Some(succ) => Some(succ),
                // Empty encoding has no successor but the bound is
                // degenerate; drop it rather than declaring the
                // whole range empty.
                None if encoded.is_empty() => None,
                // Non-empty all-0xFF: provably no key satisfies the
                // bound (assuming fixed-length encoding); short-
                // circuit to an empty iteration.
                None => return Ok(ByteBounds::Empty),
            }
        }
        Bound::Unbounded => None,
    };
    let upper = match range.end_bound() {
        Bound::Excluded(k) => Some(k.encode()?),
        Bound::Included(k) => next_prefix(&k.encode()?),
        Bound::Unbounded => None,
    };
    Ok(ByteBounds::Range(lower, upper))
}

/// Convert a typed prefix into encoded byte bounds.
///
/// Lower bound is the encoded prefix; upper bound is the prefix's
/// lex successor (or `None` if the prefix is empty or all `0xFF`).
pub(crate) fn prefix_to_byte_bounds<P>(prefix: &P) -> Result<ByteBounds, EncodeError>
where
    P: Encode,
{
    let lower_bytes = prefix.encode()?;
    let upper = next_prefix(&lower_bytes);
    Ok(ByteBounds::Range(Some(lower_bytes), upper))
}

/// Shared `next` body for both directions. `forward = true` advances
/// via `DBRawIterator::next` after yielding; `forward = false`
/// advances via `prev`.
fn next_step<K, V>(
    slot: &mut Option<DBRawIterator<'_>>,
    forward: bool,
) -> Option<Result<(K, V), Error>>
where
    K: Decode,
    V: Decode,
{
    let inner = slot.as_mut()?;
    if !inner.valid() {
        // Either the iterator is exhausted or it errored. `status`
        // distinguishes the two.
        let err = inner.status().err();
        *slot = None;
        return err.map(|e| Err(Error::Rocksdb(e)));
    }

    // The iterator reports valid, so key/value must be present;
    // surface a defensive error rather than silently terminating if
    // RocksDB ever violates this invariant.
    let (Some(key_bytes), Some(value_bytes)) = (inner.key(), inner.value()) else {
        *slot = None;
        return Some(Err(Error::Internal(
            "raw iterator returned None for key or value while marked valid",
        )));
    };

    let item = match (
        K::decode(&mut &key_bytes[..]),
        V::decode(&mut &value_bytes[..]),
    ) {
        (Ok(k), Ok(v)) => Ok((k, v)),
        (Err(e), _) | (_, Err(e)) => {
            *slot = None;
            return Some(Err(Error::Decode(e)));
        }
    };

    if forward {
        inner.next();
    } else {
        inner.prev();
    }
    Some(item)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_prefix_increments_last_byte() {
        assert_eq!(next_prefix(&[1, 2, 3]), Some(vec![1, 2, 4]));
    }

    #[test]
    fn next_prefix_carries_when_last_is_0xff() {
        assert_eq!(next_prefix(&[1, 2, 0xFF]), Some(vec![1, 3]));
    }

    #[test]
    fn next_prefix_carries_through_multiple_0xff_bytes() {
        assert_eq!(next_prefix(&[1, 0xFF, 0xFF]), Some(vec![2]));
    }

    #[test]
    fn next_prefix_returns_none_for_all_0xff() {
        assert_eq!(next_prefix(&[0xFF, 0xFF]), None);
    }

    #[test]
    fn next_prefix_returns_none_for_empty() {
        assert_eq!(next_prefix(&[]), None);
    }

    /// Hand-rolled big-endian `u64` matching the test types in the
    /// rest of the crate.
    #[derive(Debug, Clone, Copy)]
    struct U64Be(u64);

    impl Encode for U64Be {
        fn encode_into<B: bytes::BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
            buf.put_slice(&self.0.to_be_bytes());
            Ok(())
        }
    }

    /// Destructure the `Range` variant for assertions; panics if
    /// `Empty` (the test-side equivalent of `unwrap`).
    fn range(b: ByteBounds) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
        match b {
            ByteBounds::Range(lo, hi) => (lo, hi),
            ByteBounds::Empty => panic!("expected ByteBounds::Range, got Empty"),
        }
    }

    #[test]
    fn range_to_byte_bounds_full_range() {
        let (lo, hi) = range(range_to_byte_bounds::<U64Be, _>(&(..)).unwrap());
        assert!(lo.is_none());
        assert!(hi.is_none());
    }

    #[test]
    fn range_to_byte_bounds_inclusive_to_exclusive() {
        let (lo, hi) = range(range_to_byte_bounds(&(U64Be(1)..U64Be(5))).unwrap());
        assert_eq!(lo.unwrap(), &1u64.to_be_bytes());
        assert_eq!(hi.unwrap(), &5u64.to_be_bytes());
    }

    #[test]
    fn range_to_byte_bounds_inclusive_to_inclusive() {
        let (lo, hi) = range(range_to_byte_bounds(&(U64Be(1)..=U64Be(5))).unwrap());
        assert_eq!(lo.unwrap(), &1u64.to_be_bytes());
        // 5 inclusive becomes lex successor of encoded 5.
        assert_eq!(hi.unwrap(), &6u64.to_be_bytes());
    }

    #[test]
    fn range_to_byte_bounds_from() {
        let (lo, hi) = range(range_to_byte_bounds(&(U64Be(7)..)).unwrap());
        assert_eq!(lo.unwrap(), &7u64.to_be_bytes());
        assert!(hi.is_none());
    }

    #[test]
    fn range_to_byte_bounds_to() {
        let (lo, hi) = range(range_to_byte_bounds(&(..U64Be(7))).unwrap());
        assert!(lo.is_none());
        assert_eq!(hi.unwrap(), &7u64.to_be_bytes());
    }

    #[test]
    fn range_to_byte_bounds_excluded_max_lower_is_empty() {
        // (Excluded(MAX), Unbounded) — no key > MAX in fixed-length
        // encodings. Previously silently widened to "no lower
        // bound"; now reports as Empty so iteration yields nothing.
        let bounds =
            range_to_byte_bounds(&(Bound::Excluded(U64Be(u64::MAX)), Bound::Unbounded)).unwrap();
        assert!(matches!(bounds, ByteBounds::Empty));
    }

    #[test]
    fn prefix_to_byte_bounds_returns_lower_and_successor() {
        let (lo, hi) = range(prefix_to_byte_bounds(&U64Be(42)).unwrap());
        assert_eq!(lo.unwrap(), &42u64.to_be_bytes());
        assert_eq!(hi.unwrap(), &43u64.to_be_bytes());
    }
}
