// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------
// This holds a configuration for the decompiler.

pub struct Config {
    pub debug_print: DebugPrintFlags,
}

pub struct DebugPrintFlags {
    pub control_flow_graph: bool,
    pub decompiled_code: bool,
    pub input: bool,
    pub stackless: bool,
    pub structured: bool,
    pub structuring: bool,
    pub dominators: bool,
}

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl Config {
    pub fn new(debug: DebugPrintFlags) -> Self {
        Self { debug_print: debug }
    }
}

impl DebugPrintFlags {
    pub fn print_function_heading(&self) -> bool {
        self.input
            || self.control_flow_graph
            || self.decompiled_code
            || self.stackless
            || self.structured
            || self.structuring
    }
}

// -------------------------------------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------------------------------------

pub fn print_heading(word: &str) {
    println!(
        "== {word} {}",
        "=".repeat(60 - (word.len() + 4)) // 4 = "== " + " "
    );
}

// -------------------------------------------------------------------------------------------------
// Default Impls
// -------------------------------------------------------------------------------------------------

#[allow(clippy::derivable_impls)]
impl Default for Config {
    fn default() -> Self {
        Self {
            debug_print: DebugPrintFlags::default(),
        }
    }
}

#[allow(clippy::derivable_impls)]
impl Default for DebugPrintFlags {
    fn default() -> Self {
        Self {
            stackless: false,
            input: false,
            structured: false,
            decompiled_code: false,
            control_flow_graph: false,
            structuring: false,
            dominators: false,
        }
    }
}
