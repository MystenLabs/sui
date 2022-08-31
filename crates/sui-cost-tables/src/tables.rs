// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::{Add, Mul};

use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_binary_format::file_format_common::Opcodes::{self};
use move_core_types::gas_algebra::{
    AbstractMemorySize, GasQuantity, InternalGas, InternalGasPerAbstractMemoryUnit,
    InternalGasUnit, NumArgs, NumBytes, ToUnit, ToUnitFractional,
};
use move_core_types::language_storage::ModuleId;
use move_core_types::vm_status::StatusCode;
use move_vm_types::gas::{GasMeter, SimpleInstruction};
use move_vm_types::views::{TypeView, ValueView};
use once_cell::sync::Lazy;

use move_binary_format::{
    file_format::{
        Bytecode, ConstantPoolIndex, FieldHandleIndex, FieldInstantiationIndex,
        FunctionHandleIndex, FunctionInstantiationIndex, SignatureIndex,
        StructDefInstantiationIndex, StructDefinitionIndex,
    },
    file_format_common::instruction_key,
};
use serde::{Deserialize, Serialize};

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
pub fn _native_cost_schedule() -> Vec<GasCost> {
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

pub enum GasUnit {}

pub type Gas = GasQuantity<GasUnit>;

impl ToUnit<InternalGasUnit> for GasUnit {
    const MULTIPLIER: u64 = 1000;
}

impl ToUnitFractional<GasUnit> for InternalGasUnit {
    const NOMINATOR: u64 = 1;
    const DENOMINATOR: u64 = 1000;
}

/// The size in bytes for a non-string or address constant on the stack
pub const CONST_SIZE: AbstractMemorySize = AbstractMemorySize::new(16);

/// The size in bytes for a reference on the stack
pub const REFERENCE_SIZE: AbstractMemorySize = AbstractMemorySize::new(8);

/// The size of a struct in bytes
pub const STRUCT_SIZE: AbstractMemorySize = AbstractMemorySize::new(2);

/// For exists checks on data that doesn't exists this is the multiplier that is used.
pub const MIN_EXISTS_DATA_SIZE: AbstractMemorySize = AbstractMemorySize::new(100);

/// The cost tables, keyed by the serialized form of the bytecode instruction.  We use the
/// serialized form as opposed to the instruction enum itself as the key since this will be the
/// on-chain representation of bytecode instructions in the future.
#[derive(Clone, Debug, Serialize, PartialEq, Eq, Deserialize)]
pub struct CostTable {
    pub instruction_table: Vec<GasCost>,
}

impl CostTable {
    #[inline]
    pub fn instruction_cost(&self, instr_index: u8) -> &GasCost {
        debug_assert!(instr_index > 0 && instr_index <= (self.instruction_table.len() as u8));
        &self.instruction_table[(instr_index - 1) as usize]
    }
}

/// The  `GasCost` tracks:
/// - instruction cost: how much time/computational power is needed to perform the instruction
/// - memory cost: how much memory is required for the instruction, and storage overhead
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GasCost {
    pub instruction_gas: u64,
    pub memory_gas: u64,
}

impl GasCost {
    pub fn new(instruction_gas: u64, memory_gas: u64) -> Self {
        Self {
            instruction_gas,
            memory_gas,
        }
    }

    /// Convert a GasCost to a total gas charge in `InternalGas`.
    #[inline]
    pub fn total(&self) -> u64 {
        self.instruction_gas.add(self.memory_gas)
    }
}

static ZERO_COST_SCHEDULE: Lazy<CostTable> = Lazy::new(zero_cost_schedule);

/// The Move VM implementation of state for gas metering.
///
/// Initialize with a `CostTable` and the gas provided to the transaction.
/// Provide all the proper guarantees about gas metering in the Move VM.
///
/// Every client must use an instance of this type to interact with the Move VM.
pub struct GasStatus<'a> {
    cost_table: &'a CostTable,
    gas_left: InternalGas,
    charge: bool,
}

