// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

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
    // TODO: This will eventually be removed as term structuring is copleted.
    Untranslated(move_stackless_bytecode_2::stackless::ast::BasicBlock),
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
