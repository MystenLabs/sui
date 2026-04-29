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
            LocalState, SimpleAbsInt, SimpleAbsIntConstructor, SimpleDomain, SimpleExecutionContext,
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
    /// The object reference itself, not yet accessed through a field
    UnusedObj(Var, Loc),
    /// A value derived from accessing a field of the tracked object
    FieldOf(Var, Loc),
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
            if is_qualifying_obj(context.info, context.pre_compiled_program.as_deref(), st) {
                tracked_params.insert(*v, v.0.loc);
                let locals = init_state.locals_mut();
                let LocalState::Available(loc, val) = locals.get_mut(v).unwrap() else {
                    unreachable!("parameter must be available at init")
                };
                *val = Value::UnusedObj(*v, *loc);
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
        match &cmd.value {
            C::Mutate(lhs, rhs) => {
                self.exp(context, state, rhs);
                let lhs_vals = self.exp(context, state, lhs);
                self.mark_unused_values(&lhs_vals);
                true
            }
            C::Return { exp, .. } => {
                let vals = self.exp(context, state, exp);
                // Only field-derived values count as usage; returning the
                // object reference itself is just passing it through.
                for v in &vals {
                    if let Value::FieldOf(var, _) = v {
                        self.used_params.borrow_mut().insert(*var);
                    }
                }
                true
            }
            _ => false,
        }
    }

    fn exp_custom(
        &self,
        context: &mut ExecutionContext,
        state: &mut State,
        e: &Exp,
    ) -> Option<Vec<Value>> {
        use UnannotatedExp_ as E;
        match &e.exp.value {
            // Field access: promote UnusedObj → FieldOf to distinguish
            // "object passed through" from "field actually accessed"
            E::Borrow(_, inner, _, _) => {
                let vals = self.exp(context, state, inner);
                Some(
                    vals.into_iter()
                        .map(|v| match v {
                            Value::UnusedObj(var, loc) => Value::FieldOf(var, loc),
                            other => other,
                        })
                        .collect(),
                )
            }
            // The default handler discards sub-expression values for these.
            // Propagate tracking so downstream consumers can mark usage.
            E::Dereference(inner) | E::Freeze(inner) | E::UnaryExp(_, inner) => {
                Some(self.exp(context, state, inner))
            }
            E::BinopExp(e1, _, e2) => {
                let v1 = self.exp(context, state, e1);
                let v2 = self.exp(context, state, e2);
                self.mark_unused_values(&v1);
                self.mark_unused_values(&v2);
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
            match v {
                Value::UnusedObj(var, _) | Value::FieldOf(var, _) => {
                    self.used_params.borrow_mut().insert(*var);
                }
                Value::Other => {}
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
            (Value::Other, _) | (_, Value::Other) => Value::Other,
            (Value::FieldOf(var, loc), _) | (_, Value::FieldOf(var, loc)) => {
                Value::FieldOf(*var, *loc)
            }
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