impl<'a> GasStatus<'a> {
    /// Initialize the gas state with metering enabled.
    ///
    /// Charge for every operation and fail when there is no more gas to pay for operations.
    /// This is the instantiation that must be used when executing a user script.
    pub fn new(cost_table: &'a CostTable, gas_left: Gas) -> Self {
        Self {
            gas_left: gas_left.to_unit(),
            cost_table,
            charge: true,
        }
    }

    /// Initialize the gas state with metering disabled.
    ///
    /// It should be used by clients in very specific cases and when executing system
    /// code that does not have to charge the user.
    pub fn new_unmetered() -> Self {
        Self {
            gas_left: InternalGas::new(0),
            cost_table: &ZERO_COST_SCHEDULE,
            charge: false,
        }
    }

    /// Return the `CostTable` behind this `GasStatus`.
    pub fn cost_table(&self) -> &CostTable {
        self.cost_table
    }

    /// Return the gas left.
    pub fn remaining_gas(&self) -> Gas {
        self.gas_left.to_unit_round_down()
    }

    /// Charge a given amount of gas and fail if not enough gas units are left.
    pub fn deduct_gas(&mut self, amount: InternalGas) -> PartialVMResult<()> {
        if !self.charge {
            return Ok(());
        }

        match self.gas_left.checked_sub(amount) {
            Some(gas_left) => {
                self.gas_left = gas_left;
                Ok(())
            }
            None => {
                self.gas_left = InternalGas::new(0);
                Err(PartialVMError::new(StatusCode::OUT_OF_GAS))
            }
        }
    }

    fn charge_instr(&mut self, opcode: Opcodes) -> PartialVMResult<()> {
        self.deduct_gas(
            self.cost_table
                .instruction_cost(opcode as u8)
                .total()
                .into(),
        )
    }

    /// Charge an instruction over data with a given size and fail if not enough gas units are left.
    fn charge_instr_with_size(
        &mut self,
        opcode: Opcodes,
        size: AbstractMemorySize,
    ) -> PartialVMResult<()> {
        // Make sure that the size is always non-zero
        let size = std::cmp::max(1.into(), size);
        debug_assert!(size > 0.into());
        self.deduct_gas(
            InternalGasPerAbstractMemoryUnit::new(
                self.cost_table.instruction_cost(opcode as u8).total(),
            )
            .mul(size),
        )
    }

    pub fn set_metering(&mut self, enabled: bool) {
        self.charge = enabled
    }
}

fn get_simple_instruction_opcode(instr: SimpleInstruction) -> Opcodes {
    use Opcodes::*;
    use SimpleInstruction::*;

    match instr {
        Nop => NOP,
        Ret => RET,

        BrTrue => BR_TRUE,
        BrFalse => BR_FALSE,
        Branch => BRANCH,

        Pop => POP,
        LdU8 => LD_U8,
        LdU64 => LD_U64,
        LdU128 => LD_U128,
        LdTrue => LD_TRUE,
        LdFalse => LD_FALSE,

        FreezeRef => FREEZE_REF,
        MutBorrowLoc => MUT_BORROW_LOC,
        ImmBorrowLoc => IMM_BORROW_LOC,
        ImmBorrowField => IMM_BORROW_FIELD,
        MutBorrowField => MUT_BORROW_FIELD,
        ImmBorrowFieldGeneric => IMM_BORROW_FIELD_GENERIC,
        MutBorrowFieldGeneric => MUT_BORROW_FIELD_GENERIC,

        CastU8 => CAST_U8,
        CastU64 => CAST_U64,
        CastU128 => CAST_U128,

        Add => ADD,
        Sub => SUB,
        Mul => MUL,
        Mod => MOD,
        Div => DIV,

        BitOr => BIT_OR,
        BitAnd => BIT_AND,
        Xor => XOR,
        Shl => SHL,
        Shr => SHR,

        Or => OR,
        And => AND,
        Not => NOT,

        Lt => LT,
        Gt => GT,
        Le => LE,
        Ge => GE,

        Abort => ABORT,
    }
}

