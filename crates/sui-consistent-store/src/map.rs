// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Typed column-family handles.
//!
//! [`DbMap<K, V, R>`] is the primary read and write surface in the
//! crate. Each instance is tied to a single column family on a
//! [`Db`] handle (carried by the reader), to a key type and a value
//! type that implement the crate's encoding traits, and to a
//! [`Reader`] that pins the consistency context.
//!
//! # The reader parameter
//!
//! `R` defaults to [`Db`], so today's call sites
//! (`DbMap::new(db, "items")`, schemas opened via
//! [`Db::open`](crate::Db::open)) work unchanged. To re-bind a map
//! at a captured snapshot, call [`DbMap::at`]; for the whole-schema
//! equivalent see
//! [`SchemaAtSnapshot::at`](crate::SchemaAtSnapshot::at). To
//! construct a borrowed-reader handle (no `Arc` bump), pass `&db`
//! or `&snap` to [`DbMap::new`] — the reader type is inferred from
//! the argument.
//!
//! Schemas are typically structs of `DbMap` fields parameterized by
//! `R`; see the [`Schema`](crate::Schema) trait for the construction
//! pattern.
//!
//! # Reading
//!
//! Two read methods are exposed for point lookups:
//!
//! - [`DbMap::get`] decodes the value into an owned `V` and is the
//!   common path. Internally it routes through RocksDB's
//!   `get_pinned_cf` to avoid the redundant copy that `DB::get`
//!   performs into a fresh `Vec<u8>`.
//! - [`DbMap::get_raw`] returns the raw value bytes as a
//!   [`bytes::Bytes`]. The `Bytes` is backed zero-copy by the
//!   RocksDB block cache where possible (cache hits) and by an
//!   internal copy where not (memtable hits, merge results,
//!   wide-column values). The handle co-owns the [`Db`] handle (via
//!   the reader) so it is sound to hold the `Bytes` past the borrow
//!   it was read through.
//!
//! Both methods have batched counterparts ([`DbMap::multi_get`] and
//! [`DbMap::multi_get_raw`]) that issue a single RocksDB
//! `batched_multi_get_cf` call rather than N independent reads.
//!
//! # A note on pinned reads
//!
//! Each outstanding `Bytes` clone of a block-cache-backed value pins
//! an LRU handle in RocksDB's block cache. Long-lived or unbounded
//! pins can drive `block_cache.pinned-usage` past
//! `block_cache.capacity()`. Hold the `Bytes` for as short a scope as
//! the application allows, especially on high-fanout query paths.

use std::marker::PhantomData;
use std::mem;
use std::ops::RangeBounds;
use std::sync::Arc;

use bytes::Bytes;
use rocksdb::DBPinnableSlice;

use crate::Decode;
use crate::Encode;
use crate::db::Db;
use crate::encode_buf::with_encode_buf;
use crate::error::Error;
use crate::error::OpenError;
use crate::iter::ByteBounds;
use crate::iter::Iter;
use crate::iter::RevIter;
use crate::iter::prefix_to_byte_bounds;
use crate::iter::range_to_byte_bounds;
use crate::reader::Reader;
use crate::snapshot::Snapshot;

/// A typed handle to a single column family on a [`Db`], bound to a
/// [`Reader`] that pins its consistency context.
///
/// Construct a live-tip handle with [`DbMap::new`], typically inside a
/// schema's [`Schema::open`](crate::Schema::open) implementation. Re-bind
/// at a snapshot via [`DbMap::at`].
///
/// # Examples
///
/// ```
/// use bytes::Buf;
/// use bytes::BufMut;
///
/// use sui_consistent_store::Db;
/// use sui_consistent_store::DbMap;
/// use sui_consistent_store::DbOptions;
/// use sui_consistent_store::Decode;
/// use sui_consistent_store::Encode;
/// use sui_consistent_store::Reader;
/// use sui_consistent_store::Schema;
/// use sui_consistent_store::error::DecodeError;
/// use sui_consistent_store::error::EncodeError;
/// use sui_consistent_store::error::OpenError;
///
/// #[derive(Debug, PartialEq, Eq)]
/// struct U64Be(u64);
///
/// impl Encode for U64Be {
///     fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
///         buf.put_slice(&self.0.to_be_bytes());
///         Ok(())
///     }
/// }
///
/// impl Decode for U64Be {
///     fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
///         if buf.remaining() != 8 {
///             return Err(DecodeError::msg("expected 8 bytes"));
///         }
///         Ok(Self(buf.get_u64()))
///     }
/// }
///
/// struct MySchema<R: Reader = Db> {
///     items: DbMap<U64Be, U64Be, R>,
/// }
///
/// impl Schema for MySchema {
///     fn cfs(opts: &sui_consistent_store::CfOptionsResolver) -> Vec<sui_consistent_store::CfDescriptor> {
///         vec![sui_consistent_store::CfDescriptor::new("items", opts.options("items"))]
///     }
///
///     fn open(db: &Db) -> Result<Self, OpenError> {
///         Ok(Self {
///             items: DbMap::new(db.clone(), "items")?,
///         })
///     }
/// }
///
/// let dir = tempfile::tempdir().unwrap();
/// let (_db, schema) = Db::open::<MySchema>(dir.path(), DbOptions::default()).unwrap();
/// // Nothing was inserted, so a lookup returns None.
/// assert!(schema.items.get(&U64Be(1)).unwrap().is_none());
/// ```
#[derive(Debug)]
pub struct DbMap<K, V, R: Reader = Db> {
    reader: R,
    cf_name: &'static str,
    _data: PhantomData<fn(K) -> V>,
}

/// Owner struct used to tie a `DBPinnableSlice` to a [`Db`] handle
/// so the slice can be wrapped in `bytes::Bytes::from_owner`. The
/// `'static` lifetime on the slice is justified by the [`Db`]
/// co-owner; see `pinned_to_bytes` for the safety argument.
///
/// Field declaration order is load-bearing: `slice` must drop before
/// `_db` so the cleanup function the pin runs on `Drop` (typically a
/// block-cache `Cache::Release`) executes while the underlying DB
/// allocation is still alive.
struct PinnedOwner {
    slice: DBPinnableSlice<'static>,
    _db: Db,
}

impl<K, V, R: Reader> DbMap<K, V, R> {
    /// Like [`DbMap::new`], but skips the existence check on
    /// `cf_name`. Crate-internal; used to construct typed handles
    /// against column families known to be registered (auto-created
    /// by [`Db::open`]), avoiding a redundant
    /// [`cf_handle`](Db::cf_handle) lookup per construction.
    pub(crate) fn new_unchecked(reader: R, cf_name: &'static str) -> Self {
        Self {
            reader,
            cf_name,
            _data: PhantomData,
        }
    }

    /// Construct a typed handle for the column family named `cf_name`
    /// on `reader`'s database.
    ///
    /// `reader` controls the consistency context. Common cases:
    ///
    /// - `DbMap::new(db, "items")` — owned [`Db`] (live-tip,
    ///   one `Arc` bump).
    /// - `DbMap::new(&db, "items")` — borrowed [`Db`] (live-tip,
    ///   no `Arc` bump; the returned handle borrows `db`).
    /// - `DbMap::new(snap, "items")` — owned [`Snapshot`]
    ///   (snapshot-bound). Usually obtained via
    ///   [`DbMap::at`] from an existing handle so the cross-`Db`
    ///   safety check fires.
    /// - `DbMap::new(&snap, "items")` — borrowed
    ///   [`Snapshot`] (snapshot-bound, no `Arc` bump).
    ///
    /// Returns an [`OpenError`] if the named column family is not
    /// registered on `reader.db()`. Construction is the only place
    /// where a missing column family is reported as an open-time
    /// error; subsequent operations report it as
    /// [`crate::error::Error::MissingColumnFamily`].
    pub fn new(reader: R, cf_name: &'static str) -> Result<Self, OpenError> {
        if reader.db().cf_handle(cf_name).is_none() {
            return Err(OpenError::msg(format!(
                "column family `{cf_name}` is not registered",
            )));
        }
        Ok(Self {
            reader,
            cf_name,
            _data: PhantomData,
        })
    }

