// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This file implements the expression evaluation part of the stackless bytecode interpreter.

use num::BigInt;
use std::collections::BTreeMap;

use crate::concrete::value::BaseValue;

//**************************************************************************************************
// Types
//**************************************************************************************************

pub type EvalResult<T> = ::std::result::Result<T, BigInt>;

//**************************************************************************************************
// Evaluation context
//**************************************************************************************************

#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct ExpState {
    // bindings for the local variables
    local_vars: BTreeMap<String, BaseValue>,
}

impl ExpState {
    pub fn add_var(&mut self, name: String, val: BaseValue) {
        let exists = self.local_vars.insert(name, val);
        if cfg!(debug_assertions) {
            assert!(exists.is_none());
        }
    }

    pub fn get_var(&self, name: &str) -> BaseValue {
        self.local_vars.get(name).unwrap().clone()
    }
}