impl<'b> GasMeter for GasStatus<'b> {
    /// Charge an instruction and fail if not enough gas units are left.
    fn charge_simple_instr(&mut self, instr: SimpleInstruction) -> PartialVMResult<()> {
        self.charge_instr(get_simple_instruction_opcode(instr))
    }

    fn charge_native_function(&mut self, amount: InternalGas) -> PartialVMResult<()> {
        self.deduct_gas(amount)
    }

    fn charge_call(
        &mut self,
        _module_id: &ModuleId,
        _func_name: &str,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        self.charge_instr_with_size(Opcodes::CALL, (args.len() as u64 + 1).into())
    }

    fn charge_call_generic(
        &mut self,
        _module_id: &ModuleId,
        _func_name: &str,
        ty_args: impl ExactSizeIterator<Item = impl TypeView>,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        self.charge_instr_with_size(
            Opcodes::CALL_GENERIC,
            ((ty_args.len() + args.len() + 1) as u64).into(),
        )
    }

    fn charge_ld_const(&mut self, size: NumBytes) -> PartialVMResult<()> {
        self.charge_instr_with_size(Opcodes::LD_CONST, u64::from(size).into())
    }

    fn charge_copy_loc(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        self.charge_instr_with_size(Opcodes::COPY_LOC, val.legacy_abstract_memory_size())
    }

    fn charge_move_loc(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        self.charge_instr_with_size(Opcodes::MOVE_LOC, val.legacy_abstract_memory_size())
    }

    fn charge_store_loc(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        self.charge_instr_with_size(Opcodes::ST_LOC, val.legacy_abstract_memory_size())
    }

    fn charge_pack(
        &mut self,
        is_generic: bool,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        let field_count = AbstractMemorySize::new(args.len() as u64);
        self.charge_instr_with_size(
            if is_generic {
                Opcodes::PACK_GENERIC
            } else {
                Opcodes::PACK
            },
            args.fold(field_count, |acc, val| {
                acc + val.legacy_abstract_memory_size()
            }),
        )
    }

    fn charge_unpack(
        &mut self,
        is_generic: bool,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        let field_count = AbstractMemorySize::new(args.len() as u64);
        self.charge_instr_with_size(
            if is_generic {
                Opcodes::UNPACK_GENERIC
            } else {
                Opcodes::UNPACK
            },
            args.fold(field_count, |acc, val| {
                acc + val.legacy_abstract_memory_size()
            }),
        )
    }

    fn charge_read_ref(&mut self, ref_val: impl ValueView) -> PartialVMResult<()> {
        self.charge_instr_with_size(Opcodes::READ_REF, ref_val.legacy_abstract_memory_size())
    }

    fn charge_write_ref(&mut self, val: impl ValueView) -> PartialVMResult<()> {
        self.charge_instr_with_size(Opcodes::WRITE_REF, val.legacy_abstract_memory_size())
    }

    fn charge_eq(&mut self, lhs: impl ValueView, rhs: impl ValueView) -> PartialVMResult<()> {
        self.charge_instr_with_size(
            Opcodes::EQ,
            lhs.legacy_abstract_memory_size() + rhs.legacy_abstract_memory_size(),
        )
    }

    fn charge_neq(&mut self, lhs: impl ValueView, rhs: impl ValueView) -> PartialVMResult<()> {
        self.charge_instr_with_size(
            Opcodes::NEQ,
            lhs.legacy_abstract_memory_size() + rhs.legacy_abstract_memory_size(),
        )
    }

    fn charge_load_resource(&mut self, _loaded: Option<NumBytes>) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_borrow_global(
        &mut self,
        _is_mut: bool,
        _is_generic: bool,
        _ty: impl TypeView,
        _is_success: bool,
    ) -> PartialVMResult<()> {
        Err(PartialVMError::new(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
        ))
    }

