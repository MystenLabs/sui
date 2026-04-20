// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Basic-block gas charge batching pass.
//!
//! This pass rewrites each basic block by prepending a synthetic `Charge`
//! instruction that accounts, in aggregate, for the gas cost of every
//! fixed-cost instruction in the block. At runtime the interpreter handles
//! `Charge` with a single `charge_block()` call, replacing what would
//! otherwise have been N individual `charge_simple_instr()` calls. The
//! original instructions remain in place and execute identically; only the
//! gas charging is hoisted.
//!
//! A "fixed-cost" instruction is one whose gas cost is statically known from
//! the bytecode alone — loads (`LdU8`..`LdU256`, `LdTrue`, `LdFalse`),
//! arithmetic (`Add`, `Sub`, `Mul`, `Div`, `Mod`), bitwise ops (`BitOr`,
//! `BitAnd`, `Xor`, `Shl`, `Shr`), boolean ops (`Or`, `And`, `Not`),
//! comparisons (`Lt`, `Gt`, `Le`, `Ge`), branches (`BrTrue`, `BrFalse`,
//! `Branch`), casts (`CastU8`..`CastU256`), reference ops (`FreezeRef`,
//! `{Mut,Imm}BorrowLoc`, `{Mut,Imm}BorrowField{,Generic}`), and the
//! unconditional terminators (`Ret`, `Nop`, `Abort`).
//!
//! Variable-cost instructions — whose gas depends on runtime value sizes —
//! are left to charge individually at execution time: `LdConst`, `CopyLoc`,
//! `MoveLoc`, `StLoc`, `Pop`, `ReadRef`, `WriteRef`, `Eq`, `Neq`, all
//! `Pack`/`Unpack` and generic variants, vector operations (`VecPack`,
//! `VecPushBack`, ...), variant operations, `VariantSwitch`, and
//! `Call`/`CallGeneric`.
//!
//! The pass produces a `Charge(ChargeInfo)` instruction at the head of each
//! block with `has_fixed_costs() == true`. `ChargeInfo` records the
//! aggregate instruction count, pushes, pops, push size, and pop size for
//! the block's fixed-cost instructions so the gas meter can charge the same
//! totals as it would have per-instruction. Blocks whose fixed-cost total
//! is zero are left unchanged.

use crate::jit::optimization::ast::{self, Bytecode, ChargeInfo};
use move_binary_format::errors::PartialVMResult;
use move_core_types::gas_algebra::AbstractMemorySize;
use tracing::instrument;

/// Accumulated fixed gas costs for a basic block.
#[derive(Default)]
struct BlockGasCost {
    instructions: u64,
    pushes: u64,
    pops: u64,
    push_size: u64,
    pop_size: u64,
}

impl BlockGasCost {
    fn has_fixed_costs(&self) -> bool {
        self.instructions > 0
    }

    fn add(
        &mut self,
        pops: u64,
        pushes: u64,
        pop_size: AbstractMemorySize,
        push_size: AbstractMemorySize,
    ) {
        // Using saturating arithmetic: these fields aggregate per-block fixed
        // gas costs and realistic basic blocks never approach u64 bounds, but
        // saturation is the safe behavior if a malformed input ever did.
        self.instructions = self.instructions.saturating_add(1);
        self.pushes = self.pushes.saturating_add(pushes);
        self.pops = self.pops.saturating_add(pops);
        self.push_size = self.push_size.saturating_add(u64::from(push_size));
        self.pop_size = self.pop_size.saturating_add(u64::from(pop_size));
    }
}

/// Size constants for gas computation (matching the gas meter).
const REFERENCE_SIZE: AbstractMemorySize = AbstractMemorySize::new(8);
const BOOL_SIZE: AbstractMemorySize = AbstractMemorySize::new(1);
const U8_SIZE: AbstractMemorySize = AbstractMemorySize::new(1);
const U16_SIZE: AbstractMemorySize = AbstractMemorySize::new(2);
const U32_SIZE: AbstractMemorySize = AbstractMemorySize::new(4);
const U64_SIZE: AbstractMemorySize = AbstractMemorySize::new(8);
const U128_SIZE: AbstractMemorySize = AbstractMemorySize::new(16);
const U256_SIZE: AbstractMemorySize = AbstractMemorySize::new(32);

