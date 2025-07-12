// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_model_2::{model::Model as Model2, source_kind::SourceKind};

use crate::stackless::ast;
// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

pub struct Context<'a, K: SourceKind> {
    pub var_counter: Counter,
    pub locals_counter: Counter,
    pub model: &'a Model2<K>,
    pub logical_stack: Vec<usize>,
    pub optimize: bool,
}

pub struct Counter {
    pub count: usize,
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl<'a, K: SourceKind> Context<'a, K> {
    pub fn new(model: &'a Model2<K>) -> Self {
        Self {
            var_counter: Counter::new(),
            locals_counter: Counter::new(),
            model,
            logical_stack: vec![],
            optimize: false,
        }
    }

    pub fn pop_register(&mut self) -> ast::RegId {
        self.logical_stack
            .pop()
            .expect("Popped a register and there was none")
    }

    pub fn push_register(&mut self) -> ast::RegId {
        let reg_id = self.var_counter.next();
        self.logical_stack.push(reg_id);
        reg_id
    }

    pub fn optimize(&mut self, value: bool) {
        self.optimize = value;
    }
}

impl Counter {
    pub fn new() -> Self {
        Self { count: 0 }
    }

    pub fn next(&mut self) -> usize {
        let curr = self.count;
        self.count += 1;
        curr
    }

    #[allow(unused)]
    pub fn prev(&mut self) -> usize {
        if self.count == 0 {
            panic!("Cannot decrement Counter below zero");
        }
        self.count -= 1;
        self.count
    }

    pub fn reset(&mut self) {
        self.count = 0;
    }

    #[allow(unused)]
    pub fn current(&self) -> usize {
        self.count
    }

    #[allow(unused)]
    pub fn last(&self) -> usize {
        self.count - 1
    }

    #[allow(unused)]
    pub fn set(&mut self, value: usize) {
        self.count = value;
    }

    #[allow(unused)]
    pub fn increment(&mut self) {
        self.count += 1;
    }

    #[allow(unused)]
    pub fn decrement(&mut self) {
        if self.count == 0 {
            panic!("Cannot decrement Counter below zero");
        }
        self.count -= 1;
    }
}

// -------------------------------------------------------------------------------------------------
// Default
// -------------------------------------------------------------------------------------------------

impl Default for Counter {
    fn default() -> Self {
        Self::new()
    }
}
