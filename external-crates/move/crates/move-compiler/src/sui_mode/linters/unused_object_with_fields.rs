// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This analysis flags objects with fields (key ability, id: UID, at least one other field,
//! no type parameters) passed as function parameters that are not used in the function body —
//! not passed to another function, not read, and not written.

use crate::{
    PreCompiledProgramInfo,
    cfgir::{
        CFGContext,
        absint::JoinResult,
        cfg::{CFG, ImmForwardCFG},
        visitor::{
            LocalState, SimpleAbsInt, SimpleAbsIntConstructor, SimpleDomain,
            SimpleExecutionContext,
        },
    },
    diag,
    diagnostics::{
        Diagnostic, Diagnostics,
        codes::{DiagnosticInfo, Severity, custom},
    },
    hlir::ast::{
        BaseType_, Command, Command_ as C, Exp, Label, ModuleCall, SingleType, SingleType_, Type,
        TypeName_, UnannotatedExp_, Var,
    },
    naming::ast::StructFields,
    parser::ast::Ability_,
    shared::program_info::TypingProgramInfo,
    sui_mode::linters::{LINT_WARNING_PREFIX, LinterDiagnosticCategory, LinterDiagnosticCode},
};
use move_ir_types::location::*;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

const UNUSED_OBJ_WITH_FIELDS_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Sui as u8,
    LinterDiagnosticCode::UnusedObjWithFields as u8,
    "unused object with fields",
);

//**************************************************************************************************
// types
//**************************************************************************************************

pub struct UnusedObjWithFieldsVerifier;

pub struct UnusedObjWithFieldsAI {
    tracked_params: BTreeMap<Var, Loc>,
    used_params: RefCell<BTreeSet<Var>>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Value {
    /// Tracks the original parameter variable through assignments
    UnusedObj(Var, Loc),
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

impl SimpleAbsIntConstructor for UnusedObjWithFieldsVerifier {
    type AI<'a> = UnusedObjWithFieldsAI;

    fn new<'a>(
        context: &'a CFGContext<'a>,
        cfg: &ImmForwardCFG,
        init_state: &mut <Self::AI<'a> as SimpleAbsInt>::State,
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

        // Skip functions that always abort — the object is intentionally consumed
        if always_aborts(cfg) {
            return None;
        }

        let mut tracked_params = BTreeMap::new();
        for (_mutability, v, st) in &context.signature.parameters {
            if is_qualifying_obj(
                context.info,
                context.pre_compiled_program.as_deref(),
                st,
            ) {
                tracked_params.insert(*v, v.0.loc);
                let locals = init_state.locals_mut();
                if let Some(LocalState::Available(loc, _)) = locals.get(v) {
                    let loc = *loc;
                    locals.insert(*v, LocalState::Available(loc, Value::UnusedObj(*v, loc)));
                }
            }
        }
        if tracked_params.is_empty() {
            return None;
        }

        Some(UnusedObjWithFieldsAI {
            tracked_params,
            used_params: RefCell::new(BTreeSet::new()),
        })
    }
}

/// Returns true if every terminal block in the CFG ends with an abort.
fn always_aborts(cfg: &ImmForwardCFG) -> bool {
    let mut has_terminal = false;
    for lbl in cfg.block_labels() {
        if cfg.successors(lbl).is_empty() {
            has_terminal = true;
            let ends_with_abort = cfg
                .commands(lbl)
                .last()
                .is_some_and(|(_, sp!(_, cmd))| matches!(cmd, C::Abort(_, _)));
            if !ends_with_abort {
                return false;
            }
        }
    }
    has_terminal
}

/// Checks whether a parameter type is a Sui object with key ability, no type parameters,
/// id: UID field, and at least one additional field.
fn is_qualifying_obj(
    info: &TypingProgramInfo,
    pre_compiled: Option<&PreCompiledProgramInfo>,
    st: &SingleType,
) -> bool {
    let bt = match &st.value {
        // Only check reference parameters — by-value consumption is intentional
        SingleType_::Ref(_, b) => b,
        SingleType_::Base(_) => return false,
    };
    let BaseType_::Apply(abilities, sp!(_, TypeName_::ModuleType(m, n)), _) = &bt.value else {
        return false;
    };
    if !abilities.has_ability_(Ability_::Key) {
        return false;
    }

    let sdef = info
        .struct_definition_opt(m, n)
        .or_else(|| pre_compiled?.module_info(m)?.structs.get(n));
    let Some(sdef) = sdef else {
        return false;
    };

    if !sdef.type_parameters.is_empty() {
        return false;
    }

    let StructFields::Defined(_, fields) = &sdef.fields else {
        return false;
    };
    // id: UID (guaranteed by Sui type checker for key objects) plus at least one other field
    fields.len() >= 2
}

impl SimpleAbsInt for UnusedObjWithFieldsAI {
    type State = State;
    type ExecutionContext = ExecutionContext;

