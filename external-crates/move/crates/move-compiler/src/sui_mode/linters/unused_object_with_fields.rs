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
    parser::ast::{Ability_, Field},
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
    /// Union of `State::used` taken from every command's post-state. This
    /// harvests path-tracked uses without needing to walk the CFG again in
    /// `finish` (the framework only hands us pre-states there).
    used_in_post_states: RefCell<BTreeSet<Var>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Value {
    /// One or more tracked roots flow into this value directly.
    /// Invariant: at least one of `root` or `field` is true, and `roots` is non-empty.
    Tracked {
        /// Some incoming path produced the root reference unchanged.
        root: bool,
        /// Some incoming path produced a value derived from a field.
        field: bool,
        /// Tracked parameter vars whose values may flow into this value.
        roots: BTreeSet<Var>,
    },
    /// A packed struct or variant. Each entry says "field `f` of this struct
    /// carries tracking from these roots." Fields not in the map are unrelated
    /// to any tracked param.
    Packed(BTreeMap<Field, BTreeSet<Var>>),
    #[default]
    Other,
}

pub struct ExecutionContext {
    diags: Diagnostics,
}

#[derive(Clone, Debug)]
pub struct State {
    locals: BTreeMap<Var, LocalState<Value>>,
    /// Tracked parameter vars marked as used along the paths reaching this state.
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

        // Skip functions that always abort — the object is intentionally consumed
        if always_aborts(cfg) {
            return None;
        }

        let mut tracked_params = BTreeMap::new();
        for (_mutability, v, st) in &context.signature.parameters {
            if !is_qualifying_obj(context.info, context.pre_compiled_program.as_deref(), st) {
                continue;
            }
            let locals = init_state.locals_mut();
            // The framework guarantees parameters are Available at init; if
            // an unexpected state slips through, skip rather than panic so
            // the lint can't bring the whole compile down.
            let Some(LocalState::Available(_, val)) = locals.get_mut(v) else {
                debug_assert!(false, "parameter must be available at init");
                continue;
            };
            *val = Value::Tracked {
                root: true,
                field: false,
                roots: BTreeSet::from([*v]),
            };
            tracked_params.insert(*v, v.0.loc);
        }
        if tracked_params.is_empty() {
            return None;
        }

