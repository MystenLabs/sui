// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Small utility functions with Verus specifications.

use vstd::prelude::*;

verus! {

/// Check whether a slice contains an element.
///
/// Trusted (`external_body`) — neither `Iterator::any` nor `slice::contains`
/// is specced in vstd. The spec is straightforward: true iff the element
/// appears in the slice's ghost sequence view.
#[verifier::external_body]
pub fn slice_contains<T: PartialEq + Eq + Copy>(v: &[T], elem: T) -> (b: bool)
    ensures b == v@.to_set().contains(elem)
{
    v.iter().any(|a| *a == elem)
}

/// Clone a boolean slice and set position `pos` to `true`.
///
/// Trusted (`external_body`) — `slice::to_vec` and indexing assignment
/// are not specced in vstd. The spec captures what callers need for the proof.
#[verifier::external_body]
pub fn clone_and_set<T: Clone>(v: &[T], pos: usize, val: T) -> (result: Vec<T>)
    requires pos < v@.len()
    ensures
        result@.len() == v@.len(),
        result@[pos as int] == val,
        forall|k: int| 0 <= k < result@.len() && k != pos as int ==> result@[k] == v@[k],
{
    let mut r = v.to_vec();
    r[pos] = val;
    r
}

/// Prepend a single `u8` to a slice, returning a new `Vec`.
///
/// Trusted (`external_body`) — `extend_from_slice` is not specced in vstd.
/// The ensures directly state the concatenation so callers need no extra proof.
#[verifier::external_body]
pub fn prepend_u8(x: u8, v: &[u8]) -> (result: Vec<u8>)
    ensures
        result@.len() == 1 + v@.len(),
        result@[0] == x,
        forall|i: int| 1 <= i < result@.len() ==> result@[i] == v@[i - 1],
        result@ =~= seq![x] + v@,
{
    let mut r = Vec::with_capacity(1 + v.len());
    r.push(x);
    r.extend_from_slice(v);
    r
}

} // verus!
