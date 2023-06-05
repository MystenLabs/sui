// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This analysis flags transfers of an object to tx_context::sender().
//! Such objects should be returned from the procedure instead

use std::{
    cell::RefCell,
    cmp::Ordering,
    collections::BTreeMap,
    ops::{Deref, DerefMut},
};

use codespan::FileId;
use codespan_reporting::diagnostic::{Diagnostic, Label, Severity};

use move_binary_format::file_format::CodeOffset;
use move_model::{
    ast::TempIndex,
    model::{FunctionEnv, GlobalEnv},
    ty::Type,
};

use move_stackless_bytecode::{
    dataflow_analysis::{DataflowAnalysis, TransferFunctions},
    dataflow_domains::{AbstractDomain, JoinResult, MapDomain},
    function_target::FunctionData,
    stackless_bytecode::{AttrId, Bytecode, Operation},
    stackless_control_flow_graph::StacklessControlFlowGraph,
};
use sui_types::SUI_FRAMEWORK_ADDRESS;

// =================================================================================================
// Data Model

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AbsValue {
    /// an address read from tx_context:sender()
    SenderAddress,
    /// an address read from a parameter, field, or another function call
    /// it may have originally been read from tx_context::sender() in some
    /// other function--we are ok with that
    NonSenderAddress,
    /// Possibly a sender address
    Top,
}

impl AbsValue {
    pub fn is_tx_context_sender(&self) -> bool {
        matches!(self, Self::SenderAddress)
    }
}

type SelfTransferState = MapDomain<TempIndex, AbsValue>;

#[derive(Debug)]
struct State<'a>(&'a mut SelfTransferState);

impl<'a> State<'a> {
    fn get_local_index(&self, i: &TempIndex) -> &AbsValue {
        self.0
            .get(i)
            .unwrap_or_else(|| panic!("Unbound local index {} in state {:?}", i, self))
    }

    fn assign(&mut self, lhs: TempIndex, rhs: &TempIndex) {
        let rhs_value = *self.get_local_index(rhs);
        self.0.insert(lhs, rhs_value);
    }
}

impl<'a> Deref for State<'a> {
    type Target = SelfTransferState;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<'a> DerefMut for State<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

// =================================================================================================
// Joins

impl PartialOrd for AbsValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self == other {
            return Some(Ordering::Equal);
        }
        match (self, other) {
            (_, AbsValue::Top) => Some(Ordering::Less),
            _ => None,
        }
    }
}

impl AbstractDomain for AbsValue {
    fn join(&mut self, other: &Self) -> JoinResult {
        if self == other {
            return JoinResult::Unchanged;
        }
        // unequal; use top value
        *self = AbsValue::Top;
        JoinResult::Changed
    }
}

// =================================================================================================
// Transfer functions

#[derive(PartialOrd, PartialEq, Eq, Ord)]
struct WarningId {
    arg_index: usize,
    call_attr: AttrId,
}

pub struct SelfTransferAnalysis<'a> {
    func_env: &'a FunctionEnv<'a>,
    func_data: &'a FunctionData,
    /// Warnings to surface to the programmer
    // Uses a map instead of a vec to avoid reporting multiple warnings
    // at program locations in a loop during fixpoint iteration
    warnings: RefCell<BTreeMap<WarningId, Diagnostic<FileId>>>,
}

impl<'a> TransferFunctions for SelfTransferAnalysis<'a> {
    type State = SelfTransferState;
    const BACKWARD: bool = false;

