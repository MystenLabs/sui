// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfgir::{
        absint::JoinResult,
        cfg::ImmForwardCFG,
        visitor::{
            cfg_satisfies, LocalState, SimpleAbsInt, SimpleAbsIntConstructor, SimpleDomain,
            SimpleExecutionContext,
        },
        CFGContext, MemberName,
    },
    diag,
    diagnostics::{Diagnostic, Diagnostics},
    editions::Flavor,
    expansion::ast::ModuleIdent,
    hlir::ast::{self as H, Exp, Label, ModuleCall, SingleType, Type, Type_, Var},
    parser::ast::{Ability_, TargetKind},
    shared::{program_info::TypingProgramInfo, Identifier},
    sui_mode::{
        AUTHENTICATOR_STATE_CREATE, AUTHENTICATOR_STATE_MODULE_NAME, BRIDGE_ADDR_VALUE,
        BRIDGE_CREATE, BRIDGE_MODULE_NAME, CLOCK_MODULE_NAME, DENY_LIST_CREATE,
        DENY_LIST_MODULE_NAME, ID_LEAK_DIAG, OBJECT_MODULE_NAME, OBJECT_NEW,
        OBJECT_NEW_UID_FROM_HASH, RANDOMNESS_MODULE_NAME, RANDOMNESS_STATE_CREATE, SUI_ADDR_NAME,
        SUI_ADDR_VALUE, SUI_CLOCK_CREATE, SUI_SYSTEM_ADDR_VALUE, SUI_SYSTEM_CREATE,
        SUI_SYSTEM_MODULE_NAME, TEST_SCENARIO_MODULE_NAME, TS_NEW_OBJECT, UID_TYPE_NAME,
    },
};
use move_core_types::account_address::AccountAddress;
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::collections::BTreeMap;

pub const FRESH_ID_FUNCTIONS: &[(AccountAddress, Symbol, Symbol)] = &[
    (SUI_ADDR_VALUE, OBJECT_MODULE_NAME, OBJECT_NEW),
    (SUI_ADDR_VALUE, OBJECT_MODULE_NAME, OBJECT_NEW_UID_FROM_HASH),
    (SUI_ADDR_VALUE, TEST_SCENARIO_MODULE_NAME, TS_NEW_OBJECT),
];
pub const FUNCTIONS_TO_SKIP: &[(AccountAddress, Symbol, Symbol)] = &[
    (
        SUI_SYSTEM_ADDR_VALUE,
        SUI_SYSTEM_MODULE_NAME,
        SUI_SYSTEM_CREATE,
    ),
    (SUI_ADDR_VALUE, CLOCK_MODULE_NAME, SUI_CLOCK_CREATE),
    (
        SUI_ADDR_VALUE,
        AUTHENTICATOR_STATE_MODULE_NAME,
        AUTHENTICATOR_STATE_CREATE,
    ),
    (
        SUI_ADDR_VALUE,
        RANDOMNESS_MODULE_NAME,
        RANDOMNESS_STATE_CREATE,
    ),
    (SUI_ADDR_VALUE, DENY_LIST_MODULE_NAME, DENY_LIST_CREATE),
    (BRIDGE_ADDR_VALUE, BRIDGE_MODULE_NAME, BRIDGE_CREATE),
];

//**************************************************************************************************
// types
//**************************************************************************************************

pub struct IDLeakVerifier;
pub struct IDLeakVerifierAI<'a> {
    module: &'a ModuleIdent,
    info: &'a TypingProgramInfo,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Value {
    FreshID(Loc),
    NotFresh(Loc),
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

impl SimpleAbsIntConstructor for IDLeakVerifier {
    type AI<'a> = IDLeakVerifierAI<'a>;

