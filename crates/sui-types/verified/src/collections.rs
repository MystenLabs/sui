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

    #[verifier::external_body]
    pub fn get_value<'a>(&'a self, k: &K) -> (out: Option<&'a V>)
        ensures
            // Key is present iff a value is returned, and the returned value
            // matches what the ghost map records at that key.
            match out {
                Some(v) => self@.contains_key(*k) && self@[*k] == *v,
                None => !self@.contains_key(*k),
            },
    {
        self.inner.get(k)
    }

    /// Remove a key. Used by unverified code (e.g. the AuthoritySignInfo
    /// specialisation that evicts bad signers); spec states the key is gone
    /// and the rest of the map is unchanged.
    #[verifier::external_body]
    pub fn remove(&mut self, k: &K) -> (out: Option<V>)
        ensures
            !self@.contains_key(*k),
            match out {
                Some(v) => old(self)@.contains_key(*k) && old(self)@[*k] == v,
                None => !old(self)@.contains_key(*k),
            },
            forall|j: K| j != *k ==>
                self@.contains_key(j) == old(self)@.contains_key(j),
    {
        self.inner.remove(k)
    }

    /// Iterate over keys. No spec beyond satisfying the trait; callers that
    /// need spec-level reasoning about the domain should use `self@.dom()`.
    #[verifier::external_body]
    pub fn keys(&self) -> (out: impl Iterator<Item = &K>) {
        self.inner.keys()
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
