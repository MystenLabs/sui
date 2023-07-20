// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This analysis flags potential custom implementations of transfer/share/freeze calls on objects
//! that already have a store ability and where "public" variants of these calls can be used. This
//! can be dangerous as custom transfer/share/freeze operation is becoming unenforceable in this
//! situation.  A function is considered a potential custom implementation if it takes as a
//! parameter an instance of a struct type defined in a given module with a store ability and passes
//! it as an argument to a "private" transfer/share/freeze call.

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
    hlir::ast::{
        BaseType_, Command, Exp, LValue, Label, ModuleCall, SingleType, SingleType_, Type,
        TypeName_, Type_, Var,
    },
    parser::ast::Ability_,
    shared::{CompilationEnv, Identifier},
};
use std::collections::BTreeMap;

use super::{
    CUSTOM_STATE_CHANGE_DIAG_CATEGORY, CUSTOM_STATE_CHANGE_DIAG_CODE, INVALID_LOC,
    LINT_WARNING_PREFIX,
};

const TRANSFER_FUN: &str = "transfer";
const SHARE_FUN: &str = "share_object";
const FREEZE_FUN: &str = "freeze_object";

const PRIVATE_OBJ_FUNCTIONS: &[(&str, &str, &str)] = &[
    ("sui", "transfer", TRANSFER_FUN),
    ("sui", "transfer", SHARE_FUN),
    ("sui", "transfer", FREEZE_FUN),
];

const CUSTOM_STATE_CHANGE_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    CUSTOM_STATE_CHANGE_DIAG_CATEGORY,
    CUSTOM_STATE_CHANGE_DIAG_CODE,
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
        _env: &CompilationEnv,
        _program: &'a Program,
        context: &'a CFGContext<'a>,
        _init_state: &mut <Self::AI<'a> as SimpleAbsInt>::State,
    ) -> Option<Self::AI<'a>> {
        let Some(_) = &context.module else {
            return None
        };
        let MemberName::Function(fn_name) = context.member else {
            return None;
        };

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
                let msg = format!(
                    "Potential unintended implementation of a custom {} function.",
                    fname
                );
                let (op, action) = if *fname == TRANSFER_FUN {
                    ("transfer", "transferred")
                } else if *fname == SHARE_FUN {
                    ("share", "shared")
                } else {
                    ("freeze", "frozen")
                };
                let uid_msg = format!(
                    "Instances of a type with a store ability can be {action} using \
                                       the public_{fname} function which often negates the intent \
                                       of enforcing a custom {op} policy"
                );
                let note_msg = format!("A custom {op} policy for a given type is implemented through calling \
                                       the private {fname} function variant in the module defining this type");
                let mut d = diag!(
                    CUSTOM_STATE_CHANGE_DIAG,
                    (self.fn_name_loc, msg),
                    (f.name.loc(), uid_msg)
                );
                d.add_note(note_msg);
                if obj_addr_loc != INVALID_LOC {
                    let loc_msg = format!("An instance of a module-private type with a store ability to be {} coming from here", action);
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
            if let Some(current_mident) = context.module {
                if mident.value == current_mident.value {
                    return true;
                }
            }
        }
    }
    false
}

impl SimpleDomain for State {
    type Value = Value;

    fn new(context: &CFGContext, mut locals: BTreeMap<Var, LocalState<Value>>) -> Self {
        for (v, st) in &context.signature.parameters {
            if is_local_obj_with_store(st, context) {
                let local_state = locals.get_mut(v).unwrap();
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
