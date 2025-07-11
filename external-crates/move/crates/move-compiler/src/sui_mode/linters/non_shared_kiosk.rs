// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tracks transfer operations on `sui::kiosk::Kiosk` type. If a `Kiosk` is passed
//! to a `public_transfer` or `public_freeze_object` function, it will emit a warning,
//! suggesting to use `transfer::public_share_object` instead.

use crate::{
    cfgir::{
        CFGContext,
        cfg::ImmForwardCFG,
        visitor::{
            LocalState, SimpleAbsInt, SimpleAbsIntConstructor, SimpleDomain,
            SimpleExecutionContext, calls_special_function,
        },
    },
    diag,
    diagnostics::{
        Diagnostic, Diagnostics,
        codes::{DiagnosticInfo, Severity, custom},
    },
    hlir::ast::{Label, ModuleCall, SingleType, Type, Type_, Var},
    sui_mode::{
        SUI_ADDR_VALUE,
        linters::{KIOSK_MOD_NAME, KIOSK_STRUCT_NAME, PUBLIC_FREEZE_FUN},
    },
};
use move_bytecode_verifier::absint::JoinResult;
use move_core_types::account_address::AccountAddress;
use move_ir_types::location::*;

use std::collections::BTreeMap;

use super::{
    INVALID_LOC, LINT_WARNING_PREFIX, LinterDiagnosticCategory, LinterDiagnosticCode,
    PUBLIC_TRANSFER_FUN, TRANSFER_MOD_NAME,
};

const TRANSFER_FUNCTIONS: &[(AccountAddress, &str, &str)] =
    &[(SUI_ADDR_VALUE, TRANSFER_MOD_NAME, PUBLIC_TRANSFER_FUN)];

const TRANSFER_KIOSK_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Sui as u8,
    LinterDiagnosticCode::TransferKiosk as u8,
    "Kiosks should always be `shared`",
);

pub struct KioskTransferVerifier;

pub struct KioskTransferVerifierAI;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Value {
    Kiosk(Loc),
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

impl SimpleAbsIntConstructor for KioskTransferVerifier {
    type AI<'a> = KioskTransferVerifierAI;

    fn new<'a>(
        _context: &'a CFGContext<'a>,
        cfg: &ImmForwardCFG,
        _init_state: &mut <Self::AI<'a> as SimpleAbsInt>::State,
    ) -> Option<Self::AI<'a>> {
        // Skip if it does not use transfer functions.
        if !calls_special_function(TRANSFER_FUNCTIONS, cfg) {
            return None;
        }

        Some(KioskTransferVerifierAI {})
    }
}

impl SimpleAbsInt for KioskTransferVerifierAI {
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
        // If there is a transfer function, we need to check if the argument to this function is a `Kiosk`.
        if f.is(&SUI_ADDR_VALUE, TRANSFER_MOD_NAME, PUBLIC_TRANSFER_FUN) {
            if let Value::Kiosk(kiosk_loc) = args[0] {
                let mut d = diag!(
                    TRANSFER_KIOSK_DIAG,
                    (
                        *loc,
                        "Kiosk should not be transferred, use `transfer::public_share_object` instead"
                    )
                );

                d.add_secondary_label((kiosk_loc, "Value originating from here"));
                context.add_diag(d);
            }

            return Some(vec![]);
        }

        if f.is(&SUI_ADDR_VALUE, TRANSFER_MOD_NAME, PUBLIC_FREEZE_FUN) {
            if let Value::Kiosk(kiosk_loc) = args[0] {
                let mut d = diag!(
                    TRANSFER_KIOSK_DIAG,
                    (
                        *loc,
                        "Kiosk should not be frozen, use `transfer::public_share_object` instead"
                    )
                );

                d.add_secondary_label((kiosk_loc, "Value originating from here"));
                context.add_diag(d);
            }

            return Some(vec![]);
        }

        Some(match &return_ty.value {
            Type_::Unit => vec![],
            Type_::Single(st) => vec![map_kiosk_value(st)],
            Type_::Multiple(types) => types.iter().map(map_kiosk_value).collect(),
        })
    }
}

impl SimpleDomain for State {
    type Value = Value;

    fn new(context: &CFGContext, mut locals: BTreeMap<Var, LocalState<Value>>) -> Self {
        for (_mut, v, st) in &context.signature.parameters {
            let local_state = locals.get_mut(v).unwrap();
            if let LocalState::Available(loc, _) = local_state {
                *local_state = LocalState::Available(*loc, map_kiosk_value(st));
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
            (v @ Value::Kiosk(loc1), Value::Kiosk(loc2)) => {
                if loc1 == loc2 {
                    *v
                } else {
                    Value::Kiosk(INVALID_LOC)
                }
            }
            (_, _) => Value::Other,
        }
    }

    fn join_impl(&mut self, _: &Self, _: &mut JoinResult) {}
}

impl SimpleExecutionContext for ExecutionContext {
    fn add_diag(&mut self, diag: Diagnostic) {
        self.diags.add(diag)
    }
}

fn map_kiosk_value(value: &SingleType) -> Value {
    if value
        .value
        .is_apply(&SUI_ADDR_VALUE, KIOSK_MOD_NAME, KIOSK_STRUCT_NAME)
        .is_some()
    {
        Value::Kiosk(value.loc)
    } else {
        Value::Other
    }
}
