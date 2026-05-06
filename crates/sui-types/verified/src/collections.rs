// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Generic verified collection wrappers.
//!
//! These types give verified code a clean ghost view while keeping the
//! underlying stdlib containers accessible (via `pub inner`) for unverified
//! code that needs iteration or other operations vstd doesn't expose.

use std::collections::HashMap;
use std::hash::Hash;
use vstd::prelude::*;

verus! {

/// Thin wrapper around `HashMap<K, V>` with a `Map<K, V>` ghost view.
///
/// # Design
///
/// * Verified code uses the specced methods (`contains_key`, `insert_new`,
///   `get_value`) whose postconditions are `external_body`-trusted.
/// * Unverified code (e.g. callers that need to iterate or call `.remove()`)
///   accesses the real HashMap via `self.inner` directly.
/// * The `view()` is `open` and delegates to the HashMap's own vstd view, so
///   `map@` unfolds transparently without any additional axioms.
#[derive(Debug)]
pub struct VerifiedHashMap<K, V> {
    pub inner: HashMap<K, V>,
}

impl<K, V> View for VerifiedHashMap<K, V> {
    type V = Map<K, V>;
    // Transparent: self@ unfolds to self.inner@ (HashMap's vstd view).
    open spec fn view(&self) -> Map<K, V> { self.inner@ }
}

impl<K: Eq + Hash, V> VerifiedHashMap<K, V> {
    #[verifier::external_body]
    pub fn new() -> (out: Self)
        ensures
            out@ =~= Map::<K, V>::empty(),
            out@.dom().finite(),
    {
        Self { inner: HashMap::new() }
    }

    #[verifier::external_body]
    pub fn contains_key(&self, k: &K) -> (b: bool)
        ensures b == self@.contains_key(*k),
    {
        self.inner.contains_key(k)
    }

    /// Insert a key that is guaranteed to be absent. Callers must prove
    /// `!self@.contains_key(k)` so the domain grows by exactly `{k}`.
    #[verifier::external_body]
    pub fn insert_new(&mut self, k: K, v: V)
        requires
            !old(self)@.contains_key(k),
            old(self)@.dom().finite(),
        ensures
            self@ =~= old(self)@.insert(k, v),
            self@.dom().finite(),
    {
        self.inner.insert(k, v);
    }

    /// Look up a value. No useful spec: callers use this only for exec-only
    /// metadata that does not affect the verified invariant.
    #[verifier::external_body]
    pub fn get_value<'a>(&'a self, k: &K) -> (out: Option<&'a V>) {
        self.inner.get(k)
    }
}

impl<K: Eq + Hash, V> Default for VerifiedHashMap<K, V> {
    #[verifier::external_body]
    fn default() -> (out: Self)
        ensures
            out@ =~= Map::<K, V>::empty(),
            out@.dom().finite(),
    {
        Self::new()
    }
}

} // verus!