    fn finish(
        &mut self,
        _final_states: BTreeMap<Label, State>,
        mut diags: Diagnostics,
    ) -> Diagnostics {
        let used = self.used_params.borrow();
        for (var, loc) in &self.tracked_params {
            if !used.contains(var) {
                diags.add(diag!(
                    UNUSED_OBJ_WITH_FIELDS_DIAG,
                    (
                        *loc,
                        "Unused object with fields. Consider reading or writing \
                         the object's fields, or passing it to another function."
                    ),
                ));
            }
        }
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

    fn command_custom(
        &self,
        context: &mut ExecutionContext,
        state: &mut State,
        cmd: &Command,
    ) -> bool {
        // Returning a tracked ref marks it as used
        if let sp!(_, C::Return { exp: e, .. }) = cmd {
            let vals = self.exp(context, state, e);
            self.mark_unused_values(&vals);
            return true;
        }
        false
    }

    fn exp_custom(
        &self,
        context: &mut ExecutionContext,
        state: &mut State,
        e: &Exp,
    ) -> Option<Vec<Value>> {
        use UnannotatedExp_ as E;
        match &e.exp.value {
            E::BorrowLocal(_, var) => {
                // Direct borrow of a tracked parameter
                if self.tracked_params.contains_key(var) {
                    self.used_params.borrow_mut().insert(*var);
                }
                // Transitive: the local might hold a value assigned from a tracked param
                if let Some(LocalState::Available(_, Value::UnusedObj(orig_var, _))) =
                    state.locals().get(var)
                {
                    self.used_params.borrow_mut().insert(*orig_var);
                }
                None
            }
            // Field access / deref / freeze on a ref: the default handler evaluates the
            // inner expression but discards its abstract value. We intercept to check
            // whether the inner value tracks an unused ref param.
            E::Borrow(_, inner, _, _) | E::Dereference(inner) | E::Freeze(inner) => {
                let vals = self.exp(context, state, inner);
                self.mark_unused_values(&vals);
                Some(vec![Value::default()])
            }
            _ => None,
        }
    }

    fn call_custom(
        &self,
        _context: &mut ExecutionContext,
        _state: &mut State,
        _loc: &Loc,
        _return_ty: &Type,
        _f: &ModuleCall,
        args: Vec<Value>,
    ) -> Option<Vec<Value>> {
        // If a tracked ref flows into a function call, mark it as used
        self.mark_unused_values(&args);
        None
    }


}

impl UnusedObjWithFieldsAI {
    fn mark_unused_values(&self, values: &[Value]) {
        for v in values {
            if let Value::UnusedObj(var, _) = v {
                self.used_params.borrow_mut().insert(*var);
            }
        }
    }
}

impl SimpleDomain for State {
    type Value = Value;

    fn new(_context: &CFGContext, locals: BTreeMap<Var, LocalState<Value>>) -> Self {
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
            // If used on any path, consider it used overall
            (Value::Other, _) | (_, Value::Other) => Value::Other,
            (Value::UnusedObj(var, loc), Value::UnusedObj(_, _)) => Value::UnusedObj(*var, *loc),
        }
    }

    fn join_impl(&mut self, _: &Self, _: &mut JoinResult) {}
}

impl SimpleExecutionContext for ExecutionContext {
    fn add_diag(&mut self, diag: Diagnostic) {
        self.diags.add(diag)
    }
}
