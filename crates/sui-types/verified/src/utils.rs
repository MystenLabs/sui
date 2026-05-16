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

} // verus!
