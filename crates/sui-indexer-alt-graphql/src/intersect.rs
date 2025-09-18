// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Merges two filter fields. If both values exist, `merge` is used to combine them, which returns
/// some combined value if there is some consistent combination, and `None` otherwise. The overall
/// function returns `Some(None)`, if the filters combined to no filter, `Some(Some(f))` if the
/// filters combined to `f`, and `None` if the filters couldn't be combined.
pub(crate) fn field<T>(
    this: Option<T>,
    that: Option<T>,
    merge: impl FnOnce(T, T) -> Option<T>,
) -> Option<Option<T>> {
    match (this, that) {
        (None, None) => Some(None),
        (Some(this), None) => Some(Some(this)),
        (None, Some(that)) => Some(Some(that)),
        (Some(this), Some(that)) => merge(this, that).map(Some),
    }
}

/// Merge options by equality check (equal values get merged, everything else is inconsistent).
pub(crate) fn by_eq<T: Eq>(a: T, b: T) -> Option<T> {
    (a == b).then_some(a)
}

/// Merge options by taking the max.
pub(crate) fn by_max<T: Ord>(a: T, b: T) -> Option<T> {
    Some(a.max(b))
}

/// Merge options by taking the min.
pub(crate) fn by_min<T: Ord>(a: T, b: T) -> Option<T> {
    Some(a.min(b))
}
