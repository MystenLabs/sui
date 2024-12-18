// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This analysis flags making objects passed as function parameters or resulting from unpacking
//! (likely already owned) shareable which would lead to an abort. A typical patterns is to create a
//! fresh object and share it within the same function

use crate::{
    cfgir::{
        absint::JoinResult,
        cfg::ImmForwardCFG,
        visitor::{
            calls_special_function, LocalState, SimpleAbsInt, SimpleAbsIntConstructor,
            SimpleDomain, SimpleExecutionContext,
        },
        CFGContext,
    },
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        Diagnostic, Diagnostics,
    },
    expansion::ast::ModuleIdent,
    hlir::ast::{
        BaseType, BaseType_, Exp, LValue, LValue_, Label, ModuleCall, SingleType, SingleType_,
        Type, TypeName_, Type_, UnannotatedExp_, Var,
    },
    naming::ast::BuiltinTypeName_,
    parser::ast::{Ability_, DatatypeName},
    shared::{
        program_info::{DatatypeKind, TypingProgramInfo},
        Identifier,
    },
    sui_mode::{
        info::{SuiInfo, TransferKind},
        linters::{
            type_abilities, LinterDiagnosticCategory, LinterDiagnosticCode, LINT_WARNING_PREFIX,
            PUBLIC_SHARE_FUN, SHARE_FUN, TRANSFER_MOD_NAME,
        },
        SUI_ADDR_VALUE, TX_CONTEXT_MODULE_NAME, TX_CONTEXT_TYPE_NAME,
    },
};
use move_core_types::account_address::AccountAddress;
use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use std::collections::BTreeMap;

const SHARE_FUNCTIONS: &[(AccountAddress, &str, &str)] = &[
    (SUI_ADDR_VALUE, TRANSFER_MOD_NAME, PUBLIC_SHARE_FUN),
    (SUI_ADDR_VALUE, TRANSFER_MOD_NAME, SHARE_FUN),
];

const SHARE_OWNED_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Sui as u8,
    LinterDiagnosticCode::ShareOwned as u8,
    "possible owned object share",
);

//**************************************************************************************************
// types
//**************************************************************************************************

pub struct ShareOwnedVerifier;

pub struct ShareOwnedVerifierAI<'a> {
    info: &'a TypingProgramInfo,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Value {
    /// a fresh object resulting from packing
    FreshObj,
    /// a most likely non-fresh object coming from unpacking or a function argument
    NotFreshObj(Loc),
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

impl SimpleAbsIntConstructor for ShareOwnedVerifier {
    type AI<'a> = ShareOwnedVerifierAI<'a>;

    fn new<'a>(
        context: &'a CFGContext<'a>,
        cfg: &ImmForwardCFG,
        _init_state: &mut <Self::AI<'a> as SimpleAbsInt>::State,
    ) -> Option<Self::AI<'a>> {
        if context.attributes.is_test_or_test_only()
            || context
                .info
                .module(&context.module)
                .attributes
                .is_test_or_test_only()
        {
            return None;
        }
        if !calls_special_function(SHARE_FUNCTIONS, cfg) {
            return None;
        }
        Some(ShareOwnedVerifierAI { info: context.info })
    }
}

impl<'a> SimpleAbsInt for ShareOwnedVerifierAI<'a> {
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
        use UnannotatedExp_ as E;

        if let E::Pack(_, _, fields) = &e.exp.value {
            for (_, _, inner) in fields.iter() {
                self.exp(context, state, inner);
            }
            return Some(vec![Value::FreshObj]);
        };

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
        if SHARE_FUNCTIONS
            .iter()
            .any(|(addr, module, fun)| f.is(addr, module, fun))
            && args.first().is_some_and(|v| v != &Value::FreshObj)
        {
            self.maybe_warn_share_owned(context, loc, f, args)
        }
        let all_args_pure = !f.arguments.iter().any(|a| self.can_hold_obj(&a.ty));
        Some(match &return_ty.value {
            Type_::Unit => vec![],
            Type_::Single(t) => {
                let v = if all_args_pure || !is_obj_type(t) {
                    Value::Other
                } else {
                    Value::NotFreshObj(t.loc)
                };
                vec![v]
            }
            Type_::Multiple(types) => types
                .iter()
                .map(|t| {
                    if all_args_pure || !is_obj_type(t) {
                        Value::Other
                    } else {
                        Value::NotFreshObj(t.loc)
                    }
                })
                .collect(),
        })
    }

    fn lvalue_custom(
        &self,
        context: &mut ExecutionContext,
        state: &mut State,
        l: &LValue,
        _value: &Value,
    ) -> bool {
        use LValue_ as L;

        let sp!(_, l_) = l;
        if let L::Unpack(_, _, fields) = l_ {
            for (f, l) in fields {
                let v = if is_obj(l) {
                    Value::NotFreshObj(f.loc())
                } else {
                    Value::default()
                };
                self.lvalue(context, state, l, v)
            }
            return true;
        }
        false
    }
}

