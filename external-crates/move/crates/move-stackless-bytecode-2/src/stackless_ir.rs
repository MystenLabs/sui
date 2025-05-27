#[derive(Debug, Clone)]
pub enum Instruction {
    Return(Vec<Operand>),
    Assign {
        lhs: Var,
        rhs: RValue,
    },
    Jump(Label),
    Branch {
        condition: Var,
        then_label: Label,
        else_label: Label,
    },
}

#[derive(Debug, Clone)]
pub enum Operand {
    Var(Var),
    Constant(Constant),
    Location(u8),
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
    ImmBorrowField,
    MutBorrowField,
    ReadRef,
    WriteRef,
    Pack,
}

pub type Var = usize;
pub type Label = usize;
pub type FunctionId = usize;
pub type PrimitiveOpId = usize;
