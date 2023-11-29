// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module lays out the basic abstract costing schedule for bytecode instructions.
//!
//! It is important to note that the cost schedule defined in this file does not track hashing
//! operations or other native operations; the cost of each native operation will be returned by the
//! native function itself.
use move_binary_format::{
    errors::{PartialVMError, PartialVMResult},
    file_format::{
        Bytecode, ConstantPoolIndex, EnumDefInstantiationIndex, EnumDefinitionIndex,
        FieldHandleIndex, FieldInstantiationIndex, FunctionHandleIndex, FunctionInstantiationIndex,
        SignatureIndex, StructDefInstantiationIndex, StructDefinitionIndex, VariantJumpTableIndex,
    },
    file_format_common::{instruction_key, Opcodes},
};
use move_core_types::{
    gas_algebra::{
        AbstractMemorySize, GasQuantity, InternalGas, InternalGasPerAbstractMemoryUnit,
        InternalGasUnit, NumArgs, NumBytes, ToUnit, ToUnitFractional,
    },
    language_storage::ModuleId,
    u256,
    vm_status::StatusCode,
};
#[cfg(debug_assertions)]
use move_vm_profiler::GasProfiler;
use move_vm_types::{
    gas::{GasMeter, SimpleInstruction},
    views::{TypeView, ValueView},
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{
    ops::{Add, Mul},
    u64,
};
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
    #[cfg(debug_assertions)]
    profiler: Option<GasProfiler>,
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
            #[cfg(debug_assertions)]
            profiler: None,
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
            #[cfg(debug_assertions)]
            profiler: None,
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
        LdU16 => LD_U16,
        LdU32 => LD_U32,
        LdU256 => LD_U256,
        CastU16 => CAST_U16,
        CastU32 => CAST_U32,
        CastU256 => CAST_U256,
    }
}

impl<'b> GasMeter for GasStatus<'b> {
    /// Charge an instruction and fail if not enough gas units are left.
    fn charge_simple_instr(&mut self, instr: SimpleInstruction) -> PartialVMResult<()> {
        self.charge_instr(get_simple_instruction_opcode(instr))
    }

    fn charge_pop(&mut self, _popped_val: impl ValueView) -> PartialVMResult<()> {
        self.charge_instr(Opcodes::POP)
    }

    fn charge_native_function(
        &mut self,
        amount: InternalGas,
        _ret_vals: Option<impl ExactSizeIterator<Item = impl ValueView>>,
    ) -> PartialVMResult<()> {
        self.deduct_gas(amount)
    }

