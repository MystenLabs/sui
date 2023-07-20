// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This analysis flags transfers of an object to tx_context::sender(). Such objects should be
//! returned from the function instead

use move_ir_types::location::*;

use move_compiler::{
    cfgir::{
        absint::JoinResult,
        ast::Program,
        visitor::{
            LocalState, SimpleAbsInt, SimpleAbsIntConstructor, SimpleDomain, SimpleExecutionContext,
        },
        CFGContext, MemberName,
    },
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        Diagnostic, Diagnostics,
    },
    hlir::ast::{Command, Exp, LValue, Label, ModuleCall, Type, Type_, Var},
    shared::CompilationEnv,
};
use move_symbol_pool::Symbol;
use std::collections::BTreeMap;

use super::{
    INVALID_LOC, LINT_WARNING_PREFIX, SELF_TRANSFER_DIAG_CATEGORY, SELF_TRANSFER_DIAG_CODE,
};

const TRANSFER_FUNCTIONS: &[(&str, &str, &str)] = &[
    ("sui", "transfer", "public_transfer"),
    ("sui", "transfer", "transfer"),
];

const SELF_TRANSFER_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    SELF_TRANSFER_DIAG_CATEGORY,
    SELF_TRANSFER_DIAG_CODE,
    "non-composable transfer to sender",
);

//**************************************************************************************************
// types
//**************************************************************************************************

pub struct SelfTransferVerifier;

pub struct SelfTransferVerifierAI {
    fn_name: Symbol,
    fn_ret_loc: Loc,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Value {
    /// an address read from tx_context:sender()
    SenderAddress(Loc),
    #[default]
    Other,
}

pub struct ExecutionContext {
    diags: Diagnostics,
}

#[derive(Clone, Debug)]
pub struct State {
    locals: BTreeMap<Var, LocalState<Value>>,
}

//**************************************************************************************************
// impls
//**************************************************************************************************

impl SimpleAbsIntConstructor for SelfTransferVerifier {
    type AI<'a> = SelfTransferVerifierAI;

    fn new<'a>(
        _env: &CompilationEnv,
        _program: &'a Program,
        context: &'a CFGContext<'a>,
        _init_state: &mut <Self::AI<'a> as SimpleAbsInt>::State,
    ) -> Option<Self::AI<'a>> {
        let Some(_) = &context.module else {
            return None
        };

        let MemberName::Function(name) = context.member else {
            return None;
        };

        if name.value.as_str() == "init" {
            // do not lint module initializers, since they do not have the option of returning
            // values, and the entire purpose of this linter is to encourage folks to return
            // values instead of using transfer
            return None;
        }
        Some(SelfTransferVerifierAI {
            fn_name: name.value,
            fn_ret_loc: context.signature.return_type.loc,
        })
    }
}

impl SimpleAbsInt for SelfTransferVerifierAI {
    type State = State;
    type ExecutionContext = ExecutionContext;

    fn finish(&mut self, _final_states: BTreeMap<Label, State>, diags: Diagnostics) -> Diagnostics {
        diags
    }

    fn start_command(&self, _: &mut State) -> ExecutionContext {
        ExecutionContext {
            diags: Diagnostics::new(),
        }
    }

    fn finish_command(&self, context: ExecutionContext, _state: &mut State) -> Diagnostics {
        let ExecutionContext { diags } = context;
        diags
    }

    fn exp_custom(
        &self,
        _context: &mut ExecutionContext,
        _state: &mut State,
        _e: &Exp,
    ) -> Option<Vec<Value>> {
        None
    }

    fn call_custom(
        &self,
        context: &mut ExecutionContext,
        _state: &mut State,
        loc: &Loc,
        return_ty: &Type,
        f: &ModuleCall,
        args: Vec<Value>,
    ) -> Option<Vec<Value>> {
        if TRANSFER_FUNCTIONS
            .iter()
            .any(|(addr, module, fun)| f.is(addr, module, fun))
        {
            if let Value::SenderAddress(sender_addr_loc) = args[1] {
                let msg = format!(
                    "Transfer of an object to transaction sender address in function {}",
                    self.fn_name
                );
                let uid_msg =
                    "Returning an object from a function, allows a caller to use the object \
                               and enables composability via programmable transactions.";
                let mut d = diag!(SELF_TRANSFER_DIAG, (*loc, msg), (self.fn_ret_loc, uid_msg));
                if sender_addr_loc != INVALID_LOC {
                    d.add_secondary_label((
                        sender_addr_loc,
                        "Transaction sender address coming from here",
                    ));
                }
                context.add_diag(d);
            }
            return Some(vec![]);
        }
        if f.is("sui", "tx_context", "sender") {
            return Some(vec![Value::SenderAddress(*loc)]);
        }
        Some(match &return_ty.value {
            Type_::Unit => vec![],
            Type_::Single(_) => vec![Value::Other],
            Type_::Multiple(types) => vec![Value::Other; types.len()],
        })
    }

    fn command_custom(&self, _: &mut ExecutionContext, _: &mut State, _: &Command) -> bool {
        false
    }

    fn lvalue_custom(
        &self,
        _context: &mut ExecutionContext,
        _state: &mut State,
        _l: &LValue,
        _value: &Value,
    ) -> bool {
        false
    }
}

impl SimpleDomain for State {
    type Value = Value;

    fn new(context: &CFGContext, mut locals: BTreeMap<Var, LocalState<Value>>) -> Self {
        for (v, _) in &context.signature.parameters {
            let local_state = locals.get_mut(v).unwrap();
            if let LocalState::Available(loc, _) = local_state {
                *local_state = LocalState::Available(*loc, Value::Other);
            }
        }
        State { locals }
    }

    fn locals_mut(&mut self) -> &mut BTreeMap<Var, LocalState<Value>> {
        &mut self.locals
    }

    fn locals(&self) -> &BTreeMap<Var, LocalState<Value>> {
        &self.locals
    }

    fn join_value(v1: &Value, v2: &Value) -> Value {
        match (v1, v2) {
            (sender @ Value::SenderAddress(loc1), Value::SenderAddress(loc2)) => {
                if loc1 == loc2 {
                    *sender
                } else {
                    Value::SenderAddress(INVALID_LOC)
                }
            }
            (Value::Other, _) | (_, Value::Other) => Value::Other,
        }
    }

    fn join_impl(&mut self, _: &Self, _: &mut JoinResult) {}
}

impl SimpleExecutionContext for ExecutionContext {
    fn add_diag(&mut self, diag: Diagnostic) {
        self.diags.add(diag)
    }
}
