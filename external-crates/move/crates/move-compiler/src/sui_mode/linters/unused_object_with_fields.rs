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
//! - the value is the field-derived result of `return`;
//! - the value appears on either side of `mutate` when the LHS itself
//!   resolves to a tracked target — i.e. the write is into a `&mut` field
//!   of an input parameter, so the RHS escapes via that input;
//! - the value is the condition of a branch or variant switch.
//!
//! Pure pass-throughs that don't on their own count: `borrow`, `freeze`,
//! `pack` (carries tracking forward), `binop` (carries tracking forward),
//! `let _ = e` for an `e` that was never dereferenced, `e;`
//! (ignore-and-pop), `abort e` for an unread `e`. In particular `borrow`
//! is needed to access a field but is not on its own a use; the use is
//! whatever the borrow flows into.
//!
//! ## How the analysis tracks "unused"
//!
//! The state carries a set `unused` of tracked params that have not yet been
//! observed as used along any path reaching the program point. The set
//! starts as every tracked param at function entry and shrinks monotonically:
//! every "use" event removes the relevant roots, and joins at block
//! boundaries take the intersection of incoming sets (a param is still
//! unused at a join only if it was unused on every incoming path). After the
//! fixed point, intersecting `unused` over every block's post-state yields
//! the params that remain unused on every reachable terminal path; those are
//! the ones we flag.

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

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Value {
    /// Carries data of one or more tracked roots.
    /// `is_field` is true if some incoming path produced a field-derived
    /// value (i.e. went through a `Borrow`); false means the value is a
    /// bare unchanged reference to the param itself.
    Tracked {
        is_field: bool,
        roots: BTreeSet<Var>,
    },
    #[default]
    Other,
}

pub struct ExecutionContext {
    diags: Diagnostics,
}

#[derive(Clone, Debug)]
pub struct State {
    locals: BTreeMap<Var, LocalState<Value>>,
    /// Tracked params still possibly unused on the path reaching this point.
    /// Shrinks as uses are observed; never grows.
    unused: BTreeSet<Var>,
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
            *val = Value::Tracked {
                is_field: false,
                roots: BTreeSet::from([*v]),
            };
            tracked_params.insert(*v, v.0.loc);
        }
        if tracked_params.is_empty() {
            return None;
        }
        init_state.unused = tracked_params.keys().copied().collect();

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

