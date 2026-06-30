// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Page-cursor pagination engine for the v1alpha
//! [`ConsistentService`] list endpoints. Ports the shape of
//! `sui_indexer_alt_consistent_store::rpc::pagination` to the
//! `sui-consistent-store` iter API.
//!
//! Cursors are opaque raw key bytes — the bytes the iterator
//! exposes via [`Iter::raw_key`]. Each `Balance` / `Object` in
//! the response carries its `page_token` so a caller can resume
//! from any point.
//!
//! `after_token` / `before_token` semantics match the
//! alt-consistent-store: forward iteration starts immediately
//! after `after_token`; backward iteration starts immediately
//! before `before_token`. Both directions stamp `has_prev` /
//! `has_next` so clients know whether more data exists.
//!
//! [`ConsistentService`]: sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_server::ConsistentService
//! [`Iter::raw_key`]: sui_consistent_store::iter::Iter::raw_key

use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::error::Error as DbError;
use sui_consistent_store::iter::Iter;
use sui_consistent_store::iter::RevIter;
use sui_consistent_store::reader::Reader;

use crate::config::PaginationConfig;

/// `End` mirrors the proto enum, including the `Unknown`
/// default (which we treat as "forward").
pub(crate) enum End {
    Front,
    Back,
}

impl End {
    /// Translate the proto `End` value into the typed enum,
    /// folding `Unknown` and missing into `Front`.
    pub(crate) fn from_proto(value: i32) -> Self {
        use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::End as Proto;
        match Proto::try_from(value).unwrap_or(Proto::Unknown) {
            Proto::Back => End::Back,
            Proto::Front | Proto::Unknown => End::Front,
        }
    }
}

/// Page parameters parsed off a list-request, plus the
/// configured limits the engine applies.
pub(crate) struct Page<'r> {
    after: Option<&'r [u8]>,
    before: Option<&'r [u8]>,
    limit: usize,
    is_from_front: bool,
}

/// Raw paginated rows plus the headline `has_previous_page` /
/// `has_next_page` flags. The engine is generic over the
/// decoded key / value types; callers do the final shape into
/// the proto response.
pub(crate) struct Response<K, V> {
    pub has_prev: bool,
    pub has_next: bool,
    /// `(cursor_bytes, K, V)` triples in iteration order.
    pub results: Vec<(Vec<u8>, K, V)>,
}

impl<'r> Page<'r> {
    pub(crate) fn from_request(
        config: &PaginationConfig,
        after: &'r [u8],
        before: &'r [u8],
        page_size: u32,
        end: End,
    ) -> Self {
        let limit = if page_size == 0 {
            config.default_page_size
        } else if page_size > config.max_page_size {
            config.max_page_size
        } else {
            page_size
        } as usize;

        let is_from_front = matches!(end, End::Front);

        Self {
            after: (!after.is_empty()).then_some(after),
            before: (!before.is_empty()).then_some(before),
            limit,
            is_from_front,
        }
    }

    /// Exposed for future handlers / tests that need the
    /// clamped page size.
    #[allow(dead_code)]
    pub(crate) fn limit(&self) -> usize {
        self.limit
    }

    /// Same — useful when a handler wants to special-case
    /// "front" direction without re-parsing the proto request.
    #[allow(dead_code)]
    pub(crate) fn is_from_front(&self) -> bool {
        self.is_from_front
    }

    /// Paginate forward / backward over every entry in `map`
    /// whose encoded key starts with `prefix`, with no
    /// additional filtering.
    pub(crate) fn paginate_prefix<R, K, V, P>(
        &self,
        map: &sui_consistent_store::DbMap<K, V, R>,
        prefix: &P,
    ) -> Result<Response<K, V>, DbError>
    where
        R: Reader,
        K: Encode + Decode,
        V: Decode,
        P: Encode,
    {
        self.paginate_filtered(map, prefix, |_, _, _| true)
    }

    /// Like [`paginate_prefix`](Self::paginate_prefix) but skips
    /// entries for which `pred` returns false.
    pub(crate) fn paginate_filtered<R, K, V, P>(
        &self,
        map: &sui_consistent_store::DbMap<K, V, R>,
        prefix: &P,
        pred: impl FnMut(&[u8], &K, &V) -> bool,
    ) -> Result<Response<K, V>, DbError>
    where
        R: Reader,
        K: Encode + Decode,
        V: Decode,
        P: Encode,
    {
        if self.is_from_front {
            self.paginate_from_front(map.iter_prefix(prefix)?, pred)
        } else {
            self.paginate_from_back(map.iter_rev_prefix(prefix)?, pred)
        }
    }

    fn paginate_from_front<K, V>(
        &self,
        mut iter: Iter<'_, K, V>,
        mut pred: impl FnMut(&[u8], &K, &V) -> bool,
    ) -> Result<Response<K, V>, DbError>
    where
        K: Decode,
        V: Decode,
    {
        // Normalize `after` against the first key in the
        // iterator. A cursor that points before the iterator's
        // current head means the caller is asking for a page
        // that overlaps with a previous one; treat that as "no
        // after" so we don't double-skip. If `after` survives
        // this normalization, a previous page is guaranteed to
        // exist.
        let after = match (iter.raw_key(), self.after) {
            (_, None) | (None, _) => None,
            (Some(f), Some(a)) => (f <= a).then_some(a),
        };

        if let Some(a) = after {
            iter.seek(a);
            if iter.raw_key() == Some(a) {
                iter.next();
            }
        }

        let mut results = Vec::with_capacity(self.limit);
        while results.len() < self.limit {
            let Some(cursor) = iter.raw_key() else {
                break;
            };

            if self.before.is_some_and(|b| cursor >= b) {
                return Ok(Response {
                    has_prev: after.is_some(),
                    has_next: true,
                    results,
                });
            }

            let cursor = cursor.to_owned();
            let (key, value) = iter.next().expect("raw_key returned Some")?;
            if pred(&cursor, &key, &value) {
                results.push((cursor, key, value));
            }
        }

        Ok(Response {
            has_prev: after.is_some(),
            has_next: iter.valid(),
            results,
        })
    }

    fn paginate_from_back<K, V>(
        &self,
        mut iter: RevIter<'_, K, V>,
        mut pred: impl FnMut(&[u8], &K, &V) -> bool,
    ) -> Result<Response<K, V>, DbError>
    where
        K: Decode,
        V: Decode,
    {
        let before = match (self.before, iter.raw_key()) {
            (_, None) | (None, _) => None,
            (Some(b), Some(l)) => (b <= l).then_some(b),
        };

        if let Some(b) = before {
            iter.seek(b);
            if iter.raw_key() == Some(b) {
                iter.next();
            }
        }

        let mut results = Vec::with_capacity(self.limit);
        while results.len() < self.limit {
            let Some(cursor) = iter.raw_key() else {
                break;
            };

            if self.after.is_some_and(|a| cursor <= a) {
                return Ok(Response {
                    has_prev: true,
                    has_next: before.is_some(),
                    results,
                });
            }

            let cursor = cursor.to_owned();
            let (key, value) = iter.next().expect("raw_key returned Some")?;
            if pred(&cursor, &key, &value) {
                results.push((cursor, key, value));
            }
        }

        Ok(Response {
            has_prev: iter.valid(),
            has_next: before.is_some(),
            results,
        })
    }
}