    /// Re-bind this handle at a captured snapshot.
    ///
    /// Returns a new [`DbMap`] whose reader is
    /// [`Snapshot`], so subsequent reads see the
    /// database state at the snapshot's checkpoint regardless of
    /// writes that occurred after
    /// [`Db::take_snapshot`](crate::Db::take_snapshot) was called.
    /// The returned handle owns a clone of `snap` (two `Arc`
    /// bumps), so it is self-contained and can outlive the
    /// originating `Snapshot`. The column-family name is a
    /// [`&'static str`](prim@str), so re-binding does not allocate.
    ///
    /// # Panics
    ///
    /// Panics if `snap` was taken on a different [`Db`] than this
    /// handle is bound to. Cross-`Db` re-binding is a programmer
    /// error: the snapshot's column family is unrelated to this
    /// handle's, and silently reading from the wrong CF (or hitting
    /// a `MissingColumnFamily` error one read later) would mask the
    /// underlying bug.
    pub fn at(&self, snap: &Snapshot) -> DbMap<K, V, Snapshot> {
        assert!(
            self.reader.db().ptr_eq(snap.db()),
            "snapshot was taken on a different Db than this DbMap is bound to",
        );
        DbMap {
            reader: snap.clone(),
            cf_name: self.cf_name,
            _data: PhantomData,
        }
    }

    /// Borrowed counterpart to [`DbMap::at`]: re-bind this handle
    /// at a borrowed [`Snapshot`] rather than cloning it.
    ///
    /// Returns a new [`DbMap`] whose reader is `&'a Snapshot`,
    /// tied to the lifetime of `snap`. The snapshot's two `Arc`s
    /// are not bumped, and the column-family name is a
    /// [`&'static str`](prim@str) (no allocation), so the re-bind
    /// is essentially free.
    ///
    /// # Panics
    ///
    /// Panics if `snap` was taken on a different [`Db`] than this
    /// handle is bound to; see [`DbMap::at`] for the rationale.
    pub fn at_ref<'a>(&self, snap: &'a Snapshot) -> DbMap<K, V, &'a Snapshot> {
        assert!(
            self.reader.db().ptr_eq(snap.db()),
            "snapshot was taken on a different Db than this DbMap is bound to",
        );
        DbMap {
            reader: snap,
            cf_name: self.cf_name,
            _data: PhantomData,
        }
    }

    fn cf(&self) -> Result<Arc<rocksdb::BoundColumnFamily<'_>>, Error> {
        self.reader
            .db()
            .cf_handle(self.cf_name)
            .ok_or_else(|| Error::MissingColumnFamily(self.cf_name.to_string()))
    }

    /// The shared [`Db`] handle this `DbMap` is bound to. Used by
    /// `Batch` to look up the same column family the handle points
    /// at.
    pub(crate) fn db(&self) -> &Db {
        self.reader.db()
    }

    /// The name of the column family this handle is bound to.
    pub(crate) fn cf_name(&self) -> &'static str {
        self.cf_name
    }
}

impl<K, V, R: Reader> DbMap<K, V, R>
where
    K: Encode,
    V: Decode,
{
    /// Read and decode the value for `key`.
    ///
    /// Returns `Ok(None)` if the key is not present; otherwise
    /// returns the decoded value. Internally uses RocksDB's pinned
    /// read path so that block-cache hits avoid the extra heap
    /// allocation that `DB::get` would do.
    pub fn get(&self, key: &K) -> Result<Option<V>, Error> {
        let opts = self.reader.read_options();
        let cf = self.cf()?;
        with_encode_buf(|buf| {
            key.encode_into(buf)?;
            let pinned =
                self.reader
                    .db()
                    .rocksdb()
                    .get_pinned_cf_opt(&cf, buf.as_slice(), &opts)?;
            match pinned {
                Some(slice) => Ok(Some(V::decode(&mut &slice[..])?)),
                None => Ok(None),
            }
        })
    }

    /// Batched counterpart to [`get`](Self::get).
    ///
    /// Issues one RocksDB `batched_multi_get_cf` for all the keys
    /// rather than `keys.len()` separate reads. The outer `Result`
    /// captures failures in the encoding step (which abort the whole
    /// batch); each inner `Result` captures per-key read or decode
    /// failures and is reported alongside successful results in the
    /// returned vector.
    pub fn multi_get<'k, I>(&self, keys: I) -> Result<Vec<Result<Option<V>, Error>>, Error>
    where
        I: IntoIterator<Item = &'k K>,
        K: 'k,
    {
        Ok(self
            .multi_get_pinned(keys)?
            .into_iter()
            .map(|r| match r {
                Ok(Some(slice)) => V::decode(&mut &slice[..]).map(Some).map_err(Error::Decode),
                Ok(None) => Ok(None),
                Err(e) => Err(Error::Rocksdb(e)),
            })
            .collect())
    }
}