// Precomputed "two of X" sizes for binary ops. Using consts avoids clippy's
// arithmetic_side_effects lint that fires on `X + X` at match-arm positions.
const TWO_BOOL_SIZE: AbstractMemorySize = AbstractMemorySize::new(2);
const TWO_U8_SIZE: AbstractMemorySize = AbstractMemorySize::new(2);

/// Returns `Some((pops, pushes, pop_size, push_size))` for fixed-cost
/// instructions, `None` for variable-cost instructions that need runtime
/// charging. The sizes for arithmetic/bitwise/comparison ops are
/// conservative approximations that match the gas meter's existing
/// `get_simple_instruction_stack_change` logic.
pub(crate) fn get_fixed_instruction_cost(
    instr: &Bytecode,
) -> Option<(u64, u64, AbstractMemorySize, AbstractMemorySize)> {
    use Bytecode::*;
    match instr {
        // No-op / unconditional transfer
        Nop | Ret => Some((0, 0, AbstractMemorySize::zero(), AbstractMemorySize::zero())),

        // Branch instructions
        BrTrue(_) | BrFalse(_) => Some((1, 0, BOOL_SIZE, AbstractMemorySize::zero())),
        Branch(_) => Some((0, 0, AbstractMemorySize::zero(), AbstractMemorySize::zero())),

        // Load integer constants
        LdU8(_) => Some((0, 1, AbstractMemorySize::zero(), U8_SIZE)),
        LdU16(_) => Some((0, 1, AbstractMemorySize::zero(), U16_SIZE)),
        LdU32(_) => Some((0, 1, AbstractMemorySize::zero(), U32_SIZE)),
        LdU64(_) => Some((0, 1, AbstractMemorySize::zero(), U64_SIZE)),
        LdU128(_) => Some((0, 1, AbstractMemorySize::zero(), U128_SIZE)),
        LdU256(_) => Some((0, 1, AbstractMemorySize::zero(), U256_SIZE)),
        LdTrue | LdFalse => Some((0, 1, AbstractMemorySize::zero(), BOOL_SIZE)),

        // Reference operations with fixed cost
        FreezeRef => Some((1, 1, REFERENCE_SIZE, REFERENCE_SIZE)),
        MutBorrowLoc(_) | ImmBorrowLoc(_) => {
            Some((0, 1, AbstractMemorySize::zero(), REFERENCE_SIZE))
        }
        MutBorrowField(_)
        | ImmBorrowField(_)
        | MutBorrowFieldGeneric(_)
        | ImmBorrowFieldGeneric(_) => Some((1, 1, REFERENCE_SIZE, REFERENCE_SIZE)),

        // Cast operations — conservative: smallest input, actual output
        CastU8 => Some((1, 1, U8_SIZE, U8_SIZE)),
        CastU16 => Some((1, 1, U8_SIZE, U16_SIZE)),
        CastU32 => Some((1, 1, U8_SIZE, U32_SIZE)),
        CastU64 => Some((1, 1, U8_SIZE, U64_SIZE)),
        CastU128 => Some((1, 1, U8_SIZE, U128_SIZE)),
        CastU256 => Some((1, 1, U8_SIZE, U256_SIZE)),

        // Arithmetic — conservative: smallest inputs, largest output
        Add | Sub | Mul | Mod | Div => Some((2, 1, TWO_U8_SIZE, U256_SIZE)),
        BitOr | BitAnd | Xor => Some((2, 1, TWO_U8_SIZE, U256_SIZE)),
        Shl | Shr => Some((2, 1, TWO_U8_SIZE, U256_SIZE)),

        // Boolean operations
        Or | And => Some((2, 1, TWO_BOOL_SIZE, BOOL_SIZE)),
        Not => Some((1, 1, BOOL_SIZE, BOOL_SIZE)),

        // Comparison operations
        Lt | Gt | Le | Ge => Some((2, 1, TWO_U8_SIZE, BOOL_SIZE)),

        // Abort
        Abort => Some((1, 0, U64_SIZE, AbstractMemorySize::zero())),

        // --- Variable cost instructions (need runtime size info) ---
        // LdConst: depends on constant size
        LdConst(_) => None,
        // CopyLoc, MoveLoc, StLoc: depends on local value size
        CopyLoc(_) | MoveLoc(_) | StLoc(_) => None,
        // Pop: depends on value size
        Pop => None,
        // ReadRef, WriteRef: depends on referenced value size
        ReadRef | WriteRef => None,
        // Eq, Neq: depends on operand sizes
        Eq | Neq => None,
        // Pack/Unpack: depends on field count/sizes
        Pack(_) | PackGeneric(_) | Unpack(_) | UnpackGeneric(_) => None,
        // Vector operations: depend on element sizes
        VecPack(_, _)
        | VecLen(_)
        | VecImmBorrow(_)
        | VecMutBorrow(_)
        | VecPushBack(_)
        | VecPopBack(_)
        | VecUnpack(_, _)
        | VecSwap(_) => None,
        // Variant operations: depend on field sizes
        PackVariant(_)
        | PackVariantGeneric(_)
        | UnpackVariant(_)
        | UnpackVariantImmRef(_)
        | UnpackVariantMutRef(_)
        | UnpackVariantGeneric(_)
        | UnpackVariantGenericImmRef(_)
        | UnpackVariantGenericMutRef(_) => None,
        // VariantSwitch: depends on value size
        VariantSwitch(_) => None,
        // Call operations: handled separately at runtime (charge_call)
        Call(_) | CallGeneric(_) => None,
        // Charge itself is inserted by this pass; it should not appear in input.
        Charge(..) => None,
    }
}