    fn new<'a>(
        context: &'a CFGContext<'a>,
        cfg: &ImmForwardCFG,
        _init_state: &mut <Self::AI<'a> as SimpleAbsInt>::State,
    ) -> Option<Self::AI<'a>> {
        let module = &context.module;
        let minfo = context.info.module(module);
        let package_name = minfo.package;
        let config = context.env.package_config(package_name);
        if config.flavor != Flavor::Sui {
            // Skip if not sui
            return None;
        }
        if !matches!(
            minfo.target_kind,
            TargetKind::Source {
                is_root_package: true
            }
        ) {
            // Skip non-source, dependency modules
            return None;
        }

        if let MemberName::Function(n) = &context.member {
            let should_skip = FUNCTIONS_TO_SKIP
                .iter()
                .any(|to_skip| module.value.is(&to_skip.0, to_skip.1) && n.value == to_skip.2);
            if should_skip {
                return None;
            }
        }

        // skip any function that doesn't create an object
        cfg_satisfies(
            cfg,
            |_| true,
            |e| {
                use H::UnannotatedExp_ as E;
                matches!(
                    &e.exp.value,
                    E::Pack(s, _, _) if minfo.structs.get(s).is_some_and(|s| s.abilities.has_ability_(Ability_::Key)),
                )
            },
        );

        Some(IDLeakVerifierAI {
            module,
            info: context.info,
        })
    }
}

impl<'a> SimpleAbsInt for IDLeakVerifierAI<'a> {
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
        context: &mut ExecutionContext,
        state: &mut State,
        e: &Exp,
    ) -> Option<Vec<Value>> {
        use H::UnannotatedExp_ as E;

        let e__ = &e.exp.value;
        let E::Pack(s, _tys, fields) = e__ else {
            return None;
        };
        let abilities = {
            let minfo = self.info.module(self.module);
            &minfo.structs.get(s)?.abilities
        };
        if !abilities.has_ability_(Ability_::Key) {
            return None;
        }

        let mut fields_iter = fields.iter();
        let (f, _, first_e) = fields_iter.next().unwrap();
        let first_value = self.exp(context, state, first_e).pop().unwrap_or_default();
        if !matches!(first_value, Value::FreshID(_)) {
            let msg = "Invalid object creation without a newly created UID.".to_string();
            let uid_msg = format!(
                "The UID must come directly from {sui}::{object}::{new}. \
                Or for tests, it can come from {sui}::{ts}::{ts_new}",
                sui = SUI_ADDR_NAME,
                object = OBJECT_MODULE_NAME,
                new = OBJECT_NEW,
                ts = TEST_SCENARIO_MODULE_NAME,
                ts_new = TS_NEW_OBJECT,
            );
            let mut d = diag!(ID_LEAK_DIAG, (e.exp.loc, msg), (f.loc(), uid_msg));
            if let Value::NotFresh(stale) = first_value {
                d.add_secondary_label((stale, "Non fresh UID from this position"))
            }
            context.add_diag(d)
        }

        for (_, _, inner) in fields_iter {
            self.exp(context, state, inner);
        }

        Some(vec![Value::default()])
    }

    fn call_custom(
        &self,
        _context: &mut ExecutionContext,
        _state: &mut State,
        loc: &Loc,
        return_ty: &Type,
        f: &ModuleCall,
        _args: Vec<Value>,
    ) -> Option<Vec<Value>> {
        if FRESH_ID_FUNCTIONS
            .iter()
            .any(|makes_fresh| f.is(&makes_fresh.0, makes_fresh.1, makes_fresh.2))
        {
            return Some(vec![Value::FreshID(*loc)]);
        }
        Some(match &return_ty.value {
            Type_::Unit => vec![],
            Type_::Single(t) => vec![value_for_ty(loc, t)],
            Type_::Multiple(ts) => ts.iter().map(|t| value_for_ty(loc, t)).collect(),
        })
    }
}

fn value_for_ty(loc: &Loc, sp!(_, t): &SingleType) -> Value {
    if t.is_apply(&SUI_ADDR_VALUE, OBJECT_MODULE_NAME, UID_TYPE_NAME)
        .is_some()
    {
        Value::NotFresh(*loc)
    } else {
        Value::Other
    }
}

impl SimpleDomain for State {
    type Value = Value;

    fn new(_: &CFGContext, locals: BTreeMap<Var, LocalState<Value>>) -> Self {
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
            (stale @ Value::NotFresh(_), _) | (_, stale @ Value::NotFresh(_)) => *stale,

            (Value::FreshID(_), Value::FreshID(_)) => *v1,

            (Value::FreshID(_), Value::Other)
            | (Value::Other, Value::FreshID(_))
            | (Value::Other, Value::Other) => Value::Other,
        }
    }

    fn join_impl(&mut self, _: &Self, _: &mut JoinResult) {}
}

impl SimpleExecutionContext for ExecutionContext {
    fn add_diag(&mut self, diag: Diagnostic) {
        self.diags.add(diag)
    }
}