impl<'a> ShareOwnedVerifierAI<'a> {
    fn can_hold_obj(&self, sp!(_, ty_): &Type) -> bool {
        match ty_ {
            Type_::Unit => false,
            Type_::Single(st) => self.can_hold_obj_single(st),
            Type_::Multiple(sts) => sts.iter().any(|st| self.can_hold_obj_single(st)),
        }
    }

    fn can_hold_obj_single(&self, sp!(_, st_): &SingleType) -> bool {
        match st_ {
            SingleType_::Base(bt) | SingleType_::Ref(_, bt) => self.can_hold_obj_base(bt),
        }
    }

    #[growing_stack]
    fn can_hold_obj_base(&self, sp!(_, bt_): &BaseType) -> bool {
        match bt_ {
            // special case TxContext as not holding an object
            BaseType_::Apply(_, sp!(_, tn), _)
                if tn.is(
                    &SUI_ADDR_VALUE,
                    TX_CONTEXT_MODULE_NAME,
                    TX_CONTEXT_TYPE_NAME,
                ) =>
            {
                false
            }
            // vector in value might have an object
            BaseType_::Apply(
                _,
                sp!(_, TypeName_::Builtin(sp!(_, BuiltinTypeName_::Vector))),
                bs,
            ) => bs.iter().any(|b| self.can_hold_obj_base(b)),
            // builtins cannot hold objects
            BaseType_::Apply(_, sp!(_, TypeName_::Builtin(_)), _) => false,

            BaseType_::Apply(_, sp!(_, TypeName_::ModuleType(m, n)), targs) => {
                let m = *m;
                let n = *n;
                if self.sui_info().uid_holders.contains_key(&(m, n)) {
                    return true;
                }
                let phantom_positions = phantom_positions(self.info, &m, &n);
                phantom_positions
                    .into_iter()
                    .zip(targs)
                    .filter(|(is_phantom, _)| !*is_phantom)
                    .any(|(_, t)| self.can_hold_obj_base(t))
            }
            // any user defined type or type parameter is pessimistically assumed to hold an object
            BaseType_::Param(_) => true,
            BaseType_::Unreachable | BaseType_::UnresolvedError => false,
        }
    }

    fn maybe_warn_share_owned(
        &self,
        context: &mut ExecutionContext,
        loc: &Loc,
        f: &ModuleCall,
        args: Vec<Value>,
    ) {
        let Value::NotFreshObj(not_fresh_loc) = &args[0] else {
            return;
        };
        let Some(tn) = f
            .type_arguments
            .first()
            .and_then(|t| t.value.type_name())
            .and_then(|n| n.value.datatype_name())
        else {
            return;
        };
        let Some(transferred_kind) = self.sui_info().transferred.get(&tn) else {
            return;
        };

        let msg =
            "Potential abort from a (potentially) owned object created by a different transaction.";
        let uid_msg = "Creating a fresh object and sharing it within the same function will \
            ensure this does not abort.";
        let not_fresh_msg = "A potentially owned object coming from here";
        let (tloc, tmsg) = match transferred_kind {
            TransferKind::PublicTransfer(store_loc) => (
                store_loc,
                "Potentially an owned object because 'store' grants access to public transfers",
            ),
            TransferKind::PrivateTransfer(loc) => (loc, "Transferred as an owned object here"),
        };
        let d = diag!(
            SHARE_OWNED_DIAG,
            (*loc, msg),
            (f.arguments[0].exp.loc, uid_msg),
            (*not_fresh_loc, not_fresh_msg),
            (*tloc, tmsg),
        );

        context.add_diag(d)
    }

    fn sui_info(&self) -> &'a SuiInfo {
        self.info.sui_flavor_info.as_ref().unwrap()
    }
}

fn is_obj(sp!(_, l_): &LValue) -> bool {
    if let LValue_::Var { ty: st, .. } = l_ {
        return is_obj_type(st);
    }
    false
}

fn is_obj_type(st_: &SingleType) -> bool {
    let Some(abilities) = type_abilities(st_) else {
        return false;
    };
    abilities.has_ability_(Ability_::Key)
}

fn phantom_positions(
    info: &TypingProgramInfo,
    m: &ModuleIdent,
    n: &DatatypeName,
) -> Vec</* is_phantom */ bool> {
    let ty_params = match info.datatype_kind(m, n) {
        DatatypeKind::Struct => &info.struct_definition(m, n).type_parameters,
        DatatypeKind::Enum => &info.enum_definition(m, n).type_parameters,
    };
    ty_params.iter().map(|tp| tp.is_phantom).collect()
}

impl SimpleDomain for State {
    type Value = Value;

    fn new(context: &CFGContext, mut locals: BTreeMap<Var, LocalState<Value>>) -> Self {
        for (_mut, v, st) in &context.signature.parameters {
            if is_obj_type(st) {
                let local_state = locals.get_mut(v).unwrap();
                if let LocalState::Available(loc, _) = local_state {
                    *local_state = LocalState::Available(*loc, Value::NotFreshObj(*loc));
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
            (Value::FreshObj, Value::FreshObj) => Value::FreshObj,
            (stale @ Value::NotFreshObj(_), _) | (_, stale @ Value::NotFreshObj(_)) => *stale,
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
