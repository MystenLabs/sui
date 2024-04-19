// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This escape analysis flags procedures that return a reference pointing inside of a struct type
//! declared in the current module.

use std::{cell::RefCell, cmp::Ordering, collections::BTreeMap};

use codespan::FileId;
use codespan_reporting::diagnostic::{Diagnostic, Label, Severity};

use move_binary_format::file_format::CodeOffset;
use move_model::{ast::TempIndex, model::FunctionEnv};

use crate::{
    dataflow_analysis::{DataflowAnalysis, TransferFunctions},
    dataflow_domains::{AbstractDomain, JoinResult, MapDomain},
    function_target::FunctionData,
    function_target_pipeline::{FunctionTargetProcessor, FunctionTargetsHolder},
    stackless_bytecode::{Bytecode, Operation},
    stackless_control_flow_graph::StacklessControlFlowGraph,
};

// =================================================================================================
// Data Model

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AbsValue {
    NonRef,
    OkRef,
    InternalRef,
}

impl AbsValue {
    pub fn is_internal_ref(&self) -> bool {
        matches!(self, Self::InternalRef)
    }
}

type EscapeAnalysisState = MapDomain<TempIndex, AbsValue>;

impl EscapeAnalysisState {
    fn get_local_index(&self, i: &TempIndex) -> &AbsValue {
        self.get(i)
            .unwrap_or_else(|| panic!("Unbound local index {} in state {:?}", i, self))
    }

    fn assign(&mut self, lhs: TempIndex, rhs: &TempIndex) {
        let rhs_value = *self.get_local_index(rhs);
        self.insert(lhs, rhs_value);
    }

    pub fn call(&mut self, rets: &[TempIndex], args: &[TempIndex], call_env: &FunctionEnv) {
        let has_internal_ref_input = args
            .iter()
            .any(|arg_index| self.get(arg_index).unwrap().is_internal_ref());
        for (ret_index, ret_type) in call_env.get_return_types().iter().enumerate() {
            let ret_value = if ret_type.is_reference() {
                if has_internal_ref_input {
                    AbsValue::InternalRef
                } else {
                    AbsValue::OkRef
                }
            } else {
                AbsValue::NonRef
            };
            self.insert(rets[ret_index], ret_value);
        }
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
            (_, AbsValue::InternalRef) => Some(Ordering::Less),
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
        *self = AbsValue::InternalRef;
        JoinResult::Changed
    }
}

// =================================================================================================
// Transfer functions

#[derive(PartialOrd, PartialEq, Eq, Ord)]
struct WarningId {
    ret_index: usize,
    offset: CodeOffset,
}

struct EscapeAnalysis<'a> {
    func_env: &'a FunctionEnv<'a>,
    /// Warnings about escaped references to surface to the programmer
    // Uses a map instead of a vec to avoid reporting multiple warnings
    // at program locations in a loop during fixpoint iteration
    escape_warnings: RefCell<BTreeMap<WarningId, Diagnostic<FileId>>>,
}

impl EscapeAnalysis<'_> {
    pub fn add_escaped_return_warning(&self, ret_index: usize, is_mut: bool, offset: CodeOffset) {
        let message = format!(
            "Leaked {} module-internal reference via return value {}",
            if is_mut { "mutable" } else { "immutable" },
            ret_index
        );
        let fun_loc = self.func_env.get_loc();
        let label = Label::primary(fun_loc.file_id(), fun_loc.span());
        let severity = if is_mut {
            Severity::Error
        } else {
            Severity::Warning
        };
        let warning_id = WarningId { ret_index, offset };
        self.escape_warnings.borrow_mut().insert(
            warning_id,
            Diagnostic::new(severity)
                .with_message(message)
                .with_labels(vec![label]),
        );
    }
}

impl<'a> TransferFunctions for EscapeAnalysis<'a> {
    type State = EscapeAnalysisState;
    const BACKWARD: bool = false;

