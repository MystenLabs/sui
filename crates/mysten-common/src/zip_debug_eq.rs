// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::debug_fatal_at;
use std::panic::Location;

/// Extension trait providing `zip_debug_eq` on all iterators.
pub trait ZipDebugEqIteratorExt: Iterator + Sized {
    /// Like `zip_eq`, but instead of panicking logs `debug_fatal!` if the iterators have
    /// different lengths. After the diagnostic, behaves like `zip` (returns `None`).
    #[track_caller]
    fn zip_debug_eq<J: IntoIterator>(self, other: J) -> ZipDebugEq<Self, J::IntoIter> {
        ZipDebugEq {
            a: self,
            b: other.into_iter(),
            finished: false,
            caller: Location::caller(),
        }
    }
}

impl<I: Iterator> ZipDebugEqIteratorExt for I {}

pub struct ZipDebugEq<A, B> {
    a: A,
    b: B,
    finished: bool,
    caller: &'static Location<'static>,
}

impl<A: Iterator, B: Iterator> Iterator for ZipDebugEq<A, B> {
    type Item = (A::Item, B::Item);

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        match (self.a.next(), self.b.next()) {
            (Some(a), Some(b)) => Some((a, b)),
            (None, None) => None,
            (None, Some(_)) => {
                self.finished = true;
                debug_fatal_at!(
                    &format!("{}:{}", self.caller.file(), self.caller.line()),
                    "zip_debug_eq: first iterator shorter than second (created at {})",
                    self.caller
                );
                None
            }
            (Some(_), None) => {
                self.finished = true;
                debug_fatal_at!(
                    &format!("{}:{}", self.caller.file(), self.caller.line()),
                    "zip_debug_eq: second iterator shorter than first (created at {})",
                    self.caller
                );
                None
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.finished {
            return (0, Some(0));
        }
        let (a_lower, a_upper) = self.a.size_hint();
        let (b_lower, b_upper) = self.b.size_hint();
        let lower = a_lower.min(b_lower);
        let upper = match (a_upper, b_upper) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        (lower, upper)
    }
}

/// Like `itertools::izip!`, but uses `zip_debug_eq` instead of `zip`.
#[macro_export]
macro_rules! izip_debug_eq {
    // Closure helper for tuple flattening (same pattern as itertools::izip!)
    ( @closure $p:pat => $tup:expr ) => {
        |$p| $tup
    };
    ( @closure $p:pat => ( $($tup:tt)* ) , $_iter:expr $( , $tail:expr )* ) => {
        $crate::izip_debug_eq!(@closure ($p, b) => ( $($tup)*, b ) $( , $tail )*)
    };

    // unary
    ($first:expr $(,)*) => {
        ::core::iter::IntoIterator::into_iter($first)
    };

    // binary
    ($first:expr, $second:expr $(,)*) => {{
        use $crate::ZipDebugEqIteratorExt as _;
        $crate::izip_debug_eq!($first).zip_debug_eq($second)
    }};

    // n-ary where n > 2
    ( $first:expr $( , $rest:expr )* $(,)* ) => {{
        use $crate::ZipDebugEqIteratorExt as _;
        $crate::izip_debug_eq!($first)
            $(
                .zip_debug_eq($rest)
            )*
            .map(
                $crate::izip_debug_eq!(@closure a => (a) $( , $rest )*)
            )
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_length_iterators() {
        let a = vec![1, 2, 3];
        let b = vec!["a", "b", "c"];
        let result: Vec<_> = a.into_iter().zip_debug_eq(b).collect();
        assert_eq!(result, vec![(1, "a"), (2, "b"), (3, "c")]);
    }

    #[test]
    fn empty_iterators() {
        let a: Vec<i32> = vec![];
        let b: Vec<i32> = vec![];
        let result: Vec<_> = a.into_iter().zip_debug_eq(b).collect();
        assert_eq!(result, vec![]);
    }

    #[test]
    #[should_panic]
    fn first_shorter_panics_in_debug() {
        let a = vec![1, 2];
        let b = vec!["a", "b", "c"];
        let _: Vec<_> = a.into_iter().zip_debug_eq(b).collect();
    }

    #[test]
    #[should_panic]
    fn second_shorter_panics_in_debug() {
        let a = vec![1, 2, 3];
        let b = vec!["a", "b"];
        let _: Vec<_> = a.into_iter().zip_debug_eq(b).collect();
    }

    #[test]
    fn izip_debug_eq_binary() {
        let a = vec![1, 2, 3];
        let b = vec!["a", "b", "c"];
        let result: Vec<_> = izip_debug_eq!(a, b).collect();
        assert_eq!(result, vec![(1, "a"), (2, "b"), (3, "c")]);
    }

    #[test]
    fn izip_debug_eq_ternary() {
        let a = vec![1, 2];
        let b = vec!["a", "b"];
        let c = vec![10.0, 20.0];
        let result: Vec<_> = izip_debug_eq!(a, b, c).collect();
        assert_eq!(result, vec![(1, "a", 10.0), (2, "b", 20.0)]);
    }

    #[test]
    #[should_panic]
    fn izip_debug_eq_mismatch_panics() {
        let a = vec![1, 2, 3];
        let b = vec!["a", "b"];
        let c = vec![10.0, 20.0, 30.0];
        let _: Vec<_> = izip_debug_eq!(a, b, c).collect();
    }
}
