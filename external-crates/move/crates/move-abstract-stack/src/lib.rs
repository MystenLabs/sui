// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module defines an abstract stack used by various verifier passes

#[cfg(test)]
mod unit_tests;

use std::cmp::Ordering;
use std::fmt::{self, Debug};
use std::num::NonZeroU64;

#[derive(Default, Debug)]
/// An abstract value that compresses runs of the same value to reduce space usage
pub struct AbstractStack<T> {
    values: Vec<(u64, T)>,
    len: u64,
}

impl<T: Eq + Clone + Debug> AbstractStack<T> {
    /// Creates an empty stack
    pub fn new() -> Self {
        Self {
            values: vec![],
            len: 0,
        }
    }

    /// Returns true iff the stack is empty
    pub fn is_empty(&self) -> bool {
        // empty ==> len is 0
        debug_assert!(!self.values.is_empty() || self.len == 0);
        // !empty ==> last element len <= len
        debug_assert!(self.values.is_empty() || self.values.last().unwrap().0 <= self.len);
        self.values.is_empty()
    }

    /// Returns the logical length of the stack as if there was no space
    /// compression
    pub fn len(&self) -> u64 {
        // len is 0 ==> empty
        debug_assert!(self.len != 0 || self.values.is_empty());
        // len not 0 ==> !empty and last element len <= len
        debug_assert!(
            self.len == 0 || (!self.values.is_empty() && self.values.last().unwrap().0 <= self.len)
        );
        self.len
    }

    /// Pushes a single value on the stack
    pub fn push(&mut self, item: T) -> Result<(), AbsStackError> {
        self.push_n(item, 1)
    }

    /// Push n copies of an item on the stack
    pub fn push_n(&mut self, item: T, n: u64) -> Result<(), AbsStackError> {
        if n == 0 {
            return Ok(());
        }

        let Some(new_len) = self.len.checked_add(n) else {
            return Err(AbsStackError::Overflow);
        };
        self.len = new_len;
        match self.values.last_mut() {
            Some((count, last_item)) if &item == last_item => {
                debug_assert!(*count > 0);
                *count += n
            }
            _ => self.values.push((n, item)),
        }
        Ok(())
    }

    /// Pops a single value off the stack
    pub fn pop(&mut self) -> Result<T, AbsStackError> {
        self.pop_eq_n(NonZeroU64::new(1).unwrap())
    }

    /// Pops n values off the stack, erroring if there are not enough items or if the n items are
    /// not equal
    pub fn pop_eq_n(&mut self, n: NonZeroU64) -> Result<T, AbsStackError> {
        let n: u64 = n.get();
        if self.is_empty() || n > self.len {
            return Err(AbsStackError::Underflow);
        }
        let (count, last) = self.values.last_mut().unwrap();
        debug_assert!(*count > 0);
        let ret = match (*count).cmp(&n) {
            Ordering::Less => return Err(AbsStackError::ElementNotEqual),
            Ordering::Equal => {
                let (_, last) = self.values.pop().unwrap();
                last
            }
            Ordering::Greater => {
                *count -= n;
                last.clone()
            }
        };
        self.len -= n;
        Ok(ret)
    }

    /// Pop any n items off the stack. Unlike `pop_n`, items do not have to be equal
    pub fn pop_any_n(&mut self, n: NonZeroU64) -> Result<(), AbsStackError> {
        let n: u64 = n.get();
        if self.is_empty() || n > self.len {
            return Err(AbsStackError::Underflow);
        }
        let mut rem: u64 = n;
        while rem > 0 {
            let (count, _last) = self.values.last_mut().unwrap();
            debug_assert!(*count > 0);
            match (*count).cmp(&rem) {
                Ordering::Less | Ordering::Equal => {
                    rem -= *count;
                    self.values.pop().unwrap();
                }
                Ordering::Greater => {
                    *count -= rem;
                    break;
                }
            }
        }
        self.len -= n;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn assert_run_lengths<Items, Item>(&self, lengths: Items)
    where
        Item: std::borrow::Borrow<u64>,
        Items: IntoIterator<Item = Item>,
        <Items as IntoIterator>::IntoIter: ExactSizeIterator,
    {
        let lengths_iter = lengths.into_iter();
        assert_eq!(self.values.len(), lengths_iter.len());
        let mut sum = 0;
        for ((actual, _), expected) in self.values.iter().zip(lengths_iter) {
            let expected = expected.borrow();
            assert_eq!(actual, expected);
            sum += *expected;
        }
        assert_eq!(self.len, sum);
    }
}

#[derive(Eq, PartialEq, PartialOrd, Ord, Clone, Copy, Debug)]
pub enum AbsStackError {
    ElementNotEqual,
    Underflow,
    Overflow,
}

impl fmt::Display for AbsStackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AbsStackError::ElementNotEqual => {
                write!(f, "Popped element is not equal to specified item")
            }
            AbsStackError::Underflow => {
                write!(f, "Popped more values than are on the stack")
            }
            AbsStackError::Overflow => {
                write!(f, "Pushed too many elements on the stack")
            }
        }
    }
}
