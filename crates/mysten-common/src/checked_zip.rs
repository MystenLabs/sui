// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::debug_fatal;
use std::panic::Location;

/// Extension trait providing `checked_zip` on all iterators.
pub trait CheckedIteratorExt: Iterator + Sized {
    /// Like `zip`, but fires `debug_fatal!` if the iterators have different lengths.
    /// After the diagnostic, behaves like `zip` (returns `None`).
    /// Uses `#[track_caller]` to include the callsite in the error message.
    #[track_caller]
    fn checked_zip<J: IntoIterator>(self, other: J) -> CheckedZip<Self, J::IntoIter> {
        CheckedZip {
            a: self,
            b: other.into_iter(),
            finished: false,
            caller: Location::caller(),
        }
    }
}

impl<I: Iterator> CheckedIteratorExt for I {}

pub struct CheckedZip<A, B> {
    a: A,
    b: B,
    finished: bool,
    caller: &'static Location<'static>,
}

impl<A: Iterator, B: Iterator> Iterator for CheckedZip<A, B> {
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
                debug_fatal!(
                    "checked_zip: first iterator shorter than second (created at {})",
                    self.caller
                );
                None
            }
            (Some(_), None) => {
                self.finished = true;
                debug_fatal!(
                    "checked_zip: second iterator shorter than first (created at {})",
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

/// Like `itertools::izip!`, but uses `checked_zip` instead of `zip`.
#[macro_export]
macro_rules! checked_izip {
    // Closure helper for tuple flattening (same pattern as itertools::izip!)
    ( @closure $p:pat => $tup:expr ) => {
        |$p| $tup
    };
    ( @closure $p:pat => ( $($tup:tt)* ) , $_iter:expr $( , $tail:expr )* ) => {
        $crate::checked_izip!(@closure ($p, b) => ( $($tup)*, b ) $( , $tail )*)
    };

    // unary
    ($first:expr $(,)*) => {
        ::core::iter::IntoIterator::into_iter($first)
    };

    // binary
    ($first:expr, $second:expr $(,)*) => {{
        use $crate::CheckedIteratorExt as _;
        $crate::checked_izip!($first).checked_zip($second)
    }};

    // n-ary where n > 2
    ( $first:expr $( , $rest:expr )* $(,)* ) => {{
        use $crate::CheckedIteratorExt as _;
        $crate::checked_izip!($first)
            $(
                .checked_zip($rest)
            )*
            .map(
                $crate::checked_izip!(@closure a => (a) $( , $rest )*)
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
        let result: Vec<_> = a.into_iter().checked_zip(b).collect();
        assert_eq!(result, vec![(1, "a"), (2, "b"), (3, "c")]);
    }

    #[test]
    fn empty_iterators() {
        let a: Vec<i32> = vec![];
        let b: Vec<i32> = vec![];
        let result: Vec<_> = a.into_iter().checked_zip(b).collect();
        assert_eq!(result, vec![]);
    }

    #[test]
    #[should_panic]
    fn first_shorter_panics_in_debug() {
        let a = vec![1, 2];
        let b = vec!["a", "b", "c"];
        let _: Vec<_> = a.into_iter().checked_zip(b).collect();
    }

    #[test]
    #[should_panic]
    fn second_shorter_panics_in_debug() {
        let a = vec![1, 2, 3];
        let b = vec!["a", "b"];
        let _: Vec<_> = a.into_iter().checked_zip(b).collect();
    }

    #[test]
    fn checked_izip_binary() {
        let a = vec![1, 2, 3];
        let b = vec!["a", "b", "c"];
        let result: Vec<_> = checked_izip!(a, b).collect();
        assert_eq!(result, vec![(1, "a"), (2, "b"), (3, "c")]);
    }

    #[test]
    fn checked_izip_ternary() {
        let a = vec![1, 2];
        let b = vec!["a", "b"];
        let c = vec![10.0, 20.0];
        let result: Vec<_> = checked_izip!(a, b, c).collect();
        assert_eq!(result, vec![(1, "a", 10.0), (2, "b", 20.0)]);
    }

    #[test]
    #[should_panic]
    fn checked_izip_mismatch_panics() {
        let a = vec![1, 2, 3];
        let b = vec!["a", "b"];
        let c = vec![10.0, 20.0, 30.0];
        let _: Vec<_> = checked_izip!(a, b, c).collect();
    }
}
