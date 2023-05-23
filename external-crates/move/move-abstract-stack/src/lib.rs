// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module defines an abstract stack used by various verifier passes

use std::cmp::Ordering;
use std::fmt;

#[derive(Default)]
/// An abstract value that compresses runs of the same value to reduce space usage
pub struct AbsStack<T> {
    values: Vec<(u64, T)>,
}

impl<T: Eq + Clone> AbsStack<T> {
    /// Creates an empty stack
    pub fn new() -> Self {
        Self { values: vec![] }
    }

    /// Returns true iff the stack is empty
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Pushes a single value on the stack
    pub fn push(&mut self, item: T) {
        self.push_n(item, 1)
    }

    /// Pops a single value off the stack, erroring if empty
    pub fn pop(&mut self) -> Result<T, AbsStackError> {
        let Some((count, last)) = self.values.last_mut() else {
            return Err(AbsStackError::EmptyStack)
        };
        debug_assert!(*count > 0);
        Ok(if *count <= 1 {
            let (_, last) = self.values.pop().unwrap();
            last
        } else {
            *count -= 1;
            last.clone()
        })
    }

    /// Push n copies of an item on the stack
    pub fn push_n(&mut self, item: T, n: u64) {
        if n == 0 {
            return;
        }

        match self.values.last_mut() {
            Some((count, last_item)) if &item == last_item => {
                debug_assert!(*count > 0);
                *count += n
            }
            _ => self.values.push((n, item)),
        }
    }

    /// Pop n items off the stack
    /// If check_eq is Some, all popped elements must be equal to the specified item
    pub fn pop_n(&mut self, check_eq: Option<&T>, mut n: u64) -> Result<(), AbsStackError> {
        while n > 0 {
            let Some((count, last)) = self.values.last_mut() else {
                return Err(AbsStackError::EmptyStack)
            };
            debug_assert!(*count > 0);
            if let Some(check_eq_item) = check_eq {
                if last != check_eq_item {
                    return Err(AbsStackError::ElementNotEqual);
                }
            }
            match (*count).cmp(&n) {
                Ordering::Less | Ordering::Equal => {
                    n -= *count;
                    self.values.pop().unwrap();
                }
                Ordering::Greater => {
                    *count -= n;
                    n = 0;
                }
            }
        }
        Ok(())
    }
}

#[derive(Eq, PartialEq, PartialOrd, Ord, Clone, Copy, Debug)]
pub enum AbsStackError {
    ElementNotEqual,
    EmptyStack,
}

impl fmt::Display for AbsStackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AbsStackError::ElementNotEqual => {
                write!(f, "Popped element is not equal to specified item")
            }
            AbsStackError::EmptyStack => {
                write!(f, "Unexpected empty stack")
            }
        }
    }
}