    fn charge_exists(
        &mut self,
        _is_generic: bool,
        _ty: impl TypeView,
        _exists: bool,
    ) -> PartialVMResult<()> {
        Err(PartialVMError::new(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
        ))
    }

    fn charge_move_from(
        &mut self,
        _is_generic: bool,
        _ty: impl TypeView,
        _val: Option<impl ValueView>,
    ) -> PartialVMResult<()> {
        Err(PartialVMError::new(
            StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
        ))
    }

    fn charge_move_to(
        &mut self,
        is_generic: bool,
        _ty: impl TypeView,
        val: impl ValueView,
        is_success: bool,
    ) -> PartialVMResult<()> {
        use Opcodes::*;

        let op = if is_generic { MOVE_TO_GENERIC } else { MOVE_TO };

        if is_success {
            self.charge_instr_with_size(op, val.legacy_abstract_memory_size())?;
        }

        Ok(())
    }

    fn charge_vec_pack<'a>(
        &mut self,
        _ty: impl TypeView + 'a,
        args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        self.charge_instr_with_size(Opcodes::VEC_PACK, (args.len() as u64).into())
    }

    fn charge_vec_len(&mut self, _ty: impl TypeView) -> PartialVMResult<()> {
        self.charge_instr(Opcodes::VEC_LEN)
    }

    fn charge_vec_borrow(
        &mut self,
        is_mut: bool,
        _ty: impl TypeView,
        _is_success: bool,
    ) -> PartialVMResult<()> {
        use Opcodes::*;

        self.charge_instr(if is_mut {
            VEC_MUT_BORROW
        } else {
            VEC_IMM_BORROW
        })
    }

    fn charge_vec_push_back(
        &mut self,
        _ty: impl TypeView,
        val: impl ValueView,
    ) -> PartialVMResult<()> {
        self.charge_instr_with_size(Opcodes::VEC_PUSH_BACK, val.legacy_abstract_memory_size())
    }

    fn charge_vec_pop_back(
        &mut self,
        _ty: impl TypeView,
        _val: Option<impl ValueView>,
    ) -> PartialVMResult<()> {
        self.charge_instr(Opcodes::VEC_POP_BACK)
    }

    fn charge_vec_unpack(
        &mut self,
        _ty: impl TypeView,
        expect_num_elements: NumArgs,
    ) -> PartialVMResult<()> {
        self.charge_instr_with_size(
            Opcodes::VEC_PUSH_BACK,
            u64::from(expect_num_elements).into(),
        )
    }

    fn charge_vec_swap(&mut self, _ty: impl TypeView) -> PartialVMResult<()> {
        self.charge_instr(Opcodes::VEC_SWAP)
    }
}

pub fn new_from_instructions(mut instrs: Vec<(Bytecode, GasCost)>) -> CostTable {
    instrs.sort_by_key(|cost| instruction_key(&cost.0));

    if cfg!(debug_assertions) {
        let mut instructions_covered = 0;
        for (index, (instr, _)) in instrs.iter().enumerate() {
            let key = instruction_key(instr);
            if index == (key - 1) as usize {
                instructions_covered += 1;
            }
        }
        debug_assert!(
            instructions_covered == Bytecode::VARIANT_COUNT,
            "all instructions must be in the cost table"
        );
    }
    let instruction_table = instrs
        .into_iter()
        .map(|(_, cost)| cost)
        .collect::<Vec<GasCost>>();
    CostTable { instruction_table }
}

