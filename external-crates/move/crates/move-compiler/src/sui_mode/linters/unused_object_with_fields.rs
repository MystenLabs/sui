// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Flags objects with fields (key ability, id: UID, at least one other field,
//! no type parameters) passed as function parameters that never have any
//! observable use in the function body.
//!
//! ## What counts as a use
//!
//! - reading the underlying value of a field — `dereference`, `unary`,
//!   `cast` — even if the result is dropped (`let _ = o.field`);
//! - the value flows into a function call (anything passed as an argument);
//! - either operand of a `binop` (e.g. `==`, `!=`, arithmetic);
//! - the value is the field-derived result of `return`;
//! - the value is the LHS of `mutate` and resolves to a tracked target —
//!   i.e. the write is into a `&mut` field of an input parameter;
//! - the value is the condition of a branch or variant switch;
//! - the value is the argument of an `abort`.
//!
//! Pure pass-throughs that don't on their own count: `borrow`, `freeze`,
//! `assign`, `pack`, and `let _ = e` (ignore-and-pop). In particular `borrow`
//! is needed to access a field, but the use is whatever the borrow flows into.
//!
//! ## How the analysis tracks "unused"
//!
//! Each [`State`] carries a `used: BTreeSet<Var>` that records tracked roots
//! marked as used along some path reaching the current program point. The set
//! grows monotonically and joins by union. At [`SimpleAbsInt::finish`] we union
//! every reachable post-state; roots absent from that union are flagged.
//!
//! Each tracked value carries a per-root [`Kind`] — `Bare` (a reference to
//! the root param itself) or `FieldDerived` (the value went through a
//! `Borrow`). `Borrow` lifts `Bare -> FieldDerived`; [`Kind::join`] keeps the
//! field-derived tag if any incoming path produced one.

use crate::{
    PreCompiledProgramInfo,
    cfgir::{
        CFGContext,
        absint::{BlockStates, JoinResult},
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
use std::collections::{BTreeMap, BTreeSet};

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
}

/// Ordered so that `Bare < FieldDerived`; the lattice join is `max`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// Bare reference to the root param itself (`&c`). A pass-through.
    Bare,
    /// Field-derived view of the root (`&c.f`, or any further `&.g`).
    FieldDerived,
}

