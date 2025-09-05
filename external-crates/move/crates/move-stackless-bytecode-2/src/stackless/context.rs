// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::normalized::Type;
use move_model_2::{model::Model as Model2, source_kind::SourceKind};
use move_symbol_pool::Symbol;
use std::rc::Rc;

use crate::stackless::ast::Register;
// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

pub struct Context<'a, K: SourceKind> {
    pub var_counter: Counter,
    pub model: &'a Model2<K>,
    pub logical_stack: Vec<Register>,
    pub optimize: bool,
    pub locals_types: Vec<Rc<Type<Symbol>>>,
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
            model,
            logical_stack: vec![],
            optimize: false,
            locals_types: vec![],
        }
    }

    pub fn pop_register(&mut self) -> Register {
        self.logical_stack
            .pop()
            .expect("Popped a register and there was none")
    }

    pub fn push_register(&mut self, ty: Rc<Type<Symbol>>) -> Register {
        let reg_id = self.var_counter.next();
        let new_reg = Register { name: reg_id, ty };
        self.logical_stack.push(new_reg.clone());
        new_reg
    }

    pub fn nth_register(&self, n: usize) -> &Register {
        self.logical_stack
            .get(self.logical_stack.len() - n)
            .expect("Tried to get nth register but stack is too small")
    }

    pub fn optimize(&mut self, value: bool) {
        self.optimize = value;
    }

    pub fn set_locals_types(&mut self, locals_types: Vec<Rc<Type<Symbol>>>) {
        self.locals_types = locals_types;
    }

    pub fn get_local_type(&self, loc: usize) -> &Rc<Type<Symbol>> {
        &self.locals_types[loc]
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

    pub fn reset(&mut self) {
        self.count = 0;
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