/// Compute the aggregated fixed gas cost for a basic block by folding over
/// its instructions.
pub(crate) fn compute_block_fixed_costs(code: &[Bytecode]) -> BlockGasCostView {
    let mut cost = BlockGasCost::default();
    for instr in code {
        if let Some((pops, pushes, pop_size, push_size)) = get_fixed_instruction_cost(instr) {
            cost.add(pops, pushes, pop_size, push_size);
        }
    }
    BlockGasCostView(cost)
}

/// A read-only view of an accumulated block cost. Exposed for tests.
pub(crate) struct BlockGasCostView(BlockGasCost);

impl BlockGasCostView {
    pub(crate) fn has_fixed_costs(&self) -> bool {
        self.0.has_fixed_costs()
    }
    #[cfg(test)]
    pub(crate) fn instructions(&self) -> u64 {
        self.0.instructions
    }
    #[cfg(test)]
    pub(crate) fn pushes(&self) -> u64 {
        self.0.pushes
    }
    #[cfg(test)]
    pub(crate) fn pops(&self) -> u64 {
        self.0.pops
    }
}

/// Run the Charge-insertion pass over an entire package, rewriting each
/// function's basic blocks in place.
#[instrument(level = "trace", skip_all)]
pub fn pass(mut pkg: ast::Package) -> PartialVMResult<ast::Package> {
    for module in pkg.modules.values_mut() {
        for function in module.functions.values_mut() {
            if let Some(code) = function.code.as_mut() {
                for block in code.code.values_mut() {
                    insert_charge_into_block(block);
                }
            }
        }
    }
    Ok(pkg)
}

/// Rewrites a single basic block in place: if the block has any fixed-cost
/// instructions, prepend a `Charge` instruction with the aggregate costs.
fn insert_charge_into_block(block: &mut Vec<Bytecode>) {
    let cost = compute_block_fixed_costs(block);
    if !cost.has_fixed_costs() {
        return;
    }
    let info = Box::new(ChargeInfo {
        instructions: cost.0.instructions,
        pushes: cost.0.pushes,
        pops: cost.0.pops,
        push_size: cost.0.push_size,
        pop_size: cost.0.pop_size,
    });
    let mut new_code = Vec::with_capacity(block.len().saturating_add(1));
    new_code.push(Bytecode::Charge(info));
    new_code.append(block);
    *block = new_code;
}
