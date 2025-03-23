// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_abstract_interpreter::{
    absint::{self},
    control_flow_graph,
};
use move_binary_format::{
    errors::{PartialVMError, PartialVMResult},
    file_format::{
        AbilitySet, Bytecode, CodeOffset, CodeUnit, FunctionDefinitionIndex, FunctionHandle,
        Signature,
    },
    CompiledModule,
};
use move_bytecode_verifier_meter::{Meter, Scope};

//**************************************************************************************************
// New traits and public APIS
//**************************************************************************************************

pub use absint::JoinResult;

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
        function_context,
        meter,
        transfer_functions,
    };
    absint::analyze_function(
        &mut interpreter,
        &function_context.cfg,
        DomainWrapper(initial_state),
    )
}

/// A `FunctionContext` holds all the information needed by the verifier for `FunctionDefinition`.`
/// A control flow graph is built for a function when the `FunctionContext` is created.
pub struct FunctionContext<'a> {
    index: Option<FunctionDefinitionIndex>,
    parameters: &'a Signature,
    return_: &'a Signature,
    locals: &'a Signature,
    type_parameters: &'a [AbilitySet],
    cfg: ControlFlowGraph<'a>,
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
            parameters: module.signature_at(function_handle.parameters),
            return_: module.signature_at(function_handle.return_),
            locals: module.signature_at(code.locals),
            type_parameters: &function_handle.type_parameters,
            cfg: ControlFlowGraph::new(&code.code, &code.jump_tables),
        }
    }

    pub fn index(&self) -> Option<FunctionDefinitionIndex> {
        self.index
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

    pub fn cfg(&self) -> &ControlFlowGraph {
        &self.cfg
    }
}

//**************************************************************************************************
// Wrappers around shared absint and control flow graph implementations
//**************************************************************************************************

#[derive(Clone)]
struct DomainWrapper<T: AbstractDomain>(T);
impl<T: AbstractDomain> absint::AbstractDomain for DomainWrapper<T> {
    type Error = PartialVMError;
}

pub type ControlFlowGraph<'a> = control_flow_graph::VMControlFlowGraph<'a, Bytecode>;

/// Costs for metered verification
const ANALYZE_FUNCTION_BASE_COST: u128 = 10;
const EXECUTE_BLOCK_BASE_COST: u128 = 10;
const PER_BACKEDGE_COST: u128 = 10;
const PER_SUCCESSOR_COST: u128 = 10;

struct AbstractInterpreter<'context, 'meter, 'tf, M: Meter + ?Sized, TF: TransferFunctions> {
    pub function_context: &'context FunctionContext<'context>,
    pub meter: &'meter mut M,
    pub transfer_functions: &'tf mut TF,
}

impl<'context, 'meter, 'tf, M: Meter + ?Sized, TF: TransferFunctions> absint::AbstractInterpreter
    for AbstractInterpreter<'context, 'meter, 'tf, M, TF>
{
    type Error = PartialVMError;
    type BlockId = CodeOffset;
    type State = DomainWrapper<<TF as TransferFunctions>::State>;
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
        pre.0.join(&post.0, self.meter)
    }

    fn visit_block_execution(&mut self, _block_id: Self::BlockId) -> Result<(), Self::Error> {
        self.meter.add(Scope::Function, EXECUTE_BLOCK_BASE_COST)
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
        state: &mut Self::State,
        bounds: (Self::InstructionIndex, Self::InstructionIndex),
        offset: Self::InstructionIndex,
        instr: &Self::Instruction,
    ) -> Result<(), Self::Error> {
        self.transfer_functions
            .execute(&mut state.0, instr, offset, bounds, self.meter)
    }
}
