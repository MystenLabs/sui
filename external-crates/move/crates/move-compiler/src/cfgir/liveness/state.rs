// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//**************************************************************************************************
// Abstract state
//**************************************************************************************************

use crate::{cfgir::absint::*, hlir::ast::Var};
use std::{cmp::Ordering, collections::BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LivenessState {
    pub live_set: BTreeSet<Var>,
}

//**************************************************************************************************
// impls
//**************************************************************************************************

impl LivenessState {
    pub fn initial() -> Self {
        LivenessState {
            live_set: BTreeSet::new(),
        }
    }

    pub fn extend(&mut self, other: &Self) {
        self.live_set.extend(other.live_set.iter().cloned());
    }
}

impl AbstractDomain for LivenessState {
    fn join(&mut self, other: &Self) -> JoinResult {
        let before = self.live_set.len();
        self.extend(other);
        let after = self.live_set.len();
        match before.cmp(&after) {
            Ordering::Less => JoinResult::Changed,
            Ordering::Equal => JoinResult::Unchanged,
            Ordering::Greater => panic!("ICE set union made a set smaller than before"),
        }
    }
}
