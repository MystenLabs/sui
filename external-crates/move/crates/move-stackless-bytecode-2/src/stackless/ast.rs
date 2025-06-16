// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::utils::comma_separated;

use move_core_types::account_address::AccountAddress;
use move_symbol_pool::Symbol;

use std::{collections::BTreeMap, vec};

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Package {
    pub name: Option<Symbol>,
    pub address: AccountAddress,
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
    pub basic_blocks: BTreeMap<Label, BasicBlock>,
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct BasicBlock {
    pub label: Label,
    pub instructions: Vec<Instruction>,
}

#[derive(Debug, Clone)]
pub enum Instruction {
    // TODO check this
    Return(Vec<Operand>),
    Assign {
        lhs: Vec<Var>,
        rhs: RValue,
    },
    Jump(Label),
    JumpIf {
        condition: Var,
        then_label: Label,
        else_label: Label,
    },
    Abort,
    Nop,
    VariantSwitch {
        cases: Vec<Label>,
    },
    NotImplemented(String),
}

#[derive(Debug, Clone)]
pub enum Operand {
    Var(Var),
    Constant(Value),
    Immediate(Value),
}

#[derive(Debug, Clone)]
pub enum RValue {
    Call {
        function: Symbol,
        args: Vec<Operand>,
    },
    Primitive {
        op: PrimitiveOp,
        args: Vec<Operand>,
    },
    Operand(Operand),
}

#[derive(Debug, Clone)]
pub enum Value {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    U256(move_core_types::u256::U256), // Representing as two u128s for simplicity
    True,
    False,
    Address(AccountAddress),
    Empty, // empty added for the pop
    NotImplemented(String),
    Vector(Vec<Value>), // Added to represent vector values
}

#[derive(Debug, Clone)]
pub enum PrimitiveOp {
    CastU8,
    CastU64,
    CastU128,
    CopyLoc,
    MoveLoc,
    StoreLoc,
    Call,
    Pack,
    Unpack,
    ReadRef,
    WriteRef,
    FreezeRef,
    MutBorrowLoc,
    ImmBorrowLoc,
    MutBorrowField,
    ImmBorrowField,
    Add,
    Subtract,
    Multiply,
    Modulo,
    Divide,
    BitOr,
    BitAnd,
    Xor,
    Or,
    And,
    Not,
    Equal,
    NotEqual,
    LessThan,
    GreaterThan,
    LessThanOrEqual,
    GreaterThanOrEqual,
    Abort,
    NoOperation,
    ShiftLeft,
    ShiftRight,
    VecPack,
    VecLen,
    VecImmBorrow,
    VecMutBorrow,
    VecPushBack,
    VecPopBack,
    VecUnpack,
    VecSwap,
    CastU16,
    CastU32,
    CastU256,
    PackVariant,
    UnpackVariant,
    UnpackVariantImmRef,
    UnpackVariantMutRef,
    VariantSwitch,
    // MutBorrowGlobalDeprecated,
    // ImmBorrowGlobalDeprecated,
    // ExistsDeprecated,
    // MoveFromDeprecated,
    // MoveToDeprecated,
}

#[derive(Debug, Clone, Ord, Eq, PartialEq, PartialOrd)]
pub enum Var {
    Local(usize),    // Local from the original bytecode
    Register(usize), // Temporary variable index
}

pub type Label = usize;
pub type PrimitiveOpId = usize;

// -------------------------------------------------------------------------------------------------
// Impls
// -------------------------------------------------------------------------------------------------

impl BasicBlock {
    pub fn new(label: Label) -> Self {
        Self {
            label,
            instructions: vec![],
        }
    }

    pub fn from_instructions(label: Label, instructions: Vec<Instruction>) -> Self {
        Self {
            label,
            instructions,
        }
    }
}
// -------------------------------------------------------------------------------------------------
// Display
// -------------------------------------------------------------------------------------------------

impl std::fmt::Display for Package {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Package: {}",
            self.name.unwrap_or("Name not found".into())
        )?;
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
        for block in self.basic_blocks.values() {
            writeln!(f, "{}", block)?;
        }
        Ok(())
    }
}

impl std::fmt::Display for BasicBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "      Label LBL_{}:", self.label)?;
        for instr in &self.instructions {
            writeln!(f, "        {}", instr)?;
        }
        Ok(())
    }
}

impl std::fmt::Display for Instruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Instruction::Return(vars) => write!(f, "Return({})", comma_separated(vars)),
            Instruction::Assign { lhs, rhs } => write!(
                f,
                "{}{}{}",
                comma_separated(lhs),
                if lhs.is_empty() { "" } else { " = " },
                rhs
            ),
            Instruction::Jump(lbl) => write!(f, "Jump(LBL_{lbl})"),
            Instruction::JumpIf {
                condition,
                then_label,
                else_label,
            } => write!(f, "JumpIf({condition}, LBL_{then_label}, LBL_{else_label})"),
            Instruction::Abort => write!(f, "Abort"),
            Instruction::VariantSwitch { cases } => {
                write!(f, "VariantSwitch(")?;
                for (i, case) in cases.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "Label({case})")?;
                }
                write!(f, ")")
            }
            Instruction::Nop => write!(f, "NoOperation"),
            Instruction::NotImplemented(instr) => write!(f, "Unimplemented({instr})"),
        }
    }
}

impl std::fmt::Display for Operand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Operand::Var(var) => write!(f, "{}", var),
            Operand::Constant(constant) => write!(f, "{}", constant),
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

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::U8(n) => write!(f, "U8({n})"),
            Value::U16(n) => write!(f, "U16({n})"),
            Value::U32(n) => write!(f, "U32({n})"),
            Value::U64(n) => write!(f, "U64({n})"),
            Value::U128(n) => write!(f, "U128({n})"),
            Value::U256(n) => write!(f, "U256({n})"),
            Value::True => write!(f, "True"),
            Value::False => write!(f, "False"),
            Value::Empty => write!(f, "Empty"),
            Value::Address(addr) => write!(f, "Address({})", addr.to_canonical_string(true)),
            Value::NotImplemented(msg) => write!(f, "NotImplemented({})", msg),
            Value::Vector(vec) => write!(f, "Vector[{}]", comma_separated(vec)),
        }
    }
}

impl std::fmt::Display for RValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RValue::Call { function, args } => {
                write!(f, "Call {}(", function)?;
                write!(f, "{}", comma_separated(args))?;
                write!(f, ")")
            }
            // RValue::Constant(constant) => write!(f, "Constant {}", constant),
            RValue::Primitive { op, args } => write!(f, "{}({})", op, comma_separated(args)),
            RValue::Operand(op) => match op {
                Operand::Var(var) => write!(f, "Var({})", var),
                Operand::Constant(value) => write!(f, "Constant({})", value),
                Operand::Immediate(immediate) => write!(f, "Immediate({})", immediate),
            },
        }
    }
}

impl std::fmt::Display for PrimitiveOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