impl<K, V, R: Reader> DbMap<K, V, R>
where
    K: Encode,
{
    /// Read the raw bytes for `key`.
    ///
    /// Returns `Ok(None)` if the key is not present. The returned
    /// [`Bytes`] is backed zero-copy by the RocksDB block cache when
    /// the read hits a cached block; otherwise it backs onto a small
    /// internal copy made by RocksDB's pinned-read machinery. Either
    /// way, the `Bytes` co-owns the underlying [`Db`] handle so it
    /// can outlive any borrow this method was called through.
    pub fn get_raw(&self, key: &K) -> Result<Option<Bytes>, Error> {
        let opts = self.reader.read_options();
        let cf = self.cf()?;
        with_encode_buf(|buf| {
            key.encode_into(buf)?;
            let pinned =
                self.reader
                    .db()
                    .rocksdb()
                    .get_pinned_cf_opt(&cf, buf.as_slice(), &opts)?;
            Ok(pinned.map(|slice| pinned_to_bytes(self.reader.db().clone(), slice)))
        })
    }

    /// Batched counterpart to [`get_raw`](Self::get_raw).
    ///
    /// Issues one RocksDB `batched_multi_get_cf` for all the keys.
    /// The outer `Result` captures encoding failures; the inner
    /// `Result` captures per-key read failures.
    pub fn multi_get_raw<'k, I>(&self, keys: I) -> Result<Vec<Result<Option<Bytes>, Error>>, Error>
    where
        I: IntoIterator<Item = &'k K>,
        K: 'k,
    {
        Ok(self
            .multi_get_pinned(keys)?
            .into_iter()
            .map(|r| match r {
                Ok(Some(slice)) => Ok(Some(pinned_to_bytes(self.reader.db().clone(), slice))),
                Ok(None) => Ok(None),
                Err(e) => Err(Error::Rocksdb(e)),
            })
            .collect())
    }

    /// Internal helper: encode `keys` and issue a single RocksDB
    /// `batched_multi_get_cf`, returning the raw pinned-slice
    /// results. Both [`multi_get`](Self::multi_get) and
    /// [`multi_get_raw`](Self::multi_get_raw) funnel through here
    /// and just differ in how they map each result.
    fn multi_get_pinned<'k, 's, I>(
        &'s self,
        keys: I,
    ) -> Result<Vec<Result<Option<DBPinnableSlice<'s>>, rocksdb::Error>>, Error>
    where
        I: IntoIterator<Item = &'k K>,
        K: 'k,
    {
        let keys: Vec<&K> = keys.into_iter().collect();
        let opts = self.reader.read_options();
        let cf = self.cf()?;

        with_encode_buf(|buf| {
            let mut offsets = Vec::with_capacity(keys.len() + 1);
            offsets.push(0usize);
            for key in &keys {
                key.encode_into(buf)?;
                offsets.push(buf.len());
            }

            let bytes = buf.as_slice();
            let key_slices: Vec<&[u8]> = offsets
                .windows(2)
                .map(|window| &bytes[window[0]..window[1]])
                .collect();

            Ok(self
                .reader
                .db()
                .rocksdb()
                .batched_multi_get_cf_opt(&cf, key_slices, false, &opts))
        })
    }

    /// Test whether `key` is present in the column family.
    ///
    /// Returns `Ok(true)` if a value exists at `key`, `Ok(false)`
    /// otherwise. The implementation calls RocksDB's
    /// `key_may_exist_cf` first; when that returns `false` (a
    /// definitive negative answer), the call returns `Ok(false)`
    /// without a pinned read. Otherwise it confirms with a pinned
    /// `get_pinned_cf`.
    ///
    /// `key_may_exist_cf` is only useful when the column family has
    /// a bloom filter configured on its
    /// [`rocksdb::BlockBasedOptions`]; otherwise it always returns
    /// `true` and `contains_key` does the same work as `get_raw`
    /// plus an extra check. Schemas on hot
    /// `contains_key` paths should set
    /// [`set_bloom_filter`](rocksdb::BlockBasedOptions::set_bloom_filter)
    /// on the CF's options.
    pub fn contains_key(&self, key: &K) -> Result<bool, Error> {
        let opts = self.reader.read_options();
        let cf = self.cf()?;
        with_encode_buf(|buf| -> Result<bool, Error> {
            key.encode_into(buf)?;
            // Bloom-filter fast path. A `false` here is definitive;
            // a `true` is "may exist" and needs confirming.
            if !self
                .reader
                .db()
                .rocksdb()
                .key_may_exist_cf_opt(&cf, buf.as_slice(), &opts)
            {
                return Ok(false);
            }
            let pinned =
                self.reader
                    .db()
                    .rocksdb()
                    .get_pinned_cf_opt(&cf, buf.as_slice(), &opts)?;
            Ok(pinned.is_some())
        })
    }

    /// Batched counterpart to [`contains_key`](Self::contains_key).
    ///
    /// Issues one RocksDB `batched_multi_get_cf` for all the keys.
    /// Per-key encoding errors abort the entire batch; per-key read
    /// failures surface as the outer `Err`.
    pub fn multi_contains_keys<'k, I>(&self, keys: I) -> Result<Vec<bool>, Error>
    where
        I: IntoIterator<Item = &'k K>,
        K: 'k,
    {
        let raw = self.multi_get_raw(keys)?;
        raw.into_iter()
            .map(|r| r.map(|opt| opt.is_some()))
            .collect()
    }
}

impl<K, V, R: Reader> DbMap<K, V, R>
where
    K: Encode + Decode,
    V: Decode,
{
    /// Iterate forward over the subset of entries whose keys fall
    /// within `range`, in lexicographic key order.
    ///
    /// Pass `..` for an unbounded scan, `start..end` for a half-open
    /// range, `start..=end` for inclusive on both ends, or any other
    /// [`RangeBounds<K>`] combination. To bound by a different type
    /// (for example, a prefix-typed bound on a compound key), use
    /// [`iter_with`](Self::iter_with). Decode failures are reported
    /// as a per-item `Err`; the iterator stops yielding after the
    /// first error.
    pub fn iter(&self, range: impl RangeBounds<K>) -> Result<Iter<'_, K, V>, Error> {
        self.iter_forward(range_to_byte_bounds(&range)?)
    }

    /// Iterate forward over the subset of entries whose keys, when
    /// encoded, begin with `prefix`'s encoding.
    ///
    /// The prefix may be a different type than the full key, so long
    /// as the schema's encoding choice makes the prefix's encoded
    /// bytes a real byte prefix of every full-key encoding it should
    /// match. The crate documents this contract but does not verify
    /// it; see the [module-level docs](crate::iter) for an
    /// explanation and a worked example.
    pub fn iter_prefix(&self, prefix: &impl Encode) -> Result<Iter<'_, K, V>, Error> {
        self.iter_forward(prefix_to_byte_bounds(prefix)?)
    }

    /// Iterate in reverse over the subset of entries whose keys fall
    /// within `range`.
    pub fn iter_rev(&self, range: impl RangeBounds<K>) -> Result<RevIter<'_, K, V>, Error> {
        self.iter_reverse(range_to_byte_bounds(&range)?)
    }

    /// Iterate in reverse over the subset of entries whose keys,
    /// when encoded, begin with `prefix`'s encoding.
    pub fn iter_rev_prefix(&self, prefix: &impl Encode) -> Result<RevIter<'_, K, V>, Error> {
        self.iter_reverse(prefix_to_byte_bounds(prefix)?)
    }

    /// Iterate forward over entries whose keys, when encoded, fall
    /// within the encoded bytes of `range`'s `J`-typed bounds.
    ///
    /// Like [`iter`](Self::iter), but parameterized over an
    /// independent `J: Encode`. Useful when the schema's key type
    /// is a compound (e.g. `(owner, type, id)`) and the caller
    /// wants to bound the iteration by a prefix-typed value (e.g. an
    /// `owner`-typed range). The schema's encoding must make `J`'s
    /// encoded bytes well-ordered against `K`'s encoded keys; the
    /// crate documents this contract but does not verify it.
    ///
    /// Use turbofish to disambiguate `J` when passing an unbounded
    /// range: `map.iter_with::<MyPrefix>(..)`.
    pub fn iter_with<J: Encode>(
        &self,
        range: impl RangeBounds<J>,
    ) -> Result<Iter<'_, K, V>, Error> {
        self.iter_forward(range_to_byte_bounds(&range)?)
    }

    /// Reverse counterpart to [`iter_with`](Self::iter_with).
    pub fn iter_rev_with<J: Encode>(
        &self,
        range: impl RangeBounds<J>,
    ) -> Result<RevIter<'_, K, V>, Error> {
        self.iter_reverse(range_to_byte_bounds(&range)?)
    }

    /// Internal helper: forward iteration with caller-supplied byte
    /// bounds. Both range and prefix entry points funnel through
    /// here. A [`ByteBounds::Empty`] short-circuits to an empty
    /// iterator without touching RocksDB.
    ///
    /// `seek_to_first` honors `iterate_lower_bound` in modern
    /// RocksDB, landing at the smallest key at or above the bound
    /// — equivalent to `seek(lower)`. Calling `seek` explicitly
    /// would require cloning the bound (RocksDB's
    /// `set_iterate_lower_bound` consumes the `Vec<u8>`), so we
    /// use the bound-aware `seek_to_first`.
    fn iter_forward<'s>(&'s self, bounds: ByteBounds) -> Result<Iter<'s, K, V>, Error> {
        let ByteBounds::Range(lower, upper) = bounds else {
            return Ok(Iter::empty());
        };
        let mut opts = self.reader.read_options();
        if let Some(l) = lower {
            opts.set_iterate_lower_bound(l);
        }
        if let Some(u) = upper {
            opts.set_iterate_upper_bound(u);
        }
        let cf = self.cf()?;
        let mut raw = self.reader.db().rocksdb().raw_iterator_cf_opt(&cf, opts);
        raw.seek_to_first();
        Ok(Iter::new(raw))
    }

    /// Internal helper: reverse iteration with caller-supplied byte
    /// bounds. The dual of [`iter_forward`](Self::iter_forward).
    ///
    /// Symmetric to forward: `seek_to_last` honors
    /// `iterate_upper_bound` in modern RocksDB, landing at the
    /// largest key strictly less than the bound. Calling
    /// `seek_for_prev` explicitly would require cloning the bound,
    /// so we use the bound-aware `seek_to_last`.
    fn iter_reverse<'s>(&'s self, bounds: ByteBounds) -> Result<RevIter<'s, K, V>, Error> {
        let ByteBounds::Range(lower, upper) = bounds else {
            return Ok(RevIter::empty());
        };
        let mut opts = self.reader.read_options();
        if let Some(l) = lower {
            opts.set_iterate_lower_bound(l);
        }
        if let Some(u) = upper {
            opts.set_iterate_upper_bound(u);
        }
        let cf = self.cf()?;
        let mut raw = self.reader.db().rocksdb().raw_iterator_cf_opt(&cf, opts);
        raw.seek_to_last();
        Ok(RevIter::new(raw))
    }
}

