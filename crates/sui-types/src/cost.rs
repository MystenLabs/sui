// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format::{
    Bytecode, ConstantPoolIndex, FieldHandleIndex, FieldInstantiationIndex, FunctionHandleIndex,
    FunctionInstantiationIndex, SignatureIndex, StructDefInstantiationIndex, StructDefinitionIndex,
};
use move_core_types::gas_schedule::GasCost;

// Bytecode, cost, and whether or not it's depends on size of operand
pub fn _bytecode_instruction_costs() -> Vec<(Bytecode, GasCost, bool)> {
    use Bytecode::*;
    return vec![
        //
        // Arith, Logical and Comparative Instrs
        //

        // Compute costs are all fairly same because the main task is in the stack manipulation
        // Memory cost is 0 since no additional memory used
        (Add, GasCost::new(2, 0), false),
        (Sub, GasCost::new(2, 0), false),
        (Mul, GasCost::new(2, 0), false),
        (Div, GasCost::new(2, 0), false),
        (Mod, GasCost::new(2, 0), false),
        (And, GasCost::new(2, 0), false),
        (Or, GasCost::new(2, 0), false),
        (Xor, GasCost::new(2, 0), false),
        (Shl, GasCost::new(2, 0), false),
        (Shr, GasCost::new(2, 0), false),
        (Ge, GasCost::new(2, 0), false),
        (Lt, GasCost::new(2, 0), false),
        (Le, GasCost::new(2, 0), false),
        (Eq, GasCost::new(2, 0), false),
        (Gt, GasCost::new(2, 0), false),
        (Neq, GasCost::new(2, 0), false),
        (BitAnd, GasCost::new(2, 0), false),
        (BitOr, GasCost::new(2, 0), false),
        // Cheaper because unary oper
        (Not, GasCost::new(1, 0), false),
        //
        // Loads
        //

        // Memory cost is linear in number of bytes since creates val on stack
        (LdU8(0), GasCost::new(1, 1), false),
        (LdU64(0), GasCost::new(1, 8), false),
        (LdU128(0), GasCost::new(1, 16), false),
        (LdFalse, GasCost::new(1, 1), false),
        (LdTrue, GasCost::new(1, 1), false),
        // Size of the constant varies. Both costs are scaled by size
        // LdConst is currently more expensive due to deserialization each time
        // See issue to fix perf: https://github.com/move-language/move/issues/325
        (LdConst(ConstantPoolIndex::new(0)), GasCost::new(3, 1), true),
        //
        // Pop
        //

        // No additional memory used
        (Pop, GasCost::new(1, 0), false),
        //
        // Vector Operations
        //
        (VecLen(SignatureIndex::new(0)), GasCost::new(2, 0), false),
        (
            VecPopBack(SignatureIndex::new(0)),
            GasCost::new(3, 0),
            false,
        ),
        // Swap depends on size?
        (
            VecPushBack(SignatureIndex::new(0)),
            GasCost::new(3, 0),
            true,
        ),
        // TODO: Do we scale by memory for swap?
        (VecSwap(SignatureIndex::new(0)), GasCost::new(2, 0), true),
        // TODO: This is actually a linear function (dependent on object size and vec length) with a small base constant but that's not easy to model yet
        (VecPack(SignatureIndex::new(0), 0), GasCost::new(7, 0), true),
        // Performs a copy
        (
            VecImmBorrow(SignatureIndex::new(0)),
            GasCost::new(3, 1),
            true,
        ),
        // Performs a copy
        (
            VecMutBorrow(SignatureIndex::new(0)),
            GasCost::new(3, 1),
            true,
        ),
        //
        // Pack/Unpack
        //
        (
            Unpack(StructDefinitionIndex::new(0)),
            GasCost::new(2, 0),
            true,
        ),
        (
            UnpackGeneric(StructDefInstantiationIndex::new(0)),
            GasCost::new(2, 0),
            true,
        ),
        (
            Pack(StructDefinitionIndex::new(0)),
            GasCost::new(4, 0),
            true,
        ),
        (
            PackGeneric(StructDefInstantiationIndex::new(0)),
            GasCost::new(4, 0),
            true,
        ),
        //
        // Borrow
        //
        // Performs a copy
        (MutBorrowLoc(0), GasCost::new(2, 1), true),
        (ImmBorrowLoc(0), GasCost::new(2, 1), true),
        (
            MutBorrowField(FieldHandleIndex::new(0)),
            GasCost::new(2, 1),
            true,
        ),
        (
            MutBorrowFieldGeneric(FieldInstantiationIndex::new(0)),
            GasCost::new(2, 1),
            true,
        ),
        (
            ImmBorrowField(FieldHandleIndex::new(0)),
            GasCost::new(2, 1),
            true,
        ),
        (
            ImmBorrowFieldGeneric(FieldInstantiationIndex::new(0)),
            GasCost::new(2, 1),
            true,
        ),
        //
        // Ref
        //
        (WriteRef, GasCost::new(2, 0), true),
        // Performs c aopy
        (ReadRef, GasCost::new(3, 1), true),
        // Same cost as NOP. No work is done
        (FreezeRef, GasCost::new(1, 1), false),
        //
        // Local access
        //

        // This performs a copy hence scales with size
        (CopyLoc(0), GasCost::new(3, 1), true),
        //
        // TODO
        // Values are approx
        (MoveLoc(0), GasCost::new(1, 1), false),
        (StLoc(0), GasCost::new(1, 1), false),
        (CastU8, GasCost::new(2, 1), true),
        (CastU64, GasCost::new(1, 1), true),
        (CastU128, GasCost::new(1, 1), true),
        (Branch(0), GasCost::new(1, 0), false),
        (BrFalse(0), GasCost::new(1, 0), false),
        (BrTrue(0), GasCost::new(1, 0), false),
        // We have to modify the Move code to truly calculate this
        // For now scale by size
        (
            Call(FunctionHandleIndex::new(0)),
            GasCost::new(100, 1),
            true,
        ),
        (
            CallGeneric(FunctionInstantiationIndex::new(0)),
            GasCost::new(582, 1),
            false,
        ),
        (Ret, GasCost::new(638, 1), false),
        (Nop, GasCost::new(1, 1), false),
        (Abort, GasCost::new(1, 1), false),
        //
        // Not supported section
        // These will not be implemented as they should never be triggered
        //

        // Not supported yet in Sui Move
        (
            Exists(StructDefinitionIndex::new(0)),
            GasCost::new(41, 1),
            false,
        ),
        (
            ExistsGeneric(StructDefInstantiationIndex::new(0)),
            GasCost::new(34, 1),
            false,
        ),
        (
            MoveTo(StructDefinitionIndex::new(0)),
            GasCost::new(13, 1),
            false,
        ),
        (
            MoveToGeneric(StructDefInstantiationIndex::new(0)),
            GasCost::new(27, 1),
            false,
        ),
        (
            MoveFrom(StructDefinitionIndex::new(0)),
            GasCost::new(459, 1),
            false,
        ),
        (
            MoveFromGeneric(StructDefInstantiationIndex::new(0)),
            GasCost::new(13, 1),
            false,
        ),
        (
            MutBorrowGlobal(StructDefinitionIndex::new(0)),
            GasCost::new(21, 1),
            false,
        ),
        (
            MutBorrowGlobalGeneric(StructDefInstantiationIndex::new(0)),
            GasCost::new(15, 1),
            false,
        ),
        (
            ImmBorrowGlobal(StructDefinitionIndex::new(0)),
            GasCost::new(23, 1),
            false,
        ),
        (
            ImmBorrowGlobalGeneric(StructDefInstantiationIndex::new(0)),
            GasCost::new(14, 1),
            false,
        ),
        // Not supported yet in Move
        (
            VecUnpack(SignatureIndex::new(0), 0),
            GasCost::new(572, 1),
            false,
        ),
    ];
}
