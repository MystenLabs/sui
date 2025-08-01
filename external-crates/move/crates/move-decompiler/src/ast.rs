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

#[derive(Clone)]
pub enum Term {
    // TODO: This will eventually be removed as term structuring is completed.
    Untranslated(move_stackless_bytecode_2::stackless::ast::BasicBlock),
}

#[derive(Debug, Clone)]
pub enum Exp {
    Break,
    Continue,
    Block(Term),
    Loop(Box<Exp>),
    Seq(Vec<Exp>),
    While(Box<Exp>, Box<Exp>),
    IfElse(Box<Exp>, Box<Exp>, Box<Option<Exp>>),
    Switch(Box<Exp>, Vec<Exp>),
}

impl std::fmt::Display for Exp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Exp::Break => write!(f, "Break"),
            Exp::Continue => write!(f, "Continue"),
            Exp::Block(term) => write!(f, "Block({})", term),
            Exp::Loop(body) => write!(f, "Loop({})", body),
            Exp::Seq(seq) => write!(f, "Seq({:?})", seq),
            Exp::While(cond, body) => write!(f, "While({}, {})", cond, body),
            Exp::IfElse(cond, conseq, alt) => write!(f, "IfElse({}, {}, {:?})", cond, conseq, alt),
            Exp::Switch(term, cases) => write!(f, "Switch({}, {:?})", term, cases),
        }
    }
}

impl std::fmt::Display for Term {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Term::Untranslated(bb) => write!(f, "<Untranslated>"),
        }
    }
}

impl std::fmt::Debug for Term {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Term::Untranslated(bb) => write!(f, "<Untranslated>"),
        }
    }
}