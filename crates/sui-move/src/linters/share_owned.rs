// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This analysis flags making objects passed as function parameters or resulting from unpacking
//! (likely already owned) shareable which would lead to an abort. A typical patterns is to create a
//! fresh object and share it within the same function

// =================================================================================================
// Data Model

use std::{
    cell::RefCell,
    collections::BTreeMap,
    ops::{Deref, DerefMut},
};

use codespan::FileId;
use codespan_reporting::diagnostic::{Diagnostic, Label, Severity};

use move_binary_format::file_format::CodeOffset;
use move_model::{ast::TempIndex, model::FunctionEnv};

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
    /// an fresh object resulting from packing
    FreshObject,
    /// a value that cannot be a fresh object
    NotFreshObject,
    /// a possibly owned object (e.g, coming from an argument or unpacking) - also a top value
    PossiblyOwnedObject,
}

impl AbsValue {
    pub fn is_fresh(&self) -> bool {
        matches!(self, Self::FreshObject)
    }
}

impl AbstractDomain for AbsValue {
    fn join(&mut self, other: &Self) -> JoinResult {
        if self == other {
            return JoinResult::Unchanged;
        }
        // unequal; use top value
        *self = AbsValue::PossiblyOwnedObject;
        JoinResult::Changed
    }
}

type ShareOwnedState = MapDomain<TempIndex, AbsValue>;

#[derive(Debug)]
struct State<'a>(&'a mut ShareOwnedState);

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
    type Target = ShareOwnedState;

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
// Transfer functions

#[derive(PartialOrd, PartialEq, Eq, Ord)]
struct WarningId {
    arg_index: usize,
    call_attr: AttrId,
}

pub struct ShareOwnedAnalysis<'a> {
    func_env: &'a FunctionEnv<'a>,
    func_data: &'a FunctionData,
    /// Warnings to surface to the programmer
    warnings: RefCell<BTreeMap<WarningId, Diagnostic<FileId>>>,
}

impl<'a> TransferFunctions for ShareOwnedAnalysis<'a> {
    type State = ShareOwnedState;
    const BACKWARD: bool = false;

    fn execute(&self, state: &mut Self::State, instr: &Bytecode, _offset: CodeOffset) {
        use Bytecode::*;
        use Operation::*;

        let mut state = State(state);
        match instr {
            Call(attr_id, rets, oper, args, _) => match oper {
                Function(mid, fid, _) => {
                    let global_env = self.func_env.module_env.env;
                    let callee_fun_env = global_env.get_function(mid.qualified(*fid));
                    let self_address = *callee_fun_env.module_env.self_address();
                    match (
                        callee_fun_env.module_env.get_identifier().as_str(),
                        callee_fun_env.get_identifier().as_str(),
                    ) {
                        ("transfer", "public_share_object")
                            if self_address == SUI_FRAMEWORK_ADDRESS =>
                        {
                            if !state.get_local_index(&args[0]).is_fresh() {
                                self.add_warning(args[0], *attr_id)
                            }
                        }
                        ("transfer", "share_object") if self_address == SUI_FRAMEWORK_ADDRESS => {
                            if !state.get_local_index(&args[0]).is_fresh() {
                                self.add_warning(args[0], *attr_id)
                            }
                        }
                        _ => {
                            for ret in rets {
                                state.insert(*ret, AbsValue::PossiblyOwnedObject);
                            }
                        }
                    }
                }
                Unpack(..) => {
                    for ret in rets {
                        state.insert(*ret, AbsValue::PossiblyOwnedObject);
                    }
                }
                Eq | Neq | CastU8 | CastU16 | CastU32 | CastU64 | CastU128 | CastU256 | Not
                | Add | Sub | Mul | Div | Mod | BitOr | BitAnd | Xor | Shl | Shr | Lt | Gt | Le
                | Ge | Or | And | FreezeRef => {
                    // these operations cannot produce an owned object
                    state.insert(rets[0], AbsValue::NotFreshObject);
                }
                Pack(..) => {
                    state.insert(rets[0], AbsValue::FreshObject);
                }
                ReadRef | BorrowLoc | BorrowField(..) => {
                    // these operations cannot produce an owned object
                    state.insert(rets[0], AbsValue::NotFreshObject);
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
                // this operation cannot produce an owned object
                state.insert(*lhs, AbsValue::NotFreshObject);
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

impl<'a> DataflowAnalysis for ShareOwnedAnalysis<'a> {}

impl<'a> ShareOwnedAnalysis<'a> {
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
        let mut initial_state = ShareOwnedState::default();
        // initialize_formals
        for (param_index, _param_type) in func_env.get_parameter_types().iter().enumerate() {
            initial_state.insert(param_index, AbsValue::PossiblyOwnedObject);
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

    pub fn add_warning(&self, arg_index: usize, call_attr: AttrId) {
        let message = "You may be trying to share an owned object. A typical pattern is to create an object and share it within the same function.";
        let warning_loc = self.func_data.locations.get(&call_attr).unwrap();
        let label = Label::primary(warning_loc.file_id(), warning_loc.span());
        let warning_id = WarningId {
            arg_index,
            call_attr,
        };
        self.warnings.borrow_mut().insert(
            warning_id,
            Diagnostic::new(Severity::Warning)
                .with_message(message)
                .with_labels(vec![label]),
        );
    }
}