pub fn zero_cost_instruction_table() -> Vec<(Bytecode, GasCost)> {
    use Bytecode::*;

    vec![
        (MoveTo(StructDefinitionIndex::new(0)), GasCost::new(0, 0)),
        (
            MoveToGeneric(StructDefInstantiationIndex::new(0)),
            GasCost::new(0, 0),
        ),
        (MoveFrom(StructDefinitionIndex::new(0)), GasCost::new(0, 0)),
        (
            MoveFromGeneric(StructDefInstantiationIndex::new(0)),
            GasCost::new(0, 0),
        ),
        (BrTrue(0), GasCost::new(0, 0)),
        (WriteRef, GasCost::new(0, 0)),
        (Mul, GasCost::new(0, 0)),
        (MoveLoc(0), GasCost::new(0, 0)),
        (And, GasCost::new(0, 0)),
        (Pop, GasCost::new(0, 0)),
        (BitAnd, GasCost::new(0, 0)),
        (ReadRef, GasCost::new(0, 0)),
        (Sub, GasCost::new(0, 0)),
        (MutBorrowField(FieldHandleIndex::new(0)), GasCost::new(0, 0)),
        (
            MutBorrowFieldGeneric(FieldInstantiationIndex::new(0)),
            GasCost::new(0, 0),
        ),
        (ImmBorrowField(FieldHandleIndex::new(0)), GasCost::new(0, 0)),
        (
            ImmBorrowFieldGeneric(FieldInstantiationIndex::new(0)),
            GasCost::new(0, 0),
        ),
        (Add, GasCost::new(0, 0)),
        (CopyLoc(0), GasCost::new(0, 0)),
        (StLoc(0), GasCost::new(0, 0)),
        (Ret, GasCost::new(0, 0)),
        (Lt, GasCost::new(0, 0)),
        (LdU8(0), GasCost::new(0, 0)),
        (LdU64(0), GasCost::new(0, 0)),
        (LdU128(0), GasCost::new(0, 0)),
        (CastU8, GasCost::new(0, 0)),
        (CastU64, GasCost::new(0, 0)),
        (CastU128, GasCost::new(0, 0)),
        (Abort, GasCost::new(0, 0)),
        (MutBorrowLoc(0), GasCost::new(0, 0)),
        (ImmBorrowLoc(0), GasCost::new(0, 0)),
        (LdConst(ConstantPoolIndex::new(0)), GasCost::new(0, 0)),
        (Ge, GasCost::new(0, 0)),
        (Xor, GasCost::new(0, 0)),
        (Shl, GasCost::new(0, 0)),
        (Shr, GasCost::new(0, 0)),
        (Neq, GasCost::new(0, 0)),
        (Not, GasCost::new(0, 0)),
        (Call(FunctionHandleIndex::new(0)), GasCost::new(0, 0)),
        (
            CallGeneric(FunctionInstantiationIndex::new(0)),
            GasCost::new(0, 0),
        ),
        (Le, GasCost::new(0, 0)),
        (Branch(0), GasCost::new(0, 0)),
        (Unpack(StructDefinitionIndex::new(0)), GasCost::new(0, 0)),
        (
            UnpackGeneric(StructDefInstantiationIndex::new(0)),
            GasCost::new(0, 0),
        ),
        (Or, GasCost::new(0, 0)),
        (LdFalse, GasCost::new(0, 0)),
        (LdTrue, GasCost::new(0, 0)),
        (Mod, GasCost::new(0, 0)),
        (BrFalse(0), GasCost::new(0, 0)),
        (Exists(StructDefinitionIndex::new(0)), GasCost::new(0, 0)),
        (
            ExistsGeneric(StructDefInstantiationIndex::new(0)),
            GasCost::new(0, 0),
        ),
        (BitOr, GasCost::new(0, 0)),
        (FreezeRef, GasCost::new(0, 0)),
        (
            MutBorrowGlobal(StructDefinitionIndex::new(0)),
            GasCost::new(0, 0),
        ),
        (
            MutBorrowGlobalGeneric(StructDefInstantiationIndex::new(0)),
            GasCost::new(0, 0),
        ),
        (
            ImmBorrowGlobal(StructDefinitionIndex::new(0)),
            GasCost::new(0, 0),
        ),
        (
            ImmBorrowGlobalGeneric(StructDefInstantiationIndex::new(0)),
            GasCost::new(0, 0),
        ),
        (Div, GasCost::new(0, 0)),
        (Eq, GasCost::new(0, 0)),
        (Gt, GasCost::new(0, 0)),
        (Pack(StructDefinitionIndex::new(0)), GasCost::new(0, 0)),
        (
            PackGeneric(StructDefInstantiationIndex::new(0)),
            GasCost::new(0, 0),
        ),
        (Nop, GasCost::new(0, 0)),
        (VecPack(SignatureIndex::new(0), 0), GasCost::new(0, 0)),
        (VecLen(SignatureIndex::new(0)), GasCost::new(0, 0)),
        (VecImmBorrow(SignatureIndex::new(0)), GasCost::new(0, 0)),
        (VecMutBorrow(SignatureIndex::new(0)), GasCost::new(0, 0)),
        (VecPushBack(SignatureIndex::new(0)), GasCost::new(0, 0)),
        (VecPopBack(SignatureIndex::new(0)), GasCost::new(0, 0)),
        (VecUnpack(SignatureIndex::new(0), 0), GasCost::new(0, 0)),
        (VecSwap(SignatureIndex::new(0)), GasCost::new(0, 0)),
    ]
}

