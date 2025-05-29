// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::utils::comma_separated;

use move_symbol_pool::Symbol;

use std::collections::BTreeMap;

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Package {
    pub name: Symbol,
    pub modules: BTreeMap<Symbol, Module>,
}

#[derive(Debug, Clone)]
pub struct Module {
    pub name: Symbol,
    pub functions: BTreeMap<Symbol, Function>,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: Symbol,
    // TODO: This should be a basic block map or similar
    pub instructions: Vec<Instruction>,
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct BasicBlock {
    label: Label,
    instructions: Vec<Instruction>,
}

#[derive(Debug, Clone)]
pub enum Instruction {
    Return(Vec<Var>),
    Assign {
        lhs: Vec<Var>,
        rhs: RValue,
    },
    Jump(Label),
    Branch {
        condition: Var,
        then_label: Label,
        else_label: Label,
    },
    NotImplemented(String),
}

#[derive(Debug, Clone)]
pub enum Operand {
    Var(Var),
    Constant(Constant),
    Immediate(Immediate),
}

#[derive(Debug, Clone)]
pub enum RValue {
    Call {
        function: FunctionId,
        args: Vec<Operand>,
    },
    Constant(Constant),
    Primitive {
        op: PrimitiveOp,
        args: Vec<Operand>,
    },
    Immediate(Immediate),
}

#[derive(Debug, Clone)]
pub enum Immediate {
    U64(u64),
    True,
    False, // TODO: The rest of these
}

#[derive(Debug, Clone)]
pub enum PrimitiveOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    And,
    Or,
    Not,
    Equal,
    NotEqual,
    LessThan,
    GreaterThan,
    LessThanOrEqual,
    GreaterThanOrEqual,
    MoveLoc,
    CopyLoc,
    StoreLoc,
    ImmBorrowField,
    MutBorrowField,
    ReadRef,
    WriteRef,
    Pack,
    // TODO: The rest of these
}

#[derive(Debug, Clone)]
pub enum Var {
    Local(usize), // Local from the original bytecode
    Register(usize), // Temporary variable index
                  // Unused,          // Represents an unused variable
}

pub type Label = usize;
pub type Constant = usize;
pub type FunctionId = usize;
pub type PrimitiveOpId = usize;

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

// -------------------------------------------------------------------------------------------------
// Display
// -------------------------------------------------------------------------------------------------

impl std::fmt::Display for Package {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Package: {}", self.name)?;
        for module in self.modules.values() {
            writeln!(f, "{}", module)?;
        }
        Ok(())
    }
}

impl std::fmt::Display for Module {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "  Module: {}", self.name)?;
        for function in self.functions.values() {
            writeln!(f, "{}", function)?;
        }
        Ok(())
    }
}

impl std::fmt::Display for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "    Function: {}", self.name)?;
        for (i, instr) in self.instructions.iter().enumerate() {
            writeln!(f, "      {}: {}", i, instr)?;
        }
        Ok(())
    }
}

impl std::fmt::Display for Instruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Instruction::Return(vars) => write!(f, "Return({})", comma_separated(vars)),
            Instruction::Assign { lhs, rhs } => write!(f, "{} = {}", comma_separated(lhs), rhs),
            Instruction::Jump(lbl) => write!(f, "Jump({lbl}"),
            Instruction::Branch {
                condition,
                then_label,
                else_label,
            } => write!(f, "Branch({condition}, {then_label}, {else_label}"),
            Instruction::NotImplemented(instr) => write!(f, "Unimplemented({instr})"),
        }
    }
}

impl std::fmt::Display for Operand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Operand::Var(var) => write!(f, "{}", var),
            Operand::Constant(constant) => write!(f, "Constant({})", constant),
            Operand::Immediate(val) => write!(f, "{}", val),
        }
    }
}

impl std::fmt::Display for Var {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Var::Local(ndx) => write!(f, "lcl_{}", ndx),
            Var::Register(ndx) => write!(f, "reg_{}", ndx),
        }
    }
}

impl std::fmt::Display for Immediate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Immediate::U64(n) => write!(f, "U64({n})"),
            Immediate::True => write!(f, "True"),
            Immediate::False => write!(f, "False"),
        }
    }
}

impl std::fmt::Display for RValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RValue::Call { function, args } => {
                write!(f, "Call({}, ", function)?;
                write!(f, "{}", comma_separated(args))?;
                write!(f, ")")
            }
            RValue::Constant(constant) => write!(f, "Constant({})", constant),
            RValue::Primitive { op, args } => write!(f, "{}({})", op, comma_separated(args)),
            RValue::Immediate(immediate) => write!(f, "{immediate}"),
        }
    }
}

impl std::fmt::Display for PrimitiveOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
