// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::mem;

pub(crate) trait Peekable2Ext: Iterator + Sized {
    fn peekable2(self) -> Peekable2<Self> {
        Peekable2::new(self)
    }
}

impl<T: Iterator> Peekable2Ext for T {}

/// A peekaable iterator that allows for peeking the next two items. Unlike `std::iter::Peekable`,
/// this iterator eagerly consumes elements from the underlying iterator to fill its "peek" slots.
pub(crate) struct Peekable2<T: Iterator> {
    fst: Option<T::Item>,
    snd: Option<T::Item>,
    iter: T,
}

impl<T: Iterator> Peekable2<T> {
    pub fn new(mut iter: T) -> Self {
        Self {
            fst: iter.next(),
            snd: iter.next(),
            iter,
        }
    }

    pub fn peek(&self) -> Option<&T::Item> {
        self.fst.as_ref()
    }

    pub fn peek2(&self) -> Option<&T::Item> {
        self.snd.as_ref()
    }
}

impl<T: Iterator> Iterator for Peekable2<T> {
    type Item = T::Item;

    fn next(&mut self) -> Option<Self::Item> {
        mem::swap(&mut self.fst, &mut self.snd);
        mem::replace(&mut self.snd, self.iter.next())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peekable() {
        let mut iter = (0..10).peekable2();

        assert_eq!(iter.peek(), Some(&0));
        assert_eq!(iter.peek2(), Some(&1));

        assert_eq!(iter.next(), Some(0));
        assert_eq!(iter.peek(), Some(&1));
        assert_eq!(iter.peek2(), Some(&2));

        assert_eq!(iter.next(), Some(1));
        assert_eq!(iter.peek(), Some(&2));
        assert_eq!(iter.peek2(), Some(&3));

        let rest: Vec<_> = iter.collect();
        assert_eq!(rest, vec![2, 3, 4, 5, 6, 7, 8, 9]);
    }
}