    fn execute(&self, state: &mut Self::State, instr: &Bytecode, _offset: CodeOffset) {
        use Bytecode::*;
        use Operation::*;

        let mut state = State(state);
        match instr {
            Call(attr_id, rets, oper, args, _) => match oper {
                Function(mid, fid, types) => {
                    let global_env = self.func_env.module_env.env;
                    let callee_fun_env = global_env.get_function(mid.qualified(*fid));
                    let self_address = *callee_fun_env.module_env.self_address();
                    match (
                        callee_fun_env.module_env.get_identifier().as_str(),
                        callee_fun_env.get_identifier().as_str(),
                    ) {
                        ("tx_context", "sender") if self_address == SUI_FRAMEWORK_ADDRESS => {
                            state.insert(rets[0], AbsValue::SenderAddress);
                        }
                        ("transfer", "public_transfer")
                            if self_address == SUI_FRAMEWORK_ADDRESS =>
                        {
                            if state.get_local_index(&args[1]).is_tx_context_sender() {
                                // using public transfer (we already know the type has `store`)
                                self.add_warning(
                                    args[0],
                                    *attr_id,
                                    &types[0],
                                    &self.func_env.get_full_name_str(),
                                    global_env,
                                )
                            }
                        }
                        ("transfer", "transfer") if self_address == SUI_FRAMEWORK_ADDRESS => {
                            if state.get_local_index(&args[1]).is_tx_context_sender() {
                                // using internal transfer on a type with `store`
                                self.add_warning(
                                    args[0],
                                    *attr_id,
                                    &types[0],
                                    &self.func_env.get_full_name_str(),
                                    global_env,
                                )
                            }
                        }
                        _ => {
                            // assume it might return the sender address
                            for ret in rets {
                                state.insert(*ret, AbsValue::Top);
                            }
                        }
                    }
                }
                Unpack(..) => {
                    // assume it might return the sender address
                    for ret in rets {
                        state.insert(*ret, AbsValue::Top);
                    }
                }
                Eq | Neq | CastU8 | CastU16 | CastU32 | CastU64 | CastU128 | CastU256 | Not
                | Add | Sub | Mul | Div | Mod | BitOr | BitAnd | Xor | Shl | Shr | Lt | Gt | Le
                | Ge | Or | And | FreezeRef => {
                    // These operations all produce a non-address value
                    state.insert(rets[0], AbsValue::NonSenderAddress);
                }
                ReadRef | Pack(..) | BorrowLoc | BorrowField(..) => {
                    // we assume these do not return the sender address--don't really
                    // care about tracking the sender address across packs/unpacks
                    // or through references
                    state.insert(rets[0], AbsValue::NonSenderAddress);
                }
                Destroy => {
                    state.remove(&args[0]);
                }
                WriteRef => (), // this has no rets
                MoveTo(..) | MoveFrom(..) | OpaqueCallEnd(..) | OpaqueCallBegin(..)
                | Exists(..) | BorrowGlobal(..) | GetField(..) | Uninit | Havoc(..)
                | GetGlobal(..) | Stop | IsParent(..) | WriteBack(..) | PackRef | UnpackRef
                | UnpackRefDeep | PackRefDeep | TraceLocal(..) | TraceReturn(..) | TraceAbort
                | TraceExp(..) | TraceGlobalMem(..) | EmitEvent | EventStoreDiverge => {
                    // these operations should never appear in Sui
                    unreachable!()
                }
            },
            Load(_, lhs, _) => {
                state.insert(*lhs, AbsValue::NonSenderAddress);
            }
            Assign(_, lhs, rhs, _) => state.assign(*lhs, rhs),
            Ret(..) | Abort(..) | Branch(..) | Jump(..) | Label(..) | Nop(..) => {}
            SaveMem(..) | Prop(..) | SaveSpecVar(..) => {
                // these operations should never appear in Sui
                unreachable!()
            }
        }
    }
}

impl<'a> DataflowAnalysis for SelfTransferAnalysis<'a> {}

impl<'a> SelfTransferAnalysis<'a> {
    pub fn analyze(
        func_env: &'a FunctionEnv,
        func_data: &'a FunctionData,
        cfg: &StacklessControlFlowGraph,
    ) {
        if !func_env.is_exposed() && func_env.get_name_str() == "init" {
            // do not lint module initializers, since they do not have the option of returning values,
            // and the entire purpose of this linter is to encourage folks to return values instead
            // of using transfer
            return;
        }
        let mut initial_state = SelfTransferState::default();
        // initialize_formals
        for (param_index, _param_type) in func_env.get_parameter_types().iter().enumerate() {
            initial_state.insert(param_index, AbsValue::NonSenderAddress);
        }

        let analysis = Self {
            func_env,
            func_data,
            warnings: RefCell::new(BTreeMap::new()),
        };
        analysis.analyze_function(initial_state, &func_data.code, cfg);
        let env = func_env.module_env.env;
        for (_, warning) in analysis.warnings.into_inner() {
            env.add_diag(warning)
        }
    }

    pub fn add_warning(
        &self,
        arg_index: usize,
        call_attr: AttrId,
        transferred_type: &Type,
        function_name: &str,
        env: &GlobalEnv,
    ) {
        let main_message = "Non-composable transfer to sender";

        let label_message = format!("Instead of transferring object of type `{}` to `tx_context::sender()`, consider returning it from `{}`.\
             This allows a caller to use the object, and enables composability via programmable transactions.",
            transferred_type.display(&env.get_type_display_ctx()),
            function_name,
        );
        let warning_loc = self.func_data.locations.get(&call_attr).unwrap();
        let label =
            Label::primary(warning_loc.file_id(), warning_loc.span()).with_message(label_message);
        let warning_id = WarningId {
            arg_index,
            call_attr,
        };
        self.warnings.borrow_mut().insert(
            warning_id,
            Diagnostic::new(Severity::Warning)
                .with_message(main_message)
                .with_labels(vec![label]),
        );
    }
}