    fn charge_native_function_before_execution(
        &mut self,
        _ty_args: impl ExactSizeIterator<Item = impl TypeView>,
        _args: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_call(
        &mut self,
        _module_id: &ModuleId,
        _func_name: &str,
        args: impl ExactSizeIterator<Item = impl ValueView>,
        _num_locals: NumArgs,
    ) -> PartialVMResult<()> {
        self.charge_instr_with_size(Opcodes::CALL, (args.len() as u64 + 1).into())
    }

    fn charge_call_generic(
        &mut self,
        _module_id: &ModuleId,
        _func_name: &str,
        ty_args: impl ExactSizeIterator<Item = impl TypeView>,
        args: impl ExactSizeIterator<Item = impl ValueView>,
        _num_locals: NumArgs,
    ) -> PartialVMResult<()> {
        self.charge_instr_with_size(
            Opcodes::CALL_GENERIC,
            ((ty_args.len() + args.len() + 1) as u64).into(),
        )
    }

    fn charge_ld_const(&mut self, size: NumBytes) -> PartialVMResult<()> {
        self.charge_instr_with_size(Opcodes::LD_CONST, u64::from(size).into())
    }

    fn charge_ld_const_after_deserialization(
        &mut self,
        _val: impl ValueView,
    ) -> PartialVMResult<()> {
        Ok(())
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

    fn charge_write_ref(
        &mut self,
        new_val: impl ValueView,
        _old_val: impl ValueView,
    ) -> PartialVMResult<()> {
        self.charge_instr_with_size(Opcodes::WRITE_REF, new_val.legacy_abstract_memory_size())
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

    fn charge_load_resource(
        &mut self,
        _loaded: Option<(NumBytes, impl ValueView)>,
    ) -> PartialVMResult<()> {
        Ok(())
    }

    fn charge_borrow_global(
        &mut self,
        is_mut: bool,
        is_generic: bool,
        _ty: impl TypeView,
        is_success: bool,
    ) -> PartialVMResult<()> {
        use Opcodes::*;

        if is_success {
            let op = match (is_mut, is_generic) {
                (false, false) => IMM_BORROW_GLOBAL,
                (false, true) => IMM_BORROW_GLOBAL_GENERIC,
                (true, false) => MUT_BORROW_GLOBAL,
                (true, true) => MUT_BORROW_GLOBAL_GENERIC,
            };

            self.charge_instr_with_size(op, REFERENCE_SIZE)?;
        }

        Ok(())
    }

    fn charge_exists(
        &mut self,
        is_generic: bool,
        _ty: impl TypeView,
        // TODO(Gas): see if we can get rid of this param
        exists: bool,
    ) -> PartialVMResult<()> {
        use Opcodes::*;

        let op = if is_generic { EXISTS_GENERIC } else { EXISTS };
        self.charge_instr_with_size(
            op,
            if exists {
                REFERENCE_SIZE
            } else {
                MIN_EXISTS_DATA_SIZE
            },
        )
    }

    fn charge_move_from(
        &mut self,
        is_generic: bool,
        _ty: impl TypeView,
        val: Option<impl ValueView>,
    ) -> PartialVMResult<()> {
        use Opcodes::*;

        if let Some(val) = val {
            let op = if is_generic {
                MOVE_FROM_GENERIC
            } else {
                MOVE_FROM
            };

            self.charge_instr_with_size(op, val.legacy_abstract_memory_size())?;
        }

        Ok(())
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
        _elems: impl ExactSizeIterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        self.charge_instr_with_size(
            Opcodes::VEC_PUSH_BACK,
            u64::from(expect_num_elements).into(),
        )
    }

    fn charge_vec_swap(&mut self, _ty: impl TypeView) -> PartialVMResult<()> {
        self.charge_instr(Opcodes::VEC_SWAP)
    }

    fn charge_drop_frame(
        &mut self,
        _locals: impl Iterator<Item = impl ValueView>,
    ) -> PartialVMResult<()> {
        Ok(())
    }

    /// Returns the gas left
    fn remaining_gas(&self) -> InternalGas {
        self.gas_left
    }

    #[cfg(debug_assertions)]
    fn get_profiler_mut(&mut self) -> Option<&mut GasProfiler> {
        self.profiler.as_mut()
    }

    #[cfg(debug_assertions)]
    fn set_profiler(&mut self, profiler: GasProfiler) {
        self.profiler = Some(profiler);
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
        (LdU16(0), GasCost::new(0, 0)),
        (LdU32(0), GasCost::new(0, 0)),
        (LdU256(u256::U256::zero()), GasCost::new(0, 0)),
        (CastU16, GasCost::new(0, 0)),
        (CastU32, GasCost::new(0, 0)),
        (CastU256, GasCost::new(0, 0)),
        (
            PackVariant(EnumDefinitionIndex::new(0), 0),
            GasCost::new(0, 0),
        ),
        (
            PackVariantGeneric(EnumDefInstantiationIndex::new(0), 0),
            GasCost::new(0, 0),
        ),
        (
            UnpackVariant(EnumDefinitionIndex::new(0), 0),
            GasCost::new(0, 0),
        ),
        (
            UnpackVariantImmRef(EnumDefinitionIndex::new(0), 0),
            GasCost::new(0, 0),
        ),
        (
            UnpackVariantMutRef(EnumDefinitionIndex::new(0), 0),
            GasCost::new(0, 0),
        ),
        (
            UnpackVariantGeneric(EnumDefInstantiationIndex::new(0), 0),
            GasCost::new(0, 0),
        ),
        (
            UnpackVariantGenericImmRef(EnumDefInstantiationIndex::new(0), 0),
            GasCost::new(0, 0),
        ),
        (
            UnpackVariantGenericMutRef(EnumDefInstantiationIndex::new(0), 0),
            GasCost::new(0, 0),
        ),
        (
            VariantSwitch(VariantJumpTableIndex::new(0)),
            GasCost::new(0, 0),
        ),
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

pub fn unit_cost_schedule() -> CostTable {
    new_from_instructions(
        zero_cost_instruction_table()
            .into_iter()
            .map(|(bytecode, _)| (bytecode, GasCost::new(1, 1)))
            .collect(),
    )
}

pub fn bytecode_instruction_costs() -> Vec<(Bytecode, GasCost)> {
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
        (LdU16(0), GasCost::new(1, 1)),
        (LdU32(0), GasCost::new(1, 1)),
        (LdU256(u256::U256::zero()), GasCost::new(1, 1)),
        (CastU16, GasCost::new(2, 1)),
        (CastU32, GasCost::new(2, 1)),
        (CastU256, GasCost::new(2, 1)),
        (
            PackVariant(EnumDefinitionIndex::new(0), 0),
            GasCost::new(2, 1),
        ),
        (
            PackVariantGeneric(EnumDefInstantiationIndex::new(0), 0),
            GasCost::new(2, 1),
        ),
        (
            UnpackVariant(EnumDefinitionIndex::new(0), 0),
            GasCost::new(2, 1),
        ),
        (
            UnpackVariantImmRef(EnumDefinitionIndex::new(0), 0),
            GasCost::new(2, 1),
        ),
        (
            UnpackVariantMutRef(EnumDefinitionIndex::new(0), 0),
            GasCost::new(2, 1),
        ),
        (
            UnpackVariantGeneric(EnumDefInstantiationIndex::new(0), 0),
            GasCost::new(2, 1),
        ),
        (
            UnpackVariantGenericImmRef(EnumDefInstantiationIndex::new(0), 0),
            GasCost::new(2, 1),
        ),
        (
            UnpackVariantGenericMutRef(EnumDefInstantiationIndex::new(0), 0),
            GasCost::new(2, 1),
        ),
        (
            VariantSwitch(VariantJumpTableIndex::new(0)),
            GasCost::new(2, 1),
        ),
    ]
}

pub static INITIAL_COST_SCHEDULE: Lazy<CostTable> = Lazy::new(|| {
    let mut instrs = bytecode_instruction_costs();
    // Note that the DiemVM is expecting the table sorted by instruction order.
    instrs.sort_by_key(|cost| instruction_key(&cost.0));

    new_from_instructions(instrs)
});
