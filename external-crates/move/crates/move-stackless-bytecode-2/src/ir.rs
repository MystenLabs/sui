#[derive(Debug, Clone)]
pub enum LabelledInstruction {
    Instruction(Instruction),
    Label(Label),
}

#[derive(Debug, Clone)]
pub enum Instruction {
    Return(Operand),
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
}

#[derive(Debug, Clone)]
pub enum RValue {
    Call {
        function: FunctionId,
        args: Vec<Operand>,
    },
    Primitive {
        op: PrimitiveOp,
        args: Vec<Operand>,
    },
    Constant(Constant),
}

#[derive(Debug, Clone)]
pub enum Constant {
    Bool(bool),
    U8(u8),
    U64(u64),
    U128(u128),
    // Address(BigUint),
    ByteArray(Vec<u8>),
    Vector(Vec<Constant>),
    U16(u16),
    U32(u32),
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
}

#[derive(Debug, Clone)]
pub enum Var {
    Register(usize), // Temporary variable index
                     // Unused,          // Represents an unused variable
}

pub type Label = usize;
pub type FunctionId = usize;
pub type PrimitiveOpId = usize;