// Only used for genesis and for tests where we need a cost table and
// don't have a genesis storage state.
pub fn zero_cost_schedule() -> CostTable {
    // The actual costs for the instructions in this table _DO NOT MATTER_. This is only used
    // for genesis and testing, and for these cases we don't need to worry
    // about the actual gas for instructions.  The only thing we care about is having an entry
    // in the gas schedule for each instruction.
    let instrs = zero_cost_instruction_table();
    new_from_instructions(instrs)
}

pub fn legacy_bytecode_instruction_costs() -> Vec<(Bytecode, GasCost)> {
    use Bytecode::*;
    vec![
        (MoveTo(StructDefinitionIndex::new(0)), GasCost::new(13, 1)),
        (
            MoveToGeneric(StructDefInstantiationIndex::new(0)),
            GasCost::new(27, 1),
        ),
        (
            MoveFrom(StructDefinitionIndex::new(0)),
            GasCost::new(459, 1),
        ),
        (
            MoveFromGeneric(StructDefInstantiationIndex::new(0)),
            GasCost::new(13, 1),
        ),
        (BrTrue(0), GasCost::new(1, 1)),
        (WriteRef, GasCost::new(1, 1)),
        (Mul, GasCost::new(1, 1)),
        (MoveLoc(0), GasCost::new(1, 1)),
        (And, GasCost::new(1, 1)),
        (Pop, GasCost::new(1, 1)),
        (BitAnd, GasCost::new(2, 1)),
        (ReadRef, GasCost::new(1, 1)),
        (Sub, GasCost::new(1, 1)),
        (MutBorrowField(FieldHandleIndex::new(0)), GasCost::new(1, 1)),
        (
            MutBorrowFieldGeneric(FieldInstantiationIndex::new(0)),
            GasCost::new(1, 1),
        ),
        (ImmBorrowField(FieldHandleIndex::new(0)), GasCost::new(1, 1)),
        (
            ImmBorrowFieldGeneric(FieldInstantiationIndex::new(0)),
            GasCost::new(1, 1),
        ),
        (Add, GasCost::new(1, 1)),
        (CopyLoc(0), GasCost::new(1, 1)),
        (StLoc(0), GasCost::new(1, 1)),
        (Ret, GasCost::new(638, 1)),
        (Lt, GasCost::new(1, 1)),
        (LdU8(0), GasCost::new(1, 1)),
        (LdU64(0), GasCost::new(1, 1)),
        (LdU128(0), GasCost::new(1, 1)),
        (CastU8, GasCost::new(2, 1)),
        (CastU64, GasCost::new(1, 1)),
        (CastU128, GasCost::new(1, 1)),
        (Abort, GasCost::new(1, 1)),
        (MutBorrowLoc(0), GasCost::new(2, 1)),
        (ImmBorrowLoc(0), GasCost::new(1, 1)),
        (LdConst(ConstantPoolIndex::new(0)), GasCost::new(1, 1)),
        (Ge, GasCost::new(1, 1)),
        (Xor, GasCost::new(1, 1)),
        (Shl, GasCost::new(2, 1)),
        (Shr, GasCost::new(1, 1)),
        (Neq, GasCost::new(1, 1)),
        (Not, GasCost::new(1, 1)),
        (Call(FunctionHandleIndex::new(0)), GasCost::new(1132, 1)),
        (
            CallGeneric(FunctionInstantiationIndex::new(0)),
            GasCost::new(582, 1),
        ),
        (Le, GasCost::new(2, 1)),
        (Branch(0), GasCost::new(1, 1)),
        (Unpack(StructDefinitionIndex::new(0)), GasCost::new(2, 1)),
        (
            UnpackGeneric(StructDefInstantiationIndex::new(0)),
            GasCost::new(2, 1),
        ),
        (Or, GasCost::new(2, 1)),
        (LdFalse, GasCost::new(1, 1)),
        (LdTrue, GasCost::new(1, 1)),
        (Mod, GasCost::new(1, 1)),
        (BrFalse(0), GasCost::new(1, 1)),
        (Exists(StructDefinitionIndex::new(0)), GasCost::new(41, 1)),
        (
            ExistsGeneric(StructDefInstantiationIndex::new(0)),
            GasCost::new(34, 1),
        ),
        (BitOr, GasCost::new(2, 1)),
        (FreezeRef, GasCost::new(1, 1)),
        (
            MutBorrowGlobal(StructDefinitionIndex::new(0)),
            GasCost::new(21, 1),
        ),
        (
            MutBorrowGlobalGeneric(StructDefInstantiationIndex::new(0)),
            GasCost::new(15, 1),
        ),
        (
            ImmBorrowGlobal(StructDefinitionIndex::new(0)),
            GasCost::new(23, 1),
        ),
        (
            ImmBorrowGlobalGeneric(StructDefInstantiationIndex::new(0)),
            GasCost::new(14, 1),
        ),
        (Div, GasCost::new(3, 1)),
        (Eq, GasCost::new(1, 1)),
        (Gt, GasCost::new(1, 1)),
        (Pack(StructDefinitionIndex::new(0)), GasCost::new(2, 1)),
        (
            PackGeneric(StructDefInstantiationIndex::new(0)),
            GasCost::new(2, 1),
        ),
        (Nop, GasCost::new(1, 1)),
        (VecPack(SignatureIndex::new(0), 0), GasCost::new(84, 1)),
        (VecLen(SignatureIndex::new(0)), GasCost::new(98, 1)),
        (VecImmBorrow(SignatureIndex::new(0)), GasCost::new(1334, 1)),
        (VecMutBorrow(SignatureIndex::new(0)), GasCost::new(1902, 1)),
        (VecPushBack(SignatureIndex::new(0)), GasCost::new(53, 1)),
        (VecPopBack(SignatureIndex::new(0)), GasCost::new(227, 1)),
        (VecUnpack(SignatureIndex::new(0), 0), GasCost::new(572, 1)),
        (VecSwap(SignatureIndex::new(0)), GasCost::new(1436, 1)),
    ]
}

pub static INITIAL_COST_SCHEDULE: Lazy<CostTable> = Lazy::new(|| {
    let mut instrs = legacy_bytecode_instruction_costs();
    // Note that the DiemVM is expecting the table sorted by instruction order.
    instrs.sort_by_key(|cost| instruction_key(&cost.0));

    new_from_instructions(instrs)
});
