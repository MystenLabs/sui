// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format::ConstantPoolIndex;
use move_stackless_bytecode_2::stackless::ast::{RegId, Trivial};
use move_symbol_pool::Symbol;

use std::collections::BTreeMap;

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Module {
    pub name: Symbol,
    pub functions: BTreeMap<Symbol, Function>,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: Symbol,
    pub code: Exp,
}

#[derive(Debug, Clone)]
pub enum Term {
    // TODO: This will eventually be removed as term structuring is completed.
    Untranslated(move_stackless_bytecode_2::stackless::ast::BasicBlock),

    // Assignment operations
    Assign {
        target: Vec<RegId>,
        value: Trivial,
    },

    // Primitive operations (arithmetic, comparison, etc.)
    PrimitiveOp {
        op: String, // For now, use string representation
        args: Vec<Trivial>,
        result: Vec<RegId>,
    },

    // Data operations (pack, unpack, etc.)
    DataOp {
        op: String, // For now, use string representation
        args: Vec<Trivial>,
        result: Vec<RegId>,
    },

    // Function calls
    Call {
        function: Symbol,
        args: Vec<Trivial>,
        result: Vec<RegId>,
    },

    // Local variable operations
    LocalOp {
        op: String, // "copy", "move", "store", "borrow_mut", "borrow_imm"
        loc: usize,
        value: Option<Trivial>, // Some for store, None for others
        result: Vec<RegId>,
    },

    // Control flow and special operations
    Drop(RegId),
    Abort(Trivial),
    Return(Vec<Trivial>),

    // Constants
    Constant {
        // This could be changed in const index
        value: Vec<u8>,
        result: Vec<RegId>,
    },

    // No operation
    Nop,

    // Error/unhandled
    NotImplemented(String),
}

#[derive(Debug, Clone)]
pub enum Exp {
    Break,
    Continue,
    Block(Term),
    Loop(Box<Exp>),
    Seq(Vec<Exp>),
    While(Term, Box<Exp>),
    IfElse(Term, Box<Exp>, Box<Option<Exp>>),
    Switch(Term, Vec<Exp>),
}
