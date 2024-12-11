// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This analysis flags transfers of an object to tx_context::sender(). Such objects should be
//! returned from the function instead

use move_core_types::account_address::AccountAddress;
use move_ir_types::location::*;

use crate::{
    cfgir::{
        absint::JoinResult,
        cfg::ImmForwardCFG,
        visitor::{
            calls_special_function, LocalState, SimpleAbsInt, SimpleAbsIntConstructor,
            SimpleDomain, SimpleExecutionContext,
        },
        CFGContext, MemberName,
    },
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        Diagnostic, Diagnostics,
    },
    hlir::ast::{Label, ModuleCall, Type, Type_, Var},
    parser::ast::Ability_,
    sui_mode::{SUI_ADDR_VALUE, TX_CONTEXT_MODULE_NAME},
};
use std::collections::BTreeMap;

use super::{
    type_abilities, LinterDiagnosticCategory, LinterDiagnosticCode, INVALID_LOC,
    LINT_WARNING_PREFIX, PUBLIC_TRANSFER_FUN, TRANSFER_FUN, TRANSFER_MOD_NAME,
};

const TRANSFER_FUNCTIONS: &[(AccountAddress, &str, &str)] = &[
    (SUI_ADDR_VALUE, TRANSFER_MOD_NAME, PUBLIC_TRANSFER_FUN),
    (SUI_ADDR_VALUE, TRANSFER_MOD_NAME, TRANSFER_FUN),
];

const SELF_TRANSFER_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Sui as u8,
    LinterDiagnosticCode::SelfTransfer as u8,
    "non-composable transfer to sender",
);

//**************************************************************************************************
// types
//**************************************************************************************************

pub struct SelfTransferVerifier;

pub struct SelfTransferVerifierAI {
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
        context: &'a CFGContext<'a>,
        cfg: &ImmForwardCFG,
        _init_state: &mut <Self::AI<'a> as SimpleAbsInt>::State,
    ) -> Option<Self::AI<'a>> {
        let MemberName::Function(name) = context.member else {
            return None;
        };

        if context.entry.is_some()
            || context.attributes.is_test_or_test_only()
            || context
                .info
                .module(&context.module)
                .attributes
                .is_test_or_test_only()
        {
            // Cannot return objects from entry
            // No need to check test functions
            return None;
        }

        if name.value.as_str() == "init" {
            // do not lint module initializers, since they do not have the option of returning
            // values, and the entire purpose of this linter is to encourage folks to return
            // values instead of using transfer
            return None;
        }
        if !calls_special_function(TRANSFER_FUNCTIONS, cfg) {
            // skip if it does not use transfer functions
            return None;
        }
        Some(SelfTransferVerifierAI {
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
                if is_wrappable_obj_type(&f.arguments[0].ty) {
                    let msg = "Transfer of an object to transaction sender address";
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
            }
            return Some(vec![]);
        }
        if f.is(&SUI_ADDR_VALUE, TX_CONTEXT_MODULE_NAME, "sender") {
            return Some(vec![Value::SenderAddress(*loc)]);
        }
        Some(match &return_ty.value {
            Type_::Unit => vec![],
            Type_::Single(_) => vec![Value::Other],
            Type_::Multiple(types) => vec![Value::Other; types.len()],
        })
    }
}

pub fn is_wrappable_obj_type(sp!(_, t_): &Type) -> bool {
    let Type_::Single(st) = t_ else {
        return false;
    };
    let Some(abilities) = type_abilities(st) else {
        return false;
    };
    abilities.has_ability_(Ability_::Key) && abilities.has_ability_(Ability_::Store)
}

impl SimpleDomain for State {
    type Value = Value;

    fn new(context: &CFGContext, mut locals: BTreeMap<Var, LocalState<Value>>) -> Self {
        for (_mut, v, _) in &context.signature.parameters {
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