impl Kind {
    fn join(self, other: Self) -> Self {
        self.max(other)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Value {
    /// Map from contributing root params to their per-root `Kind`. Multiple
    /// roots collect when control flow merges or when an expression mixes
    /// values from several tracked params.
    Tracked(BTreeMap<Var, Kind>),
    #[default]
    Other,
}

pub struct ExecutionContext {
    diags: Diagnostics,
}

#[derive(Clone, Debug)]
pub struct State {
    locals: BTreeMap<Var, LocalState<Value>>,
    /// Tracked roots marked as used along a path reaching this program point.
    used: BTreeSet<Var>,
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

        // Skip functions that always abort — the object is intentionally consumed.
        if always_aborts(cfg) {
            return None;
        }

        let mut tracked_params = BTreeMap::new();
        for (_mutability, v, st) in &context.signature.parameters {
            if !is_qualifying_obj(context.info, context.pre_compiled_program.as_deref(), st) {
                continue;
            }
            let locals = init_state.locals_mut();
            let Some(LocalState::Available(_, val)) = locals.get_mut(v) else {
                debug_assert!(false, "parameter must be available at init");
                continue;
            };
            *val = Value::Tracked(BTreeMap::from([(*v, Kind::Bare)]));
            tracked_params.insert(*v, v.0.loc);
        }
        if tracked_params.is_empty() {
            return None;
        }

        Some(UnusedObjWithFieldsAI { tracked_params })
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

/// Checks whether a parameter type is a Sui object with key ability, no type
/// parameters, id: UID field, and at least one additional field.
fn is_qualifying_obj(
    info: &TypingProgramInfo,
    pre_compiled: Option<&PreCompiledProgramInfo>,
    st: &SingleType,
) -> bool {
    let bt = match &st.value {
        // Only check reference parameters — by-value consumption is intentional.
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
    // id: UID (guaranteed by Sui type checker for key objects) plus at least one other field.
    fields.len() >= 2
}

impl SimpleAbsInt for UnusedObjWithFieldsAI {
    type State = State;
    type ExecutionContext = ExecutionContext;

    fn finish(
        &mut self,
        final_states: BTreeMap<Label, BlockStates<State>>,
        mut diags: Diagnostics,
    ) -> Diagnostics {
        // Use all reachable post-states, not just exits, so uses in abort
        // branches and loop bodies are not lost.
        let mut used: BTreeSet<Var> = BTreeSet::new();
        for block in final_states.values() {
            if let Some(post) = &block.post {
                used.extend(post.used.iter().copied());
            }
        }
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

    fn start_command(&self, _label: Label, _idx: usize, _: &mut State) -> ExecutionContext {
        ExecutionContext {
            diags: Diagnostics::new(),
        }
    }

    fn finish_command(
        &self,
        _label: Label,
        _idx: usize,
        context: ExecutionContext,
        _state: &mut State,
    ) -> Diagnostics {
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
                state.mark_used(&lhs_vals);
                true
            }
            // Returning a field-derived value escapes the function. A bare
            // root reference is just a pass-through and is not on its own a
            // use of any field.
            C::Return { exp, .. } => {
                let vals = self.exp(context, state, exp);
                state.mark_used_fields(&vals);
                true
            }
            // Inspecting a tracked value to drive control flow counts.
            C::JumpIf { cond: e, .. } | C::VariantSwitch { subject: e, .. } => {
                let vals = self.exp(context, state, e);
                state.mark_used(&vals);
                true
            }
            // Aborting on a tracked value inspects it for the abort code.
            C::Abort(_, e) => {
                let vals = self.exp(context, state, e);
                state.mark_used(&vals);
                true
            }
            C::Assign(_, _, _) => false,
            C::IgnoreAndPop { .. } => false,
            C::Jump { .. } | C::Break(_) | C::Continue(_) => false,
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
            E::BorrowLocal(_, var) => match state.locals().get(var) {
                Some(LocalState::Available(_, value)) => Some(vec![value.clone()]),
                _ => None,
            },
            // Field borrow: result is a field-derived view of the same roots.
            // Borrow itself is not a use.
            E::Borrow(_, inner, _, _) => {
                let vals = self.exp(context, state, inner);
                Some(
                    vals.into_iter()
                        .map(|v| match v {
                            Value::Tracked(map) => Value::Tracked(
                                map.into_keys().map(|k| (k, Kind::FieldDerived)).collect(),
                            ),
                            Value::Other => Value::Other,
                        })
                        .collect(),
                )
            }
            // `freeze(&mut T) -> &T` is a type-level coercion; the consumer
            // of the frozen reference decides whether it counts as a use.
            E::Freeze(inner) => Some(self.exp(context, state, inner)),
            // Reading the value (`*x`, `!x`, `x as U`) consumes the field.
            // The root of a tracked param can't appear directly under any
            // of these — they only operate on primitives or references to
            // primitives — so any tracking we see here is field-derived.
            E::Dereference(inner) | E::UnaryExp(_, inner) | E::Cast(inner, _) => {
                let vals = self.exp(context, state, inner);
                state.mark_used(&vals);
                Some(vec![Value::Other])
            }
            // Binop operands are consumed — references reach binop only
            // through `==` / `!=`, and primitive operands reach it through
            // already-marked dereferences. Either way the binop result
            // carries no tracking forward.
            E::BinopExp(e1, _, e2) => {
                let v1 = self.exp(context, state, e1);
                let v2 = self.exp(context, state, e2);
                state.mark_used(&v1);
                state.mark_used(&v2);
                Some(vec![Value::Other])
            }
            _ => None,
        }
    }

    fn call_custom(
        &self,
        _context: &mut ExecutionContext,
        state: &mut State,
        _loc: &Loc,
        _return_ty: &Type,
        _f: &ModuleCall,
        args: Vec<Value>,
    ) -> Option<Vec<Value>> {
        // A tracked value flowing into a function call has escaped.
        state.mark_used(&args);
        None
    }
}

impl State {
    /// Records every root referenced by `values` as used.
    fn mark_used(&mut self, values: &[Value]) {
        for v in values {
            if let Value::Tracked(map) = v {
                self.used.extend(map.keys().copied());
            }
        }
    }

    /// Like [`Self::mark_used`], but only marks roots that flow as
    /// field-derived values. A bare root reference ([`Kind::Bare`]) is a
    /// pass-through and does not count on its own.
    fn mark_used_fields(&mut self, values: &[Value]) {
        for v in values {
            if let Value::Tracked(map) = v {
                for (k, kind) in map {
                    if *kind == Kind::FieldDerived {
                        self.used.insert(*k);
                    }
                }
            }
        }
    }
}

impl SimpleDomain for State {
    type Value = Value;

    fn new(_context: &CFGContext, locals: BTreeMap<Var, LocalState<Value>>) -> Self {
        State {
            locals,
            used: BTreeSet::new(),
        }
    }

    fn locals_mut(&mut self) -> &mut BTreeMap<Var, LocalState<Value>> {
        &mut self.locals
    }

    fn locals(&self) -> &BTreeMap<Var, LocalState<Value>> {
        &self.locals
    }

    fn join_value(v1: &Value, v2: &Value) -> Value {
        match (v1, v2) {
            (Value::Other, v) | (v, Value::Other) => v.clone(),
            (Value::Tracked(m1), Value::Tracked(m2)) => {
                let mut merged = m1.clone();
                for (k, &kind) in m2 {
                    merged
                        .entry(*k)
                        .and_modify(|e| *e = e.join(kind))
                        .or_insert(kind);
                }
                Value::Tracked(merged)
            }
        }
    }

    /// Keep each post-state self-contained: every use reaching the end of
    /// the block appears in its `used` set.
    fn join_impl(&mut self, other: &Self, result: &mut JoinResult) {
        for v in &other.used {
            if self.used.insert(*v) {
                *result = JoinResult::Changed;
            }
        }
    }
}

impl SimpleExecutionContext for ExecutionContext {
    fn add_diag(&mut self, diag: Diagnostic) {
        self.diags.add(diag)
    }
}