    fn execute(&self, state: &mut Self::State, instr: &Bytecode, offset: CodeOffset) {
        use Bytecode::*;
        use Operation::*;

        match instr {
            Call(_, rets, oper, args, _) => match oper {
                BorrowField(_, _, _type_params, _) => {
                    let to_propagate = match state.get_local_index(&args[0]) {
                        AbsValue::OkRef => AbsValue::OkRef,
                        AbsValue::InternalRef => AbsValue::InternalRef,
                        AbsValue::NonRef => panic!("Invariant violation: expected reference"),
                    };
                    state.insert(rets[0], to_propagate);
                }
                BorrowGlobal(_mid, _sid, _types) => {
                    state.insert(rets[0], AbsValue::InternalRef);
                }
                ReadRef | MoveFrom(..) | Exists(..) | Pack(..) | Eq | Neq | CastU8 | CastU64
                | CastU128 | Not | Add | Sub | Mul | Div | Mod | BitOr | BitAnd | Xor | Shl
                | Shr | Lt | Gt | Le | Ge | Or | And => {
                    // These operations all produce a non-reference value
                    state.insert(rets[0], AbsValue::NonRef);
                }
                BorrowLoc => {
                    state.insert(rets[0], AbsValue::OkRef);
                }
                Function(mid, fid, _) => {
                    let callee_fun_env = self
                        .func_env
                        .module_env
                        .env
                        .get_function(mid.qualified(*fid));
                    if callee_fun_env.is_native() {
                        // check if this is a modeled native
                        match (
                            callee_fun_env.module_env.get_identifier().as_str(),
                            callee_fun_env.get_identifier().as_str(),
                        ) {
                            ("vector", "borrow_mut") | ("vector", "borrow") => {
                                let vec_arg = 0;
                                let to_propagate = match state.get_local_index(&args[vec_arg]) {
                                    AbsValue::OkRef => AbsValue::OkRef,
                                    AbsValue::InternalRef => AbsValue::InternalRef,
                                    AbsValue::NonRef => {
                                        panic!("Invariant violation: expected reference")
                                    }
                                };
                                state.insert(rets[0], to_propagate);
                            }
                            _ => {
                                // unmodeled native, treat the same as ordinary call
                                state.call(rets, args, &callee_fun_env)
                            }
                        }
                    } else {
                        state.call(rets, args, &callee_fun_env)
                    }
                }
                Unpack(..) => {
                    for ret_index in rets {
                        state.insert(*ret_index, AbsValue::NonRef);
                    }
                }
                FreezeRef => state.assign(rets[0], &args[0]),
                WriteRef | MoveTo(..) => {
                    // these operations do not assign any locals
                }
                Uninit => {
                    // this operation is just a marker and does not assign any locals
                }
                Destroy => {
                    state.remove(&args[0]);
                }
                oper => panic!("unsupported oper {:?}", oper),
            },
            Load(_, lhs, _) => {
                state.insert(*lhs, AbsValue::NonRef);
            }
            Assign(_, lhs, rhs, _) => state.assign(*lhs, rhs),
            Ret(_, rets) => {
                let ret_types = self.func_env.get_return_types();
                for (ret_index, ret) in rets.iter().enumerate() {
                    if state.get_local_index(ret).is_internal_ref() {
                        self.add_escaped_return_warning(
                            ret_index,
                            ret_types[ret_index].is_mutable_reference(),
                            offset,
                        );
                    }
                }
            }
            Abort(..) | Branch(..) | Jump(..) | Label(..) | Nop(..) | VariantSwitch(..) => {
                // these operations do not assign any locals
            }
        }
    }
}

impl<'a> DataflowAnalysis for EscapeAnalysis<'a> {}
pub struct EscapeAnalysisProcessor();
impl EscapeAnalysisProcessor {
    pub fn new() -> Box<Self> {
        Box::new(EscapeAnalysisProcessor())
    }
}

impl FunctionTargetProcessor for EscapeAnalysisProcessor {
    fn process(
        &self,
        _targets: &mut FunctionTargetsHolder,
        func_env: &FunctionEnv,
        data: FunctionData,
        _scc_opt: Option<&[FunctionEnv]>,
    ) -> FunctionData {
        if func_env.is_native() {
            return data;
        }
        let mut initial_state = EscapeAnalysisState::default();
        // initialize_formals
        for (param_index, param_type) in func_env.get_parameter_types().iter().enumerate() {
            let param_val = if param_type.is_reference() {
                AbsValue::OkRef
            } else {
                AbsValue::NonRef
            };
            initial_state.insert(param_index, param_val);
        }

        let cfg = StacklessControlFlowGraph::new_forward(&data.code);
        let analysis = EscapeAnalysis {
            func_env,
            escape_warnings: RefCell::new(BTreeMap::new()),
        };
        analysis.analyze_function(initial_state, &data.code, &cfg);
        let env = func_env.module_env.env;
        for (_, warning) in analysis.escape_warnings.into_inner() {
            env.add_diag(warning)
        }
        data
    }

    fn name(&self) -> String {
        "escape_analysis".to_string()
    }
}