        Some(UnusedObjWithFieldsAI {
            tracked_params,
            used_in_post_states: RefCell::new(BTreeSet::new()),
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
        let used = self.used_in_post_states.borrow();
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

    fn finish_command(&self, context: ExecutionContext, state: &mut State) -> Diagnostics {
        // Capture this command's post-state contributions. Once a var is in
        // `state.used`, every successor's pre-state inherits it through the
        // join; harvesting after every command means no terminal block's
        // contribution is lost.
        self.used_in_post_states
            .borrow_mut()
            .extend(state.used.iter().copied());
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
                // RHS is evaluated for its side effects only — reading a
                // field to assign elsewhere is not on its own a "use".
                self.exp(context, state, rhs);
                let lhs_vals = self.exp(context, state, lhs);
                mark_used(state, &lhs_vals);
                true
            }
            C::Return { exp, .. } => {
                let vals = self.exp(context, state, exp);
                for v in &vals {
                    match v {
                        // Returning a field-derived value counts as use; a
                        // bare root reference is just a pass-through.
                        Value::Tracked {
                            field: true, roots, ..
                        } => state.used.extend(roots.iter().copied()),
                        // Returning a packed struct exposes its tracked
                        // fields to the caller.
                        Value::Packed(map) => {
                            for r in map.values() {
                                state.used.extend(r.iter().copied());
                            }
                        }
                        _ => {}
                    }
                }
                true
            }
            // Inspecting a tracked value to drive control flow counts.
            C::JumpIf { cond: e, .. } | C::VariantSwitch { subject: e, .. } => {
                let vals = self.exp(context, state, e);
                mark_used(state, &vals);
                true
            }
            // `IgnoreAndPop`, `Abort`, etc. are left to the default handler
            // so the value is evaluated for sub-expression side effects but
            // not counted as a use on its own.
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
            // `&local` — propagate the local's tracked value through, so
            // a subsequent field-borrow can recover roots from a packed
            // local. The framework default returns `Other` here.
            E::BorrowLocal(_, var) => match state.locals().get(var) {
                Some(LocalState::Available(_, value)) => Some(vec![value.clone()]),
                _ => None,
            },
            // Field access: tracked-as-root flips to field-derived; for a
            // packed struct we look up which roots that specific field
            // carries.
            E::Borrow(_, inner, field, _) => {
                let vals = self.exp(context, state, inner);
                Some(
                    vals.into_iter()
                        .map(|v| match v {
                            Value::Tracked { roots, .. } => Value::Tracked {
                                root: false,
                                field: true,
                                roots,
                            },
                            Value::Packed(mut map) => match map.remove(field) {
                                Some(roots) => Value::Tracked {
                                    root: false,
                                    field: true,
                                    roots,
                                },
                                None => Value::Other,
                            },
                            Value::Other => Value::Other,
                        })
                        .collect(),
                )
            }
            // Pure "view" operations — pass tracking through; consumers
            // downstream are what mark.
            E::Dereference(inner)
            | E::Freeze(inner)
            | E::UnaryExp(_, inner)
            | E::Cast(inner, _) => Some(self.exp(context, state, inner)),
            // Binop produces a fresh value derived from its operands; the
            // result still carries tracking, so a downstream consumer
            // (return, fn call, JumpIf, …) marks. `c.x + 5;` on its own
            // flags because the result is dropped.
            E::BinopExp(e1, _, e2) => {
                let mut roots = BTreeSet::new();
                for v in self.exp(context, state, e1) {
                    collect_roots(v, &mut roots);
                }
                for v in self.exp(context, state, e2) {
                    collect_roots(v, &mut roots);
                }
                let v = if roots.is_empty() {
                    Value::Other
                } else {
                    Value::Tracked {
                        root: false,
                        field: true,
                        roots,
                    }
                };
                Some(vec![v])
            }
            // Pack/PackVariant: build a per-field tracking map so a later
            // borrow of that exact field can recover the roots it came from.
            // Borrowing an untracked field of the same struct yields `Other`.
            E::Pack(_, _, fields) | E::PackVariant(_, _, _, fields) => {
                let mut field_map: BTreeMap<Field, BTreeSet<Var>> = BTreeMap::new();
                for (f, _, e) in fields {
                    let mut roots = BTreeSet::new();
                    for v in self.exp(context, state, e) {
                        collect_roots(v, &mut roots);
                    }
                    if !roots.is_empty() {
                        field_map.insert(*f, roots);
                    }
                }
                let v = if field_map.is_empty() {
                    Value::Other
                } else {
                    Value::Packed(field_map)
                };
                Some(vec![v])
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
        // If a tracked ref flows into a function call, mark it as used
        mark_used(state, &args);
        None
    }
}

fn mark_used(state: &mut State, values: &[Value]) {
    for v in values {
        match v {
            Value::Tracked { roots, .. } => state.used.extend(roots.iter().copied()),
            Value::Packed(map) => {
                for r in map.values() {
                    state.used.extend(r.iter().copied());
                }
            }
            Value::Other => {}
        }
    }
}

/// Drains the tracked roots out of a value, regardless of whether it's
/// `Tracked` or `Packed`. Used in places that don't care about the field-level
/// distinction (e.g. flattening for joins, BinopExp).
fn collect_roots(v: Value, into: &mut BTreeSet<Var>) {
    match v {
        Value::Tracked { roots, .. } => into.extend(roots),
        Value::Packed(map) => {
            for (_, r) in map {
                into.extend(r);
            }
        }
        Value::Other => {}
    }
}

/// Collapses `Packed(...)` into a flat `Tracked { field: true, roots: union }`,
/// for joining with a sibling `Tracked` value.
fn flatten_packed(v: Value) -> Value {
    if let Value::Packed(map) = v {
        let mut roots = BTreeSet::new();
        for (_, r) in map {
            roots.extend(r);
        }
        if roots.is_empty() {
            Value::Other
        } else {
            Value::Tracked {
                root: false,
                field: true,
                roots,
            }
        }
    } else {
        v
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
            (
                Value::Tracked {
                    root: r1,
                    field: f1,
                    roots: rs1,
                },
                Value::Tracked {
                    root: r2,
                    field: f2,
                    roots: rs2,
                },
            ) => Value::Tracked {
                root: *r1 || *r2,
                field: *f1 || *f2,
                roots: rs1 | rs2,
            },
            (Value::Packed(m1), Value::Packed(m2)) => {
                let mut result = m1.clone();
                for (k, rs) in m2 {
                    result.entry(*k).or_default().extend(rs.iter().copied());
                }
                Value::Packed(result)
            }
            // Mixed Tracked/Packed: collapse the packed side to a flat
            // Tracked then re-join.
            (Value::Tracked { .. }, Value::Packed(_)) => {
                let flat = flatten_packed(v2.clone());
                Self::join_value(v1, &flat)
            }
            (Value::Packed(_), Value::Tracked { .. }) => {
                let flat = flatten_packed(v1.clone());
                Self::join_value(&flat, v2)
            }
        }
    }

    fn join_impl(&mut self, other: &Self, result: &mut JoinResult) {
        let before = self.used.len();
        self.used.extend(other.used.iter().copied());
        if self.used.len() != before {
            *result = JoinResult::Changed;
        }
    }
}

impl SimpleExecutionContext for ExecutionContext {
    fn add_diag(&mut self, diag: Diagnostic) {
        self.diags.add(diag)
    }
}
