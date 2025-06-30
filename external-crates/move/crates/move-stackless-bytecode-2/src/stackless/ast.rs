// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::utils::comma_separated;

use move_binary_format::normalized::{Constant, FieldRef};
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
    pub entry_label: Label,
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
    Return(Vec<Trivial>),
    AssignReg {
        lhs: Vec<RegId>,
        rhs: RValue,
    },
    StoreLoc {
        loc: LocalId,
        value: Trivial,
    },
    Jump(Label),
    JumpIf {
        condition: Trivial,
        then_label: Label,
        else_label: Label,
    },
    Abort(Trivial),
    Nop,
    VariantSwitch {
        cases: Vec<Label>,
    },
    Drop(RegId), // Drop an operand in the case of a Pop operation
    NotImplemented(String),
}

#[derive(Debug, Clone)]
pub enum Trivial {
    Register(RegId),
    Immediate(Value),
}

#[derive(Debug, Clone)]
pub enum RValue {
    Call {
        function: Symbol,
        args: Vec<Trivial>,
    },
    Primitive {
        op: PrimitiveOp,
        args: Vec<Trivial>,
    },
    Data {
        op: DataOp,
        args: Vec<Trivial>,
    },
    Local {
        op: LocalOp,
        arg: LocalId,
    },
    Trivial(Trivial),
    Constant(std::rc::Rc<Constant<Symbol>>),
}

#[derive(Debug, Clone)]
pub enum LocalOp {
    Move,
    Copy,
    Borrow(Mutability),
}

#[derive(Debug, Clone)]
pub enum Mutability {
    Mutable,
    Immutable,
}

#[derive(Debug, Clone)]
pub enum PrimitiveOp {
    CastU8,
    CastU16,
    CastU32,
    CastU64,
    CastU128,
    CastU256,
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
    ShiftLeft,
    ShiftRight,
}

#[derive(Debug, Clone)]
pub enum DataOp {
    Pack,
    Unpack,
    ReadRef,
    WriteRef,
    FreezeRef,
    MutBorrowField(Box<FieldRef<Symbol>>),
    ImmBorrowField(Box<FieldRef<Symbol>>),
    VecPack,
    VecLen,
    VecImmBorrow,
    VecMutBorrow,
    VecPushBack,
    VecPopBack,
    VecUnpack,
    VecSwap,
    PackVariant,
    UnpackVariant,
    UnpackVariantImmRef,
    UnpackVariantMutRef,
}

#[derive(Debug, Clone)]
pub enum Value {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    U256(move_core_types::u256::U256), // Representing as two u128s for simplicity
    Bool(bool),
    Address(AccountAddress),
    Empty, // empty added for the pop
    NotImplemented(String),
    Vector(Vec<Value>), // Added to represent vector values
}

pub type Label = usize;
pub type RegId = usize;
pub type LocalId = usize;

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
        writeln!(
            f,
            "    Function: {} (entry: LBL_{})",
            self.name, self.entry_label
        )?;
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
            Instruction::Return(trivials) => write!(f, "Return({})", comma_separated(trivials)),
            Instruction::AssignReg { lhs, rhs } => write!(
                f,
                "{}{}{}",
                lhs.iter()
                    .map(|id| format!("reg_{id}"))
                    .collect::<Vec<_>>()
                    .join(", "),
                if lhs.is_empty() { "" } else { " = " },
                rhs
            ),
            Instruction::StoreLoc { loc, value } => {
                write!(f, "lcl_{loc} = {value}")
            }
            Instruction::Jump(lbl) => write!(f, "Jump(LBL_{lbl})"),
            Instruction::JumpIf {
                condition,
                then_label,
                else_label,
            } => write!(f, "JumpIf({condition}, LBL_{then_label}, LBL_{else_label})"),
            Instruction::Abort(trivial) => write!(f, "Abort({trivial})"),
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
            Instruction::Drop(reg_id) => write!(f, "Drop({reg_id})"),
            Instruction::Nop => write!(f, "NoOperation"),
            Instruction::NotImplemented(instr) => write!(f, "Unimplemented({instr})"),
        }
    }
}

impl std::fmt::Display for Trivial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Trivial::Register(reg_id) => write!(f, "reg_{reg_id}"),
            Trivial::Immediate(val) => write!(f, "Immediate({val})"),
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
            Value::Bool(bool) => write!(f, "{bool}"),
            Value::Empty => write!(f, "Empty"),
            Value::Address(addr) => write!(f, "Address({})", addr.to_canonical_string(true)),
            Value::NotImplemented(msg) => write!(f, "NotImplemented({})", msg),
            Value::Vector(vec) => write!(f, "Vector[{}]", comma_separated(vec)),
        }
    }
}

impl std::fmt::Display for LocalOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LocalOp::Move => write!(f, "Move"),
            LocalOp::Copy => write!(f, "Copy"),
            LocalOp::Borrow(mutability) => write!(f, "{}Borrow", mutability),
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
            RValue::Data { op, args } => write!(f, "{}({})", op, comma_separated(args)),
            RValue::Local { op, arg: loc } => write!(f, "{}(lcl_{})", op, loc),
            RValue::Trivial(trv) => write!(f, "{}", trv),
            RValue::Constant(constant) => write!(f, "Constant({:?})", constant),
        }
    }
}

impl std::fmt::Display for PrimitiveOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::fmt::Display for DataOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataOp::ImmBorrowField(_) => {
                write!(f, "ImmBorrowField")
            }
            DataOp::MutBorrowField(_) => {
                write!(f, "MutBorrowField")
            }
            _ => write!(f, "{:?}", self),
        }
    }
}

impl std::fmt::Display for Mutability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mutability::Mutable => write!(f, "Mut"),
            Mutability::Immutable => write!(f, "Imm"),
        }
    }
}
