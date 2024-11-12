// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This analysis flags potential custom implementations of transfer/share/freeze calls on objects
//! that already have a store ability and where "public" variants of these calls can be used. This
//! can be dangerous as custom transfer/share/freeze operation is becoming unenforceable in this
//! situation.  A function is considered a potential custom implementation if it takes as a
//! parameter an instance of a struct type defined in a given module with a store ability and passes
//! it as an argument to a "private" transfer/share/freeze call.

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
    hlir::ast::{
        BaseType_, Label, ModuleCall, SingleType, SingleType_, Type, TypeName_, Type_, Var,
    },
    parser::ast::Ability_,
    shared::Identifier,
    sui_mode::SUI_ADDR_VALUE,
};
use std::collections::BTreeMap;

use super::{
    LinterDiagnosticCategory, LinterDiagnosticCode, FREEZE_FUN, INVALID_LOC, LINT_WARNING_PREFIX,
    RECEIVE_FUN, SHARE_FUN, TRANSFER_FUN, TRANSFER_MOD_NAME,
};

const PRIVATE_OBJ_FUNCTIONS: &[(AccountAddress, &str, &str)] = &[
    (SUI_ADDR_VALUE, TRANSFER_MOD_NAME, TRANSFER_FUN),
    (SUI_ADDR_VALUE, TRANSFER_MOD_NAME, SHARE_FUN),
    (SUI_ADDR_VALUE, TRANSFER_MOD_NAME, FREEZE_FUN),
    (SUI_ADDR_VALUE, TRANSFER_MOD_NAME, RECEIVE_FUN),
];

const CUSTOM_STATE_CHANGE_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Sui as u8,
    LinterDiagnosticCode::CustomStateChange as u8,
    "potentially unenforceable custom transfer/share/freeze policy",
);

//**************************************************************************************************
// types
//**************************************************************************************************

pub struct CustomStateChangeVerifier;
pub struct CustomStateChangeVerifierAI {
    fn_name_loc: Loc,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Value {
    /// An instance of a struct defined within a given module with a store ability.
    LocalObjWithStore(Loc),
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

impl SimpleAbsIntConstructor for CustomStateChangeVerifier {
    type AI<'a> = CustomStateChangeVerifierAI;

    fn new<'a>(
        context: &'a CFGContext<'a>,
        cfg: &ImmForwardCFG,
        init_state: &mut State,
    ) -> Option<Self::AI<'a>> {
        let MemberName::Function(fn_name) = context.member else {
            return None;
        };

        if !init_state
            .locals
            .values()
            .any(|state| matches!(state, LocalState::Available(_, Value::LocalObjWithStore(_))))
        {
            // if there is no object parameter with store, we can skip the function
            // since this is the only case which will trigger the warning
            return None;
        }

        if !calls_special_function(PRIVATE_OBJ_FUNCTIONS, cfg) {
            // if the function does not call any of the private transfer functions, we can skip it
            return None;
        }

        Some(CustomStateChangeVerifierAI {
            fn_name_loc: fn_name.loc,
        })
    }
}

impl SimpleAbsInt for CustomStateChangeVerifierAI {
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
        _loc: &Loc,
        return_ty: &Type,
        f: &ModuleCall,
        args: Vec<Value>,
    ) -> Option<Vec<Value>> {
        if let Some((_, _, fname)) = PRIVATE_OBJ_FUNCTIONS
            .iter()
            .find(|(addr, module, fun)| f.is(addr, module, fun))
        {
            if let Value::LocalObjWithStore(obj_addr_loc) = args[0] {
                let (op, action) = match *fname {
                    TRANSFER_FUN => ("transfer", "transferred"),
                    SHARE_FUN => ("share", "shared"),
                    FREEZE_FUN => ("freeze", "frozen"),
                    RECEIVE_FUN => ("receive", "received"),
                    s => unimplemented!("Unexpected private obj function {s}"),
                };
                let msg = format!("Potential unintended implementation of a custom {op} function.");
                let uid_msg = format!(
                    "Instances of a type with a 'store' ability can be {action} using \
                    the 'public_{fname}' function which often negates the intent \
                    of enforcing a custom {op} policy"
                );
                let note_msg = format!(
                    "A custom {op} policy for a given type is implemented through \
                    calling the private '{fname}' function variant in the module defining this type"
                );
                let mut d = diag!(
                    CUSTOM_STATE_CHANGE_DIAG,
                    (self.fn_name_loc, msg),
                    (f.name.loc(), uid_msg)
                );
                d.add_note(note_msg);
                if obj_addr_loc != INVALID_LOC {
                    let loc_msg = format!(
                        "An instance of a module-private type with a \
                        'store' ability to be {action} coming from here"
                    );
                    d.add_secondary_label((obj_addr_loc, loc_msg));
                }
                context.add_diag(d)
            }
        }
        Some(match &return_ty.value {
            Type_::Unit => vec![],
            Type_::Single(_) => vec![Value::Other],
            Type_::Multiple(types) => vec![Value::Other; types.len()],
        })
    }
}

fn is_local_obj_with_store(sp!(_, st_): &SingleType, context: &CFGContext) -> bool {
    let sp!(_, bt_) = match st_ {
        SingleType_::Base(v) => v,
        // transfer/share/freeze take objects by value so even if by-reference object has store and
        // is module-local, it could not end up being an argument to one of these functions
        SingleType_::Ref(_, _) => return false,
    };
    if let BaseType_::Apply(abilities, sp!(_, tname), _) = bt_ {
        if !abilities.has_ability_(Ability_::Store) {
            // no store ability
            return false;
        }
        if let TypeName_::ModuleType(mident, _) = tname {
            if mident.value == context.module.value {
                return true;
            }
        }
    }
    false
}

impl SimpleDomain for State {
    type Value = Value;

    fn new(context: &CFGContext, mut locals: BTreeMap<Var, LocalState<Value>>) -> Self {
        for (_mut, v, st) in &context.signature.parameters {
            if is_local_obj_with_store(st, context) {
                let local_state = locals.get_mut(v).unwrap();
                debug_assert!(matches!(local_state, LocalState::Available(_, _)));
                if let LocalState::Available(loc, _) = local_state {
                    *local_state = LocalState::Available(*loc, Value::LocalObjWithStore(*loc));
                }
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
            (obj @ Value::LocalObjWithStore(loc1), Value::LocalObjWithStore(loc2)) => {
                if loc1 == loc2 {
                    *obj
                } else {
                    Value::LocalObjWithStore(INVALID_LOC)
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