impl AsRef<[u8]> for PinnedOwner {
    fn as_ref(&self) -> &[u8] {
        &self.slice
    }
}

/// Wrap a `DBPinnableSlice` plus its co-owned [`Db`] handle in a
/// `bytes::Bytes` so callers do not have to reason about RocksDB
/// lifetimes themselves.
///
/// The `'a` lifetime on `DBPinnableSlice<'a>` is a `PhantomData<&'a
/// DB>` annotation only. The actual backing memory is either a
/// reference-counted block in the RocksDB block cache (kept alive
/// until the slice's `Drop` runs the registered cleanup) or a copied
/// buffer owned by the C++ `PinnableSlice` itself (freed when the
/// slice drops). Neither path requires a live `&DB` borrow; both
/// require only that the underlying DB allocation outlive the slice,
/// which the co-owned [`Db`] handle guarantees. Drop order in
/// `PinnedOwner` (slice first, `Db` second) ensures the cleanup
/// runs before the [`Db`] handle's last reference goes away.
fn pinned_to_bytes(db: Db, slice: DBPinnableSlice<'_>) -> Bytes {
    // SAFETY: see the function-level doc comment. The lifetime is a
    // conservative phantom annotation; the actual memory pin is
    // sustained by the [`Db`] co-owner held in `PinnedOwner`.
    let slice: DBPinnableSlice<'static> = unsafe { mem::transmute(slice) };
    Bytes::from_owner(PinnedOwner { slice, _db: db })
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::DbOptions;
    use bytes::BufMut;

    use crate::Schema;
    use crate::Watermark;
    use crate::error::DecodeError;
    use crate::error::EncodeError;

    /// Hand-rolled big-endian `u64` for tests.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct U64Be(u64);

    impl Encode for U64Be {
        fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
            buf.put_slice(&self.0.to_be_bytes());
            Ok(())
        }
    }

    impl Decode for U64Be {
        fn decode<B: bytes::Buf>(buf: &mut B) -> Result<Self, DecodeError> {
            if buf.remaining() != 8 {
                return Err(DecodeError::msg("expected 8 bytes"));
            }
            Ok(Self(buf.get_u64()))
        }
    }

    #[derive(Debug)]
    struct TestSchema<R: Reader = Db> {
        items: DbMap<U64Be, U64Be, R>,
    }

    impl Schema for TestSchema {
        fn cfs(opts: &crate::options::CfOptionsResolver) -> Vec<crate::CfDescriptor> {
            vec![crate::CfDescriptor::new("items", opts.options("items"))]
        }

        fn open(db: &Db) -> Result<Self, OpenError> {
            Ok(Self {
                items: DbMap::new(db.clone(), "items")?,
            })
        }
    }

    /// Seed a `(key, value)` row through the underlying RocksDB so
    /// the read paths under test have something to find. Uses the
    /// `Encode` trait so the bytes match what `DbMap::get` will look
    /// for.
    fn seed(db: &Db, cf: &str, key: &U64Be, value: &U64Be) {
        let cf = db.cf_handle(cf).unwrap();
        let key_bytes = key.encode().unwrap();
        let value_bytes = value.encode().unwrap();
        db.rocksdb().put_cf(&cf, key_bytes, value_bytes).unwrap();
    }

    fn open() -> (TempDir, Db, TestSchema) {
        let dir = TempDir::new().unwrap();
        let (db, schema) = Db::open::<TestSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db, schema)
    }

    #[test]
    fn new_with_borrowed_db_errors_on_unknown_cf() {
        let (_dir, db, _schema) = open();
        let err = DbMap::<U64Be, U64Be, &Db>::new(&db, "missing").unwrap_err();
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn new_with_borrowed_db_reads_live_tip() {
        // Construct a &Db-bound DbMap against a borrowed Db and
        // verify reads see writes committed through the owned-Db
        // schema. Avoids the per-handle Arc bump that an owned
        // `DbMap::new(db.clone(), …)` pays.
        let (_dir, db, schema) = open();
        let mut batch = db.batch();
        batch.put(&schema.items, &U64Be(1), &U64Be(10)).unwrap();
        batch.commit().unwrap();

        let items_ref: DbMap<U64Be, U64Be, &Db> = DbMap::new(&db, "items").unwrap();
        assert_eq!(items_ref.get(&U64Be(1)).unwrap(), Some(U64Be(10)));
    }

    #[test]
    fn new_errors_on_unknown_cf() {
        let (_dir, db, _schema) = open();
        let err = DbMap::<U64Be, U64Be>::new(db, "missing").unwrap_err();
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn get_returns_none_for_missing_key() {
        let (_dir, _db, schema) = open();
        assert!(schema.items.get(&U64Be(42)).unwrap().is_none());
    }

    #[test]
    fn get_returns_decoded_value() {
        let (_dir, db, schema) = open();
        seed(&db, "items", &U64Be(7), &U64Be(700));
        assert_eq!(schema.items.get(&U64Be(7)).unwrap(), Some(U64Be(700)));
    }

    #[test]
    fn get_raw_returns_none_for_missing_key() {
        let (_dir, _db, schema) = open();
        assert!(schema.items.get_raw(&U64Be(42)).unwrap().is_none());
    }

    #[test]
    fn get_raw_returns_value_bytes() {
        let (_dir, db, schema) = open();
        seed(&db, "items", &U64Be(7), &U64Be(700));
        let bytes = schema.items.get_raw(&U64Be(7)).unwrap().unwrap();
        assert_eq!(&bytes[..], &700u64.to_be_bytes());
    }

    #[test]
    fn get_raw_bytes_outlive_schema_drop() {
        let (_dir, db, schema) = open();
        seed(&db, "items", &U64Be(11), &U64Be(1100));
        let bytes = schema.items.get_raw(&U64Be(11)).unwrap().unwrap();
        // Drop the schema (and its DbMap clone of `Db`); the
        // `Bytes` still co-owns the DB via `PinnedOwner`.
        drop(schema);
        assert_eq!(&bytes[..], &1100u64.to_be_bytes());
        // And `db` is still in scope, but the test passes whether or
        // not it is.
        drop(db);
    }

    #[test]
    fn multi_get_returns_decoded_values_per_key() {
        let (_dir, db, schema) = open();
        seed(&db, "items", &U64Be(1), &U64Be(10));
        seed(&db, "items", &U64Be(3), &U64Be(30));
        let keys = [U64Be(1), U64Be(2), U64Be(3)];
        let results = schema.items.multi_get(keys.iter()).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].as_ref().unwrap(), &Some(U64Be(10)));
        assert_eq!(results[1].as_ref().unwrap(), &None);
        assert_eq!(results[2].as_ref().unwrap(), &Some(U64Be(30)));
    }

    #[test]
    fn multi_get_raw_returns_bytes_per_key() {
        let (_dir, db, schema) = open();
        seed(&db, "items", &U64Be(5), &U64Be(50));
        let keys = [U64Be(5), U64Be(6)];
        let results = schema.items.multi_get_raw(keys.iter()).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(
            results[0].as_ref().unwrap().as_ref().unwrap()[..],
            50u64.to_be_bytes(),
        );
        assert!(results[1].as_ref().unwrap().is_none());
    }

    #[test]
    fn multi_get_handles_empty_input() {
        let (_dir, _db, schema) = open();
        let keys: [U64Be; 0] = [];
        let results = schema.items.multi_get(keys.iter()).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn contains_key_returns_true_for_existing() {
        let (_dir, db, schema) = open();
        seed(&db, "items", &U64Be(7), &U64Be(700));
        assert!(schema.items.contains_key(&U64Be(7)).unwrap());
    }

    #[test]
    fn contains_key_returns_false_for_missing() {
        let (_dir, _db, schema) = open();
        assert!(!schema.items.contains_key(&U64Be(99)).unwrap());
    }

    #[test]
    fn multi_contains_keys_reports_per_key() {
        let (_dir, db, schema) = open();
        seed(&db, "items", &U64Be(1), &U64Be(0));
        seed(&db, "items", &U64Be(3), &U64Be(0));
        let keys = [U64Be(1), U64Be(2), U64Be(3)];
        let results = schema.items.multi_contains_keys(keys.iter()).unwrap();
        assert_eq!(results, vec![true, false, true]);
    }

    #[test]
    fn iter_yields_all_entries_in_order() {
        let (_dir, db, schema) = open();
        seed(&db, "items", &U64Be(3), &U64Be(30));
        seed(&db, "items", &U64Be(1), &U64Be(10));
        seed(&db, "items", &U64Be(2), &U64Be(20));
        let collected: Vec<_> = schema.items.iter(..).unwrap().map(Result::unwrap).collect();
        assert_eq!(
            collected,
            vec![
                (U64Be(1), U64Be(10)),
                (U64Be(2), U64Be(20)),
                (U64Be(3), U64Be(30)),
            ],
        );
    }

    #[test]
    fn iter_on_empty_cf_yields_nothing() {
        let (_dir, _db, schema) = open();
        assert_eq!(schema.items.iter(..).unwrap().count(), 0);
    }

    #[test]
    fn iter_rev_yields_entries_in_reverse() {
        let (_dir, db, schema) = open();
        seed(&db, "items", &U64Be(1), &U64Be(10));
        seed(&db, "items", &U64Be(2), &U64Be(20));
        seed(&db, "items", &U64Be(3), &U64Be(30));
        let collected: Vec<_> = schema
            .items
            .iter_rev(..)
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(
            collected,
            vec![
                (U64Be(3), U64Be(30)),
                (U64Be(2), U64Be(20)),
                (U64Be(1), U64Be(10)),
            ],
        );
    }

    #[test]
    fn iter_rev_on_empty_cf_yields_nothing() {
        let (_dir, _db, schema) = open();
        assert_eq!(schema.items.iter_rev(..).unwrap().count(), 0);
    }

    #[test]
    fn iter_propagates_decode_errors_then_stops() {
        let (_dir, db, schema) = open();
        // Seed valid rows.
        seed(&db, "items", &U64Be(1), &U64Be(10));
        seed(&db, "items", &U64Be(2), &U64Be(20));
        // Insert a corrupt row directly under a key that sorts in
        // the middle: an 8-byte key that decodes fine, paired with a
        // 4-byte value that does not.
        let cf = db.cf_handle("items").unwrap();
        let key_bytes = U64Be(3).encode().unwrap();
        db.rocksdb().put_cf(&cf, key_bytes, [0u8; 4]).unwrap();
        seed(&db, "items", &U64Be(4), &U64Be(40));

        let mut iter = schema.items.iter(..).unwrap();
        assert_eq!(iter.next().unwrap().unwrap(), (U64Be(1), U64Be(10)));
        assert_eq!(iter.next().unwrap().unwrap(), (U64Be(2), U64Be(20)));
        assert!(matches!(iter.next(), Some(Err(Error::Decode(_)))));
        // Iterator must not yield further items after an error.
        assert!(iter.next().is_none());
    }

    #[test]
    fn iter_range_filters_by_typed_bounds() {
        let (_dir, db, schema) = open();
        for k in 1..=5 {
            seed(&db, "items", &U64Be(k), &U64Be(k * 10));
        }
        let collected: Vec<_> = schema
            .items
            .iter(U64Be(2)..U64Be(5))
            .unwrap()
            .map(|r| r.unwrap().0)
            .collect();
        assert_eq!(collected, vec![U64Be(2), U64Be(3), U64Be(4)]);
    }

    #[test]
    fn iter_range_inclusive_includes_end() {
        let (_dir, db, schema) = open();
        for k in 1..=5 {
            seed(&db, "items", &U64Be(k), &U64Be(0));
        }
        let collected: Vec<_> = schema
            .items
            .iter(U64Be(2)..=U64Be(4))
            .unwrap()
            .map(|r| r.unwrap().0)
            .collect();
        assert_eq!(collected, vec![U64Be(2), U64Be(3), U64Be(4)]);
    }

    #[test]
    fn iter_range_open_start_iterates_from_beginning() {
        let (_dir, db, schema) = open();
        for k in 1..=4 {
            seed(&db, "items", &U64Be(k), &U64Be(0));
        }
        let collected: Vec<_> = schema
            .items
            .iter(..U64Be(3))
            .unwrap()
            .map(|r| r.unwrap().0)
            .collect();
        assert_eq!(collected, vec![U64Be(1), U64Be(2)]);
    }

    #[test]
    fn iter_range_open_end_iterates_to_end() {
        let (_dir, db, schema) = open();
        for k in 1..=4 {
            seed(&db, "items", &U64Be(k), &U64Be(0));
        }
        let collected: Vec<_> = schema
            .items
            .iter(U64Be(3)..)
            .unwrap()
            .map(|r| r.unwrap().0)
            .collect();
        assert_eq!(collected, vec![U64Be(3), U64Be(4)]);
    }

    #[test]
    #[should_panic(expected = "snapshot was taken on a different Db")]
    fn at_panics_when_snapshot_is_from_a_different_db() {
        let (_dir_a, db_a, schema_a) = open();
        let (_dir_b, db_b, _schema_b) = open();
        // Snapshot is taken on db_b, but we re-bind a DbMap from db_a.
        // Should panic to surface the misuse.
        db_b.take_snapshot(Watermark::for_checkpoint(1));
        let snap_b = db_b.at_snapshot(1).unwrap();
        // db_a in scope so the assert message is meaningful, even
        // though we don't directly use it.
        let _ = &db_a;
        let _ = schema_a.items.at(&snap_b);
    }

    /// Forward-iter boundary sweep, mirroring
    /// [`open_with_odd_keys`] / `collect_rev` for the forward
    /// direction. The data is keys 1, 3, 5, 7, 9 so each bound can
    /// be probed for exact vs inexact match.
    fn collect_fwd<R: std::ops::RangeBounds<U64Be>>(
        schema: &TestSchema,
        range: R,
    ) -> Vec<(U64Be, U64Be)> {
        schema
            .items
            .iter(range)
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn iter_inclusive_lowerbound_exact_match() {
        let (_dir, _db, schema) = open_with_odd_keys();
        // 5.. yields 5, 7, 9.
        assert_eq!(
            collect_fwd(&schema, U64Be(5)..),
            vec![
                (U64Be(5), U64Be(50)),
                (U64Be(7), U64Be(70)),
                (U64Be(9), U64Be(90)),
            ],
        );
    }

    #[test]
    fn iter_inclusive_lowerbound_inexact_match() {
        let (_dir, _db, schema) = open_with_odd_keys();
        // 4.. on data without 4: same as 5..
        assert_eq!(
            collect_fwd(&schema, U64Be(4)..),
            vec![
                (U64Be(5), U64Be(50)),
                (U64Be(7), U64Be(70)),
                (U64Be(9), U64Be(90)),
            ],
        );
    }

    #[test]
    fn iter_exclusive_lowerbound_exact_match() {
        use std::ops::Bound::{Excluded, Unbounded};
        let (_dir, _db, schema) = open_with_odd_keys();
        // (Excluded(5), Unbounded) excludes 5 itself.
        assert_eq!(
            collect_fwd(&schema, (Excluded(U64Be(5)), Unbounded)),
            vec![(U64Be(7), U64Be(70)), (U64Be(9), U64Be(90))],
        );
    }

    #[test]
    fn iter_exclusive_lowerbound_inexact_match() {
        use std::ops::Bound::{Excluded, Unbounded};
        let (_dir, _db, schema) = open_with_odd_keys();
        // (Excluded(4), Unbounded): 4 is not in data, so equivalent to 5..
        assert_eq!(
            collect_fwd(&schema, (Excluded(U64Be(4)), Unbounded)),
            vec![
                (U64Be(5), U64Be(50)),
                (U64Be(7), U64Be(70)),
                (U64Be(9), U64Be(90)),
            ],
        );
    }

    #[test]
    fn iter_lowerbound_past_data_yields_nothing() {
        let (_dir, _db, schema) = open_with_odd_keys();
        assert!(collect_fwd(&schema, U64Be(100)..).is_empty());
    }

    #[test]
    fn iter_inclusive_upperbound_exact_match() {
        let (_dir, _db, schema) = open_with_odd_keys();
        // ..=5 on data including 5: yields 1, 3, 5.
        assert_eq!(
            collect_fwd(&schema, ..=U64Be(5)),
            vec![
                (U64Be(1), U64Be(10)),
                (U64Be(3), U64Be(30)),
                (U64Be(5), U64Be(50)),
            ],
        );
    }

    #[test]
    fn iter_inclusive_upperbound_inexact_match() {
        let (_dir, _db, schema) = open_with_odd_keys();
        // ..=6 on data without 6: same as ..=5.
        assert_eq!(
            collect_fwd(&schema, ..=U64Be(6)),
            vec![
                (U64Be(1), U64Be(10)),
                (U64Be(3), U64Be(30)),
                (U64Be(5), U64Be(50)),
            ],
        );
    }

    #[test]
    fn iter_exclusive_upperbound_exact_match() {
        let (_dir, _db, schema) = open_with_odd_keys();
        // ..5 excludes 5 itself.
        assert_eq!(
            collect_fwd(&schema, ..U64Be(5)),
            vec![(U64Be(1), U64Be(10)), (U64Be(3), U64Be(30))],
        );
    }

    #[test]
    fn iter_exclusive_upperbound_inexact_match() {
        let (_dir, _db, schema) = open_with_odd_keys();
        // ..6 on data without 6: same as ..=5.
        assert_eq!(
            collect_fwd(&schema, ..U64Be(6)),
            vec![
                (U64Be(1), U64Be(10)),
                (U64Be(3), U64Be(30)),
                (U64Be(5), U64Be(50)),
            ],
        );
    }

    #[test]
    fn iter_vacuous_exclusive_upperbound_yields_nothing() {
        let (_dir, _db, schema) = open_with_odd_keys();
        // ..0 has no key strictly less than 0 by encoding.
        assert!(collect_fwd(&schema, ..U64Be(0)).is_empty());
    }

    #[test]
    fn iter_single_point_inclusive_range() {
        let (_dir, _db, schema) = open_with_odd_keys();
        // 5..=5 yields exactly 5.
        assert_eq!(
            collect_fwd(&schema, U64Be(5)..=U64Be(5)),
            vec![(U64Be(5), U64Be(50))],
        );
    }

    #[test]
    fn iter_non_overlapping_range_yields_nothing() {
        let (_dir, _db, schema) = open_with_odd_keys();
        assert!(collect_fwd(&schema, U64Be(100)..=U64Be(200)).is_empty());
    }

    #[test]
    fn iter_with_keys_far_past_upperbound() {
        // Forward symmetric to iter_rev_with_keys_far_past_upperbound.
        let (_dir, db, schema) = open();
        for k in 1..=1000 {
            seed(&db, "items", &U64Be(k), &U64Be(k));
        }
        let collected: Vec<_> = schema
            .items
            .iter(..=U64Be(5))
            .unwrap()
            .map(|r| r.unwrap().0)
            .collect();
        assert_eq!(
            collected,
            vec![U64Be(1), U64Be(2), U64Be(3), U64Be(4), U64Be(5)],
        );
    }

    #[test]
    fn iter_with_excluded_max_lower_bound_yields_nothing() {
        let (_dir, db, schema) = open();
        for k in 1..=5 {
            seed(&db, "items", &U64Be(k), &U64Be(0));
        }
        // (Excluded(MAX), Unbounded): no key satisfies > MAX. Now
        // short-circuits to an empty iterator instead of silently
        // widening to "no lower bound."
        let collected: Vec<_> = schema
            .items
            .iter((
                std::ops::Bound::Excluded(U64Be(u64::MAX)),
                std::ops::Bound::Unbounded,
            ))
            .unwrap()
            .map(|r| r.unwrap().0)
            .collect();
        assert!(collected.is_empty());
    }

    #[test]
    fn iter_rev_range_filters_in_reverse() {
        let (_dir, db, schema) = open();
        for k in 1..=5 {
            seed(&db, "items", &U64Be(k), &U64Be(0));
        }
        let collected: Vec<_> = schema
            .items
            .iter_rev(U64Be(2)..=U64Be(4))
            .unwrap()
            .map(|r| r.unwrap().0)
            .collect();
        assert_eq!(collected, vec![U64Be(4), U64Be(3), U64Be(2)]);
    }

    // Boundary-case sweep tests below use a small fixed dataset
    // (odd keys 1,3,5,7,9) so each bound can be probed for "exact
    // match" (bound is in the data) vs "inexact match" (bound falls
    // between keys). Mirrors alt-consistent-store's coverage.
    fn open_with_odd_keys() -> (TempDir, Db, TestSchema) {
        let (dir, db, schema) = open();
        for k in (1..=9u64).step_by(2) {
            seed(&db, "items", &U64Be(k), &U64Be(k * 10));
        }
        (dir, db, schema)
    }

    fn collect_rev<R: std::ops::RangeBounds<U64Be>>(
        schema: &TestSchema,
        range: R,
    ) -> Vec<(U64Be, U64Be)> {
        schema
            .items
            .iter_rev(range)
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn iter_rev_inclusive_lowerbound_exact_match() {
        let (_dir, _db, schema) = open_with_odd_keys();
        // 5..  includes 5, 7, 9 in reverse.
        assert_eq!(
            collect_rev(&schema, U64Be(5)..),
            vec![
                (U64Be(9), U64Be(90)),
                (U64Be(7), U64Be(70)),
                (U64Be(5), U64Be(50)),
            ],
        );
    }

    #[test]
    fn iter_rev_inclusive_lowerbound_inexact_match() {
        let (_dir, _db, schema) = open_with_odd_keys();
        // 4.. on data without 4: same as 5..
        assert_eq!(
            collect_rev(&schema, U64Be(4)..),
            vec![
                (U64Be(9), U64Be(90)),
                (U64Be(7), U64Be(70)),
                (U64Be(5), U64Be(50)),
            ],
        );
    }

    #[test]
    fn iter_rev_exclusive_lowerbound_exact_match() {
        use std::ops::Bound::{Excluded, Unbounded};
        let (_dir, _db, schema) = open_with_odd_keys();
        // (Excluded(5), Unbounded) excludes 5 itself.
        assert_eq!(
            collect_rev(&schema, (Excluded(U64Be(5)), Unbounded)),
            vec![(U64Be(9), U64Be(90)), (U64Be(7), U64Be(70))],
        );
    }

    #[test]
    fn iter_rev_exclusive_lowerbound_inexact_match() {
        use std::ops::Bound::{Excluded, Unbounded};
        let (_dir, _db, schema) = open_with_odd_keys();
        // (Excluded(4), Unbounded): 4 is not in data, so equivalent to 5..
        assert_eq!(
            collect_rev(&schema, (Excluded(U64Be(4)), Unbounded)),
            vec![
                (U64Be(9), U64Be(90)),
                (U64Be(7), U64Be(70)),
                (U64Be(5), U64Be(50)),
            ],
        );
    }

    #[test]
    fn iter_rev_lowerbound_past_data_yields_nothing() {
        let (_dir, _db, schema) = open_with_odd_keys();
        assert!(collect_rev(&schema, U64Be(100)..).is_empty());
    }

    #[test]
    fn iter_rev_excluded_max_lowerbound_yields_nothing() {
        use std::ops::Bound::{Excluded, Unbounded};
        let (_dir, _db, schema) = open_with_odd_keys();
        // Symmetric to the forward-iter version: provably empty.
        assert!(collect_rev(&schema, (Excluded(U64Be(u64::MAX)), Unbounded)).is_empty());
    }

    #[test]
    fn iter_rev_inclusive_upperbound_exact_match() {
        let (_dir, _db, schema) = open_with_odd_keys();
        // ..=5 on data including 5: yields 5, 3, 1.
        assert_eq!(
            collect_rev(&schema, ..=U64Be(5)),
            vec![
                (U64Be(5), U64Be(50)),
                (U64Be(3), U64Be(30)),
                (U64Be(1), U64Be(10)),
            ],
        );
    }

    #[test]
    fn iter_rev_inclusive_upperbound_inexact_match() {
        let (_dir, _db, schema) = open_with_odd_keys();
        // ..=6 on data without 6: same as ..=5.
        assert_eq!(
            collect_rev(&schema, ..=U64Be(6)),
            vec![
                (U64Be(5), U64Be(50)),
                (U64Be(3), U64Be(30)),
                (U64Be(1), U64Be(10)),
            ],
        );
    }

    #[test]
    fn iter_rev_exclusive_upperbound_exact_match() {
        let (_dir, _db, schema) = open_with_odd_keys();
        // ..5 excludes 5 itself.
        assert_eq!(
            collect_rev(&schema, ..U64Be(5)),
            vec![(U64Be(3), U64Be(30)), (U64Be(1), U64Be(10))],
        );
    }

    #[test]
    fn iter_rev_exclusive_upperbound_inexact_match() {
        let (_dir, _db, schema) = open_with_odd_keys();
        // ..6 on data without 6: same as ..=5.
        assert_eq!(
            collect_rev(&schema, ..U64Be(6)),
            vec![
                (U64Be(5), U64Be(50)),
                (U64Be(3), U64Be(30)),
                (U64Be(1), U64Be(10)),
            ],
        );
    }

    #[test]
    fn iter_rev_vacuous_exclusive_upperbound_yields_nothing() {
        let (_dir, _db, schema) = open_with_odd_keys();
        // ..0 has no key strictly less than 0 by encoding.
        assert!(collect_rev(&schema, ..U64Be(0)).is_empty());
    }

    #[test]
    fn iter_rev_with_keys_far_past_upperbound() {
        // Stress-tests seek_for_prev / iterate_upper_bound: data has
        // many keys past the upper bound; iteration must not start at
        // the literal last key.
        let (_dir, db, schema) = open();
        for k in 1..=1000 {
            seed(&db, "items", &U64Be(k), &U64Be(k));
        }
        let collected: Vec<_> = schema
            .items
            .iter_rev(..=U64Be(5))
            .unwrap()
            .map(|r| r.unwrap().0)
            .collect();
        assert_eq!(
            collected,
            vec![U64Be(5), U64Be(4), U64Be(3), U64Be(2), U64Be(1)],
        );
    }

    #[test]
    fn iter_rev_single_point_inclusive_range() {
        let (_dir, _db, schema) = open_with_odd_keys();
        // 5..=5 yields exactly 5.
        assert_eq!(
            collect_rev(&schema, U64Be(5)..=U64Be(5)),
            vec![(U64Be(5), U64Be(50))],
        );
    }

    #[test]
    fn iter_rev_non_overlapping_range_yields_nothing() {
        let (_dir, _db, schema) = open_with_odd_keys();
        // Data is 1..10 odds; 100..200 doesn't overlap.
        assert!(collect_rev(&schema, U64Be(100)..=U64Be(200)).is_empty());
    }

    #[test]
    fn iter_cursor_methods_expose_raw_state() {
        let (_dir, db, schema) = open();
        seed(&db, "items", &U64Be(1), &U64Be(10));
        seed(&db, "items", &U64Be(2), &U64Be(20));
        let mut iter = schema.items.iter(..).unwrap();
        assert!(iter.valid());
        assert_eq!(iter.raw_key().unwrap(), &1u64.to_be_bytes());
        assert_eq!(iter.raw_value().unwrap(), &10u64.to_be_bytes());
        // Step via the typed Iterator interface.
        let _ = iter.next();
        assert!(iter.valid());
        assert_eq!(iter.raw_key().unwrap(), &2u64.to_be_bytes());
    }

    #[test]
    fn iter_seek_repositions_cursor_to_byte_target() {
        let (_dir, db, schema) = open();
        seed(&db, "items", &U64Be(1), &U64Be(10));
        seed(&db, "items", &U64Be(5), &U64Be(50));
        seed(&db, "items", &U64Be(9), &U64Be(90));
        let mut iter = schema.items.iter(..).unwrap();
        // Seek to bytes for U64Be(5); next() should yield 5 then 9.
        iter.seek(5u64.to_be_bytes());
        let collected: Vec<_> = (&mut iter).map(|r| r.unwrap().0).collect();
        assert_eq!(collected, vec![U64Be(5), U64Be(9)]);
    }

    #[test]
    fn iter_skip_past_advances_past_prefix() {
        let (_dir, db, schema) = open();
        seed(&db, "items", &U64Be(1), &U64Be(10));
        seed(&db, "items", &U64Be(5), &U64Be(50));
        let mut iter = schema.items.iter(..).unwrap();
        iter.skip_past(1u64.to_be_bytes());
        // After skipping past key 1, only 5 remains.
        let collected: Vec<_> = (&mut iter).map(|r| r.unwrap().0).collect();
        assert_eq!(collected, vec![U64Be(5)]);
    }

    #[test]
    fn iter_seek_inside_bounded_range_respects_lower_bound() {
        // Mirrors alt's "underflow" case: with a bounded forward
        // iter, seek to a key below the lower bound should land on
        // (or after) the lower bound, not below it.
        let (_dir, db, schema) = open();
        for k in 0..=10u64 {
            seed(&db, "items", &U64Be(k), &U64Be(k));
        }
        let mut iter = schema.items.iter(U64Be(4)..U64Be(8)).unwrap();
        iter.seek(1u64.to_be_bytes());
        let (k, _) = iter.next().unwrap().unwrap();
        assert_eq!(k, U64Be(4));
    }

    #[test]
    fn iter_seek_past_upper_bound_yields_nothing() {
        // Mirrors alt's "overflow" case.
        let (_dir, db, schema) = open();
        for k in 0..=10u64 {
            seed(&db, "items", &U64Be(k), &U64Be(k));
        }
        let mut iter = schema.items.iter(U64Be(4)..U64Be(8)).unwrap();
        iter.seek(8u64.to_be_bytes());
        assert!(iter.next().is_none());
    }

    #[test]
    fn iter_rev_cursor_methods_expose_raw_state() {
        let (_dir, db, schema) = open();
        seed(&db, "items", &U64Be(1), &U64Be(10));
        seed(&db, "items", &U64Be(2), &U64Be(20));
        let mut iter = schema.items.iter_rev(..).unwrap();
        assert!(iter.valid());
        assert_eq!(iter.raw_key().unwrap(), &2u64.to_be_bytes());
        assert_eq!(iter.raw_value().unwrap(), &20u64.to_be_bytes());
        let _ = iter.next();
        assert!(iter.valid());
        assert_eq!(iter.raw_key().unwrap(), &1u64.to_be_bytes());
    }

    #[test]
    fn iter_rev_seek_repositions_to_largest_key_at_or_below() {
        let (_dir, _db, schema) = open_with_odd_keys();
        // Seek to 6 (not in data); should land on 5.
        let mut iter = schema.items.iter_rev(..).unwrap();
        iter.seek(6u64.to_be_bytes());
        let collected: Vec<_> = (&mut iter).map(|r| r.unwrap().0).collect();
        assert_eq!(collected, vec![U64Be(5), U64Be(3), U64Be(1)]);
    }

    #[test]
    fn iter_rev_seek_to_exact_key_yields_that_key_first() {
        let (_dir, _db, schema) = open_with_odd_keys();
        let mut iter = schema.items.iter_rev(..).unwrap();
        iter.seek(5u64.to_be_bytes());
        let (k, _) = iter.next().unwrap().unwrap();
        assert_eq!(k, U64Be(5));
    }

    #[test]
    fn iter_rev_skip_past_advances_past_prefix() {
        let (_dir, _db, schema) = open_with_odd_keys();
        // skip_past in reverse: the cursor lands on the largest key
        // strictly less than the prefix.
        let mut iter = schema.items.iter_rev(..).unwrap();
        iter.skip_past(6u64.to_be_bytes());
        let collected: Vec<_> = (&mut iter).map(|r| r.unwrap().0).collect();
        assert_eq!(collected, vec![U64Be(5), U64Be(3), U64Be(1)]);
    }

    #[test]
    fn iter_rev_skip_past_below_data_yields_nothing() {
        let (_dir, _db, schema) = open_with_odd_keys();
        let mut iter = schema.items.iter_rev(..).unwrap();
        iter.skip_past(0u64.to_be_bytes());
        assert!(iter.next().is_none());
    }

    /// Compound key `(byte, u32 BE)` for prefix-iteration tests.
    /// Used to demonstrate prefix iteration with a prefix type
    /// distinct from the full key type.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
    struct ByteAndU32(u8, u32);

    impl Encode for ByteAndU32 {
        fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
            buf.put_u8(self.0);
            buf.put_slice(&self.1.to_be_bytes());
            Ok(())
        }
    }

    impl Decode for ByteAndU32 {
        fn decode<B: bytes::Buf>(buf: &mut B) -> Result<Self, DecodeError> {
            if buf.remaining() != 5 {
                return Err(DecodeError::msg(format!(
                    "expected 5 bytes for ByteAndU32, got {}",
                    buf.remaining(),
                )));
            }
            Ok(Self(buf.get_u8(), buf.get_u32()))
        }
    }

    /// Single-byte prefix used to filter `ByteAndU32` keys by their
    /// first component.
    #[derive(Debug, Clone, Copy)]
    struct BytePrefix(u8);

    impl Encode for BytePrefix {
        fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
            buf.put_u8(self.0);
            Ok(())
        }
    }

    #[derive(Debug)]
    struct CompoundSchema<R: Reader = Db> {
        rows: DbMap<ByteAndU32, U64Be, R>,
    }

    impl Schema for CompoundSchema {
        fn cfs(opts: &crate::options::CfOptionsResolver) -> Vec<crate::CfDescriptor> {
            vec![crate::CfDescriptor::new("rows", opts.options("rows"))]
        }

        fn open(db: &Db) -> Result<Self, OpenError> {
            Ok(Self {
                rows: DbMap::new(db.clone(), "rows")?,
            })
        }
    }

    fn open_compound() -> (TempDir, Db, CompoundSchema) {
        let dir = TempDir::new().unwrap();
        let (db, schema) = Db::open::<CompoundSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db, schema)
    }

    fn seed_compound(db: &Db, key: &ByteAndU32, value: &U64Be) {
        let cf = db.cf_handle("rows").unwrap();
        let k_bytes = key.encode().unwrap();
        let v_bytes = value.encode().unwrap();
        db.rocksdb().put_cf(&cf, k_bytes, v_bytes).unwrap();
    }

    #[test]
    fn iter_prefix_with_distinct_prefix_type_filters_correctly() {
        let (_dir, db, schema) = open_compound();
        // Seed across three first-byte buckets.
        for first in [1u8, 2, 3] {
            for second in 0u32..3 {
                seed_compound(&db, &ByteAndU32(first, second), &U64Be(0));
            }
        }
        let collected: Vec<_> = schema
            .rows
            .iter_prefix(&BytePrefix(2))
            .unwrap()
            .map(|r| r.unwrap().0)
            .collect();
        assert_eq!(
            collected,
            vec![ByteAndU32(2, 0), ByteAndU32(2, 1), ByteAndU32(2, 2),],
        );
    }

    #[test]
    fn iter_rev_prefix_yields_matches_in_reverse() {
        let (_dir, db, schema) = open_compound();
        for second in 0u32..3 {
            seed_compound(&db, &ByteAndU32(7, second), &U64Be(0));
        }
        // A neighbor bucket so we know the prefix bound holds.
        seed_compound(&db, &ByteAndU32(8, 0), &U64Be(0));
        let collected: Vec<_> = schema
            .rows
            .iter_rev_prefix(&BytePrefix(7))
            .unwrap()
            .map(|r| r.unwrap().0)
            .collect();
        assert_eq!(
            collected,
            vec![ByteAndU32(7, 2), ByteAndU32(7, 1), ByteAndU32(7, 0),],
        );
    }

    #[test]
    fn iter_prefix_yields_nothing_when_no_matches() {
        let (_dir, db, schema) = open_compound();
        seed_compound(&db, &ByteAndU32(1, 0), &U64Be(0));
        seed_compound(&db, &ByteAndU32(3, 0), &U64Be(0));
        let count = schema.rows.iter_prefix(&BytePrefix(2)).unwrap().count();
        assert_eq!(count, 0);
    }

    #[test]
    fn iter_with_bounds_a_prefix_typed_range_on_compound_keys() {
        let (_dir, db, schema) = open_compound();
        // Seed three first-byte buckets, three second-component each.
        for first in [1u8, 2, 3] {
            for second in 0u32..3 {
                seed_compound(&db, &ByteAndU32(first, second), &U64Be(0));
            }
        }
        // Bound by `BytePrefix(2)..BytePrefix(3)` — J = BytePrefix,
        // K = ByteAndU32. Should yield the bucket starting with 2.
        let collected: Vec<_> = schema
            .rows
            .iter_with(BytePrefix(2)..BytePrefix(3))
            .unwrap()
            .map(|r| r.unwrap().0)
            .collect();
        assert_eq!(
            collected,
            vec![ByteAndU32(2, 0), ByteAndU32(2, 1), ByteAndU32(2, 2)],
        );
    }

    #[test]
    fn iter_prefix_with_max_byte_prefix_iterates_to_end() {
        let (_dir, db, schema) = open_compound();
        // Seed entries with first byte = 0xFF so the prefix has no
        // exclusive upper bound (no byte greater than 0xFF).
        for second in 0u32..3 {
            seed_compound(&db, &ByteAndU32(0xFF, second), &U64Be(0));
        }
        // Plus a non-matching entry to confirm the lower bound works.
        seed_compound(&db, &ByteAndU32(0xFE, 0), &U64Be(0));
        let collected: Vec<_> = schema
            .rows
            .iter_prefix(&BytePrefix(0xFF))
            .unwrap()
            .map(|r| r.unwrap().0)
            .collect();
        assert_eq!(
            collected,
            vec![
                ByteAndU32(0xFF, 0),
                ByteAndU32(0xFF, 1),
                ByteAndU32(0xFF, 2),
            ],
        );
    }
}
