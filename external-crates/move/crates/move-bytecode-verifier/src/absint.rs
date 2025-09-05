// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_abstract_interpreter::{absint, control_flow_graph};
use move_binary_format::{
    CompiledModule,
    errors::{PartialVMError, PartialVMResult},
    file_format::{
        AbilitySet, Bytecode, CodeOffset, CodeUnit, FunctionDefinitionIndex, FunctionHandle,
        Signature,
    },
};
use move_bytecode_verifier_meter::{Meter, Scope};

//**************************************************************************************************
// New traits and public APIS
//**************************************************************************************************

pub use absint::JoinResult;
pub type VMControlFlowGraph = control_flow_graph::VMControlFlowGraph<Bytecode>;

pub trait AbstractDomain: Clone + Sized {
    fn join(
        &mut self,
        other: &Self,
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<JoinResult>;
}

pub trait TransferFunctions {
    type State: AbstractDomain;

    /// Execute local@instr found at index local@index in the current basic block from pre-state
    /// local@pre.
    /// Should return an Err if executing the instruction is unsuccessful, and () if
    /// the effects of successfully executing local@instr have been reflected by mutating
    /// local@pre.
    /// Auxiliary data from the analysis that is not part of the abstract state can be collected by
    /// mutating local@self.
    /// The last instruction index in the current block is local@last_index. Knowing this
    /// information allows clients to detect the end of a basic block and special-case appropriately
    /// (e.g., normalizing the abstract state before a join).
    fn execute(
        &mut self,
        pre: &mut Self::State,
        instr: &Bytecode,
        index: CodeOffset,
        bounds: (CodeOffset, CodeOffset),
        meter: &mut (impl Meter + ?Sized),
    ) -> PartialVMResult<()>;
}

pub fn analyze_function<TF: TransferFunctions, M: Meter + ?Sized>(
    function_context: &FunctionContext,
    meter: &mut M,
    transfer_functions: &mut TF,
    initial_state: TF::State,
) -> Result<(), PartialVMError> {
    let mut interpreter = AbstractInterpreter {
        meter,
        transfer_functions,
    };
    let _states = absint::analyze_function(
        &mut interpreter,
        &function_context.cfg,
        &function_context.code.code,
        initial_state,
    )?;
    Ok(())
}

/// A `FunctionContext` holds all the information needed by the verifier for `FunctionDefinition`.`
/// A control flow graph is built for a function when the `FunctionContext` is created.
pub struct FunctionContext<'a> {
    index: Option<FunctionDefinitionIndex>,
    code: &'a CodeUnit,
    parameters: &'a Signature,
    return_: &'a Signature,
    locals: &'a Signature,
    type_parameters: &'a [AbilitySet],
    cfg: VMControlFlowGraph,
}

impl<'a> FunctionContext<'a> {
    // Creates a `FunctionContext` for a module function.
    pub fn new(
        module: &'a CompiledModule,
        index: FunctionDefinitionIndex,
        code: &'a CodeUnit,
        function_handle: &'a FunctionHandle,
    ) -> Self {
        Self {
            index: Some(index),
            code,
            parameters: module.signature_at(function_handle.parameters),
            return_: module.signature_at(function_handle.return_),
            locals: module.signature_at(code.locals),
            type_parameters: &function_handle.type_parameters,
            cfg: VMControlFlowGraph::new(&code.code, &code.jump_tables),
        }
    }

    pub fn index(&self) -> Option<FunctionDefinitionIndex> {
        self.index
    }

    pub fn code(&self) -> &'a CodeUnit {
        self.code
    }

    pub fn parameters(&self) -> &'a Signature {
        self.parameters
    }

    pub fn return_(&self) -> &'a Signature {
        self.return_
    }

    pub fn locals(&self) -> &'a Signature {
        self.locals
    }

    pub fn type_parameters(&self) -> &'a [AbilitySet] {
        self.type_parameters
    }

    pub fn cfg(&self) -> &VMControlFlowGraph {
        &self.cfg
    }
}

//**************************************************************************************************
// Wrappers around shared absint and control flow graph implementations
//**************************************************************************************************

/// Costs for metered verification
const ANALYZE_FUNCTION_BASE_COST: u128 = 10;
const EXECUTE_BLOCK_BASE_COST: u128 = 10;
const PER_BACKEDGE_COST: u128 = 10;
const PER_SUCCESSOR_COST: u128 = 10;

struct AbstractInterpreter<'a, M: Meter + ?Sized, TF: TransferFunctions> {
    pub meter: &'a mut M,
    pub transfer_functions: &'a mut TF,
}

impl<M: Meter + ?Sized, TF: TransferFunctions> absint::AbstractInterpreter
    for AbstractInterpreter<'_, M, TF>
{
    type Error = PartialVMError;
    type BlockId = CodeOffset;
    type State = <TF as TransferFunctions>::State;
    type InstructionIndex = CodeOffset;
    type Instruction = Bytecode;

    fn start(&mut self) -> Result<(), Self::Error> {
        self.meter.add(Scope::Function, ANALYZE_FUNCTION_BASE_COST)
    }

    fn join(
        &mut self,
        pre: &mut Self::State,
        post: &Self::State,
    ) -> Result<absint::JoinResult, Self::Error> {
        pre.join(post, self.meter)
    }

    fn visit_block_pre_execution(
        &mut self,
        _block_id: Self::BlockId,
        _invariant: &mut absint::BlockInvariant<Self::State>,
    ) -> Result<(), Self::Error> {
        self.meter.add(Scope::Function, EXECUTE_BLOCK_BASE_COST)
    }

    fn visit_block_post_execution(
        &mut self,
        _block_id: Self::BlockId,
        _invariant: &mut absint::BlockInvariant<Self::State>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn visit_successor(&mut self, _block_id: Self::BlockId) -> Result<(), Self::Error> {
        self.meter.add(Scope::Function, PER_SUCCESSOR_COST)
    }

    fn visit_back_edge(
        &mut self,
        _from: Self::BlockId,
        _to: Self::BlockId,
    ) -> Result<(), Self::Error> {
        self.meter.add(Scope::Function, PER_BACKEDGE_COST)
    }

    fn execute(
        &mut self,
        _block_id: Self::BlockId,
        bounds: (CodeOffset, CodeOffset),
        state: &mut Self::State,
        offset: CodeOffset,
        instr: &Bytecode,
    ) -> Result<(), Self::Error> {
        self.transfer_functions
            .execute(state, instr, offset, bounds, self.meter)
    }
}
