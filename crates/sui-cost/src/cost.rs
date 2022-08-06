// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use once_cell::sync::Lazy;

use move_binary_format::{
    file_format::{
        Bytecode, ConstantPoolIndex, FieldHandleIndex, FieldInstantiationIndex,
        FunctionHandleIndex, FunctionInstantiationIndex, SignatureIndex,
        StructDefInstantiationIndex, StructDefinitionIndex,
    },
    file_format_common::instruction_key,
};
use move_core_types::gas_schedule::CostTable;
use move_core_types::gas_schedule::GasCost;
use move_vm_types::gas_schedule::new_from_instructions;

// NOTE: all values in this file are subject to change

// Maximum number of events a call can emit
pub const MAX_NUM_EVENT_EMIT: u64 = 256;

// Maximum gas a TX can use
pub const MAX_TX_GAS: u64 = 1_000_000_000;

//
// Fixed costs: these are charged regardless of execution
//
// This is a flat fee
pub const BASE_TX_COST_FIXED: u64 = 1_000;
// This is charged per byte of the TX
pub const BASE_TX_COST_PER_BYTE: u64 = 10;

//
// Object access costs: These are for reading, writing, and verifying objects
//
// Cost to read an object per byte
pub const OBJ_ACCESS_COST_READ: u64 = 100;
// Cost to mutate an object per byte
pub const OBJ_ACCESS_COST_MUTATE: u64 = 100;
// Cost to delete an object per byte
pub const OBJ_ACCESS_COST_DELETE: u64 = 20;
// For checking locks. Charged per object
pub const OBJ_ACCESS_COST_VERIFY: u64 = 200;

//
// Object storage costs: These are for storing objects
//
// Cost to store an object per byte. This is refundable
pub const OBJ_DATA_COST_REFUNDABLE: u64 = 100;
// Cost to store metadata of objects per byte.
// This depends on the size of various fields including the effects
pub const OBJ_METADATA_COST_REFUNDABLE: u64 = 100;

//
// Consensus costs: costs for TXes that use shared object
//
// Flat cost for consensus transactions
pub const CONSENSUS_COST: u64 = 1_000;

//
// Package verification & publish cost: when publishing a package
//
// Flat cost
pub const PACKAGE_PUBLISH_COST: u64 = 1_000;

//
// Native function costs
//
// TODO: need to refactor native gas calculation so it is extensible. Currently we
// have hardcoded here the stdlib natives.
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
#[repr(u8)]
pub enum SuiNativeCostIndex {
    EVENT_EMIT = 0,

    OBJECT_BYTES_TO_ADDR = 1,
    OBJECT_BORROW_UUID = 2,
    OBJECT_DELETE_IMPL = 3,

    TRANSFER_TRANSFER_INTERNAL = 4,
    TRANSFER_FREEZE_OBJECT = 5,
    TRANSFER_SHARE_OBJECT = 6,

    TX_CONTEXT_DERIVE_ID = 7,
    TX_CONTEXT_NEW_SIGNER_FROM_ADDR = 8,
}

// Native costs are currently flat
// TODO recalibrate wrt bytecode costs
pub fn native_cost_schedule() -> Vec<GasCost> {
    use SuiNativeCostIndex as N;

    let mut native_table = vec![
        // This is artificially chosen to limit too many event emits
        // We will change this in https://github.com/MystenLabs/sui/issues/3341
        (
            N::EVENT_EMIT,
            GasCost::new(MAX_TX_GAS / MAX_NUM_EVENT_EMIT, 1),
        ),
        (N::OBJECT_BYTES_TO_ADDR, GasCost::new(30, 1)),
        (N::OBJECT_BORROW_UUID, GasCost::new(150, 1)),
        (N::OBJECT_DELETE_IMPL, GasCost::new(100, 1)),
        (N::TRANSFER_TRANSFER_INTERNAL, GasCost::new(80, 1)),
        (N::TRANSFER_FREEZE_OBJECT, GasCost::new(80, 1)),
        (N::TRANSFER_SHARE_OBJECT, GasCost::new(80, 1)),
        (N::TX_CONTEXT_DERIVE_ID, GasCost::new(110, 1)),
        (N::TX_CONTEXT_NEW_SIGNER_FROM_ADDR, GasCost::new(200, 1)),
    ];
    native_table.sort_by_key(|cost| cost.0 as u64);
    native_table
        .into_iter()
        .map(|(_, cost)| cost)
        .collect::<Vec<_>>()
}

pub fn bytecode_cost_schedule() -> Vec<(Bytecode, GasCost)> {
    let mut instrs: Vec<_> = bytecode_costs()
        .iter()
        .map(|q| (q.0.clone(), q.1.clone()))
        .collect();
    // Note that the DiemVM is expecting the table sorted by instruction order.
    instrs.sort_by_key(|cost| instruction_key(&cost.0));
    instrs
}
pub static COST_SCHEDULE: Lazy<CostTable> = Lazy::new(|| {
    // Note that the DiemVM is expecting the table sorted by instruction order.
    new_from_instructions(bytecode_cost_schedule(), native_cost_schedule())
});

//
// Bytecode cost tables
//
// Bytecode, cost, and whether or not it's depends on size of operand
fn bytecode_costs() -> Vec<(Bytecode, GasCost, bool)> {
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