/// Checks whether a parameter type is a Sui object with key ability, no type parameters,
/// id: UID field, and at least one additional field.
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
        _final_states: BTreeMap<Label, State>,
        final_post_states: BTreeMap<Label, State>,
        mut diags: Diagnostics,
    ) -> Diagnostics {
        // After the fixed point, a param is unused iff it remains in
        // `unused` at every block's post-state. Because the set shrinks
        // monotonically along every path, this is equivalent to "unused at
        // every reachable terminal block". Default-start from every tracked
        // param so a body with no commands still flags everything.
        let mut unused: BTreeSet<Var> = self.tracked_params.keys().copied().collect();
        for state in final_post_states.values() {
            unused.retain(|v| state.unused.contains(v));
        }
        for (var, loc) in &self.tracked_params {
            if unused.contains(var) {
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
                let rhs_vals = self.exp(context, state, rhs);
                let lhs_vals = self.exp(context, state, lhs);
                mark_used(state, &lhs_vals);
                // The RHS counts only when the write target is itself a
                // tracked field — i.e. a `&mut` field of an input — so the
                // RHS value escapes via that input.
                if lhs_vals.iter().any(|v| matches!(v, Value::Tracked { .. })) {
                    mark_used(state, &rhs_vals);
                }
                true
            }
            // Returning a field-derived value escapes the function. A bare
            // root reference is just a pass-through and is not on its own a
            // use of any field.
            C::Return { exp, .. } => {
                let vals = self.exp(context, state, exp);
                mark_used_fields(state, &vals);
                true
            }
            // Inspecting a tracked value to drive control flow counts.
            C::JumpIf { cond: e, .. } | C::VariantSwitch { subject: e, .. } => {
                let vals = self.exp(context, state, e);
                mark_used(state, &vals);
                true
            }
            // `IgnoreAndPop` (`let _ = e;`, `e;`) and `Abort` evaluate the
            // sub-expression for its side effects but the value itself does
            // not flow out of the function — fall through.
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
            // `&local` — propagate the local's tracked value.
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
                            Value::Tracked { roots, .. } => Value::Tracked {
                                is_field: true,
                                roots,
                            },
                            Value::Other => Value::Other,
                        })
                        .collect(),
                )
            }
            // `freeze(&mut T) -> &T` is a pure type-level coercion — pass
            // tracking through.
            E::Freeze(inner) => Some(self.exp(context, state, inner)),
            // Reading the value (`*x`, `!x`, `x as U`) consumes the field.
            // The root of a tracked param can't appear directly under any
            // of these — they only operate on primitives or references to
            // primitives — so any tracking we see here is field-derived.
            // Mark and produce an untracked result.
            E::Dereference(inner) | E::UnaryExp(_, inner) | E::Cast(inner, _) => {
                let vals = self.exp(context, state, inner);
                mark_used(state, &vals);
                Some(vec![Value::Other])
            }
            // Binop produces a fresh field-derived value carrying the union
            // of operand roots; downstream consumers mark.
            E::BinopExp(e1, _, e2) => {
                let mut roots = BTreeSet::new();
                for v in self.exp(context, state, e1) {
                    collect_roots(v, &mut roots);
                }
                for v in self.exp(context, state, e2) {
                    collect_roots(v, &mut roots);
                }
                Some(vec![tracked_field_or_other(roots)])
            }
            // Pack collapses every field's tracking into a single Tracked
            // value. We don't bother distinguishing which packed field came
            // from which root — any later use of the packed struct treats
            // every contributing root as used.
            E::Pack(_, _, fields) | E::PackVariant(_, _, _, fields) => {
                let mut roots = BTreeSet::new();
                for (_, _, fe) in fields {
                    for v in self.exp(context, state, fe) {
                        collect_roots(v, &mut roots);
                    }
                }
                Some(vec![tracked_field_or_other(roots)])
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
        mark_used(state, &args);
        None
    }
}

//**************************************************************************************************
// helpers
//**************************************************************************************************

/// Removes any roots referenced by `values` from `state.unused`.
fn mark_used(state: &mut State, values: &[Value]) {
    for v in values {
        if let Value::Tracked { roots, .. } = v {
            for r in roots {
                state.unused.remove(r);
            }
        }
    }
}

/// Like [`mark_used`], but only marks roots that flow as field-derived
/// values. A bare root reference (`is_field: false`) is a pass-through and
/// does not count on its own.
fn mark_used_fields(state: &mut State, values: &[Value]) {
    for v in values {
        if let Value::Tracked {
            is_field: true,
            roots,
        } = v
        {
            for r in roots {
                state.unused.remove(r);
            }
        }
    }
}

fn collect_roots(v: Value, into: &mut BTreeSet<Var>) {
    if let Value::Tracked { roots, .. } = v {
        into.extend(roots);
    }
}

fn tracked_field_or_other(roots: BTreeSet<Var>) -> Value {
    if roots.is_empty() {
        Value::Other
    } else {
        Value::Tracked {
            is_field: true,
            roots,
        }
    }
}

impl SimpleDomain for State {
    type Value = Value;

    fn new(_context: &CFGContext, locals: BTreeMap<Var, LocalState<Value>>) -> Self {
        State {
            locals,
            unused: BTreeSet::new(),
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
            (
                Value::Tracked {
                    is_field: f1,
                    roots: rs1,
                },
                Value::Tracked {
                    is_field: f2,
                    roots: rs2,
                },
            ) => Value::Tracked {
                is_field: *f1 || *f2,
                roots: rs1 | rs2,
            },
        }
    }

    fn join_impl(&mut self, other: &Self, result: &mut JoinResult) {
        // Intersect: a param is still unused at the join only if it was
        // unused on every incoming path. The set never grows — if it did,
        // the procedure would not converge to a sound result.
        let before = self.unused.len();
        self.unused.retain(|v| other.unused.contains(v));
        if self.unused.len() != before {
            *result = JoinResult::Changed;
        }
    }
}

impl SimpleExecutionContext for ExecutionContext {
    fn add_diag(&mut self, diag: Diagnostic) {
        self.diags.add(diag)
    }
}
