// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::parser::lexer::{Tok, TOK_COUNT};

#[derive(Clone, Debug)]
pub struct FollowSet(Vec<u32>);

#[allow(dead_code)]
impl FollowSet {
    pub fn new() -> Self {
        let values = vec![0; TOK_COUNT];
        FollowSet(values)
    }

    pub fn add(&mut self, tok: Tok) {
        self.0[tok as usize] += 1;
    }

    pub fn remove(&mut self, tok: Tok) {
        if self.0[tok as usize] > 0 {
            self.0[tok as usize] -= 1;
        }
    }

    pub fn add_all(&mut self, toks: &[Tok]) {
        for tok in toks {
            self.add(*tok);
        }
    }

    pub fn remove_all(&mut self, toks: &[Tok]) {
        for tok in toks {
            self.remove(*tok);
        }
    }

    pub fn contains(&self, tok: Tok) -> bool {
        self.0[tok as usize] > 0
    }

    pub fn contains_any(&self, toks: &[Tok]) -> bool {
        toks.iter().any(|tok| self.contains(*tok))
    }

    pub fn union(&mut self, other: &FollowSet) {
        for (target, n) in self.0.iter_mut().zip(other.0.iter()) {
            *target += n;
        }
    }

    pub fn difference(&mut self, other: &FollowSet) {
        for (target, n) in self.0.iter_mut().zip(other.0.iter()) {
            *target -= n;
        }
    }
}

impl<const N: usize> std::convert::From<&[Tok; N]> for FollowSet {
    fn from(values: &[Tok; N]) -> Self {
        let mut new = FollowSet::new();
        new.add_all(values);
        new
    }
}

impl std::convert::From<&[Tok]> for FollowSet {
    fn from(values: &[Tok]) -> Self {
        let mut new = FollowSet::new();
        new.add_all(values);
        new
    }
}
