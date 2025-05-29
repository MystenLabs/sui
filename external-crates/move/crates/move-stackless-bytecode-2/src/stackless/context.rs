// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

pub struct Context {
    pub var_counter: VarCounter,
}

pub struct VarCounter {
    pub count: usize,
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl Context {
    pub fn new() -> Self {
        Self {
            var_counter: VarCounter::new(),
        }
    }

    pub fn get_var_counter(&mut self) -> &mut VarCounter {
        &mut self.var_counter
    }
}

impl VarCounter {
    pub fn new() -> Self {
        Self { count: 0 }
    }

    pub fn next(&mut self) -> usize {
        let current = self.count;
        self.count += 1;
        current
    }

    pub fn prev(&mut self) -> usize {
        if self.count == 0 {
            panic!("Cannot decrement VarCounter below zero");
        }
        self.count -= 1;
        self.count
    }

    pub fn reset(&mut self) {
        self.count = 0;
    }

    pub fn current(&self) -> usize {
        self.count
    }

    pub fn last(&self) -> usize {
        if self.count == 0 {
            panic!("VarCounter is empty, cannot return last value");
        }
        self.count - 1
    }

    pub fn set(&mut self, value: usize) {
        self.count = value;
    }

    pub fn increment(&mut self) {
        self.count += 1;
    }

    pub fn decrement(&mut self) {
        if self.count == 0 {
            panic!("Cannot decrement VarCounter below zero");
        }
        self.count -= 1;
    }
}

// -------------------------------------------------------------------------------------------------
// Default
// -------------------------------------------------------------------------------------------------

impl Default for VarCounter {
    fn default() -> Self {
        Self::new()
    }
}
