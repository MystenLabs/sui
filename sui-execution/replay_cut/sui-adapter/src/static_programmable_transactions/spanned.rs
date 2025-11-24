// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module provides a shared API amongst the ASTs for original command and argument locations.
//! Some translations might reorder or move commands/arguments, and as such we need to annotate
//! the values with the original location

use std::{
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
};

#[derive(Copy, Clone)]
pub struct Spanned<T> {
    pub idx: u16,
    pub value: T,
}

impl<T: PartialEq> PartialEq for Spanned<T> {
    fn eq(&self, other: &Spanned<T>) -> bool {
        self.value == other.value
    }
}

impl<T: Eq> Eq for Spanned<T> {}

impl<T: Hash> Hash for Spanned<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<T: PartialOrd> PartialOrd for Spanned<T> {
    fn partial_cmp(&self, other: &Spanned<T>) -> Option<Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

impl<T: Ord> Ord for Spanned<T> {
    fn cmp(&self, other: &Spanned<T>) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl<T: fmt::Display> fmt::Display for Spanned<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", &self.value)
    }
}

impl<T: fmt::Debug> fmt::Debug for Spanned<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", &self.value)
    }
}

/// Function used to have nearly tuple-like syntax for creating a Spanned
pub const fn sp<T>(idx: u16, value: T) -> Spanned<T> {
    Spanned { idx, value }
}

/// Macro used to create a tuple-like pattern match for Spanned
#[macro_export]
macro_rules! sp {
    (_, $value:pat) => {
        $crate::static_programmable_transactions::spanned::Spanned { value: $value, .. }
    };
    ($idx:pat, _) => {
        $crate::static_programmable_transactions::spanned::Spanned { idx: $idx, .. }
    };
    ($idx:pat, $value:pat) => {
        $crate::static_programmable_transactions::spanned::Spanned {
            idx: $idx,
            value: $value,
        }
    };
}
