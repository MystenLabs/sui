// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Flags discarded return values of calls with no `&mut` arg (Sui: ignoring `(&mut) TxContext`).
//! `IgnoreAndPop` of a `Fresh` value warns immediately; otherwise a `Bound` value alive in a
//! return block's post-state on every return path warns once per originating call.

use crate::{
    cfgir::{
        CFGContext,
        absint::{BlockStates, JoinResult},
        cfg::{CFG, ImmForwardCFG},
        visitor::{
            LocalState, SimpleAbsInt, SimpleAbsIntConstructor, SimpleDomain, SimpleExecutionContext,
        },
    },
    diag,
    diagnostics::{Diagnostic, Diagnostics},
    editions::Flavor,
    hlir::ast::{
        BaseType_, Command, Command_, LValue, LValue_, Label, ModuleCall, SingleType, SingleType_,
        Type, Type_, Var,
    },
    linters::StyleCodes,
    parser::ast::Ability_,
    sui_mode::{SUI_ADDR_VALUE, TX_CONTEXT_MODULE_NAME, TX_CONTEXT_TYPE_NAME},
};
use move_ir_types::location::*;
use move_symbol_pool::Symbol;
use std::collections::{BTreeMap, BTreeSet};

pub struct UnusedReturnValue;

pub struct UnusedReturnValueAI {
    is_sui: bool,
    return_blocks: BTreeSet<Label>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ValueId {
    block: Label,
    cmd_idx: usize,
    var: Symbol,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Value {
    /// Pure-call result not yet bound to a named local.
    Fresh(Loc),
    /// Bound to one or more named locals; `id -> originating call loc`. Joins union the maps.
    Bound(BTreeMap<ValueId, Loc>),
    #[default]
    Other,
}

#[derive(Clone, Debug)]
pub struct State {
    locals: BTreeMap<Var, LocalState<Value>>,
}

pub struct ExecutionContext {
    diags: Diagnostics,
    location: (Label, usize),
}

impl SimpleAbsIntConstructor for UnusedReturnValue {
    type AI<'a> = UnusedReturnValueAI;

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
        let is_sui = context.env.package_config(context.package).flavor == Flavor::Sui;
        let return_blocks = cfg
            .block_labels()
            .filter(|lbl| {
                matches!(
                    cfg.commands(*lbl).last(),
                    Some((_, sp!(_, Command_::Return { .. })))
                )
            })
            .collect();
        Some(UnusedReturnValueAI {
            is_sui,
            return_blocks,
        })
    }
}

impl SimpleAbsInt for UnusedReturnValueAI {
    type State = State;
    type ExecutionContext = ExecutionContext;

    fn finish(
        &mut self,
        final_states: BTreeMap<Label, BlockStates<State>>,
        mut diags: Diagnostics,
    ) -> Diagnostics {
        if self.return_blocks.is_empty() {
            return diags;
        }
        let per_return: Vec<BTreeMap<ValueId, Loc>> = self
            .return_blocks
            .iter()
            .filter_map(|lbl| {
                let post = final_states.get(lbl)?.post.as_ref()?;
                let mut m = BTreeMap::new();
                for ls in post.locals.values() {
                    if let LocalState::Available(_, v) = ls {
                        let ids = match v {
                            Value::Bound(ids) => ids,
                            Value::Fresh(_) => {
                                debug_assert!(false, "should never store a fresh value in a local");
                                continue;
                            }
                            Value::Other => continue,
                        };
                        m.extend(ids);
                    }
                }
                if m.is_empty() { None } else { Some(m) }
            })
            .collect();
        if per_return.is_empty() {
            return diags;
        }
        // intersect the unused from each return block
        let mut iter = per_return.into_iter();
        let mut unused = iter.next().unwrap();
        for m in iter {
            unused.retain(|id, _| m.contains_key(id));
        }

        // report an error for each unused value
        for (_id, loc) in unused {
            diags.add(unused_return_value_warning(loc));
        }
        diags
    }

    fn start_command(&self, label: Label, idx: usize, _: &mut State) -> ExecutionContext {
        ExecutionContext {
            diags: Diagnostics::new(),
            location: (label, idx),
        }
    }

    fn finish_command(
        &self,
        _label: Label,
        _idx: usize,
        context: ExecutionContext,
        _state: &mut State,
    ) -> Diagnostics {
        let ExecutionContext { diags, .. } = context;
        diags
    }

    fn command_custom(
        &self,
        context: &mut ExecutionContext,
        state: &mut State,
        cmd: &Command,
    ) -> bool {
        let Command_::IgnoreAndPop { exp, .. } = &cmd.value else {
            return false;
        };
        let values = self.exp(context, state, exp);
        for v in values {
            if let Value::Fresh(call_loc) = v {
                context.add_diag(unused_return_value_warning(call_loc));
            }
        }
        true
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
        let is_pure = call_is_pure(self.is_sui, f);
        // Non-drop slots are forced to be used by the type system, no need to track them.
        let mk_value = |st: &SingleType| {
            if is_pure && st.value.has_ability_(Ability_::Drop) {
                Value::Fresh(*loc)
            } else {
                Value::Other
            }
        };
        Some(match &return_ty.value {
            Type_::Unit => vec![],
            Type_::Single(st) => vec![mk_value(st)],
            Type_::Multiple(sts) => sts.iter().map(mk_value).collect(),
        })
    }

    fn lvalue_custom(
        &self,
        context: &mut ExecutionContext,
        state: &mut State,
        l: &LValue,
        value: &Value,
    ) -> bool {
        let sp!(loc, l_) = l;
        let LValue_::Var { var, .. } = l_ else {
            return false;
        };
        let new_value = match value {
            // Leading-underscore locals are conventionally intentionally/potentially unused
            Value::Fresh(_) | Value::Bound(_) if var.starts_with_underscore() => Value::Other,
            Value::Fresh(call_loc) => {
                let (block, cmd_idx) = context.location;
                let id = ValueId {
                    block,
                    cmd_idx,
                    var: var.0.value,
                };
                Value::Bound(BTreeMap::from([(id, *call_loc)]))
            }
            Value::Bound(_) | Value::Other => value.clone(),
        };
        state
            .locals_mut()
            .insert(*var, LocalState::Available(*loc, new_value));
        true
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
            (Value::Bound(m1), Value::Bound(m2)) => {
                let mut m = m1.clone();
                for (id, loc) in m2 {
                    m.insert(*id, *loc);
                }
                Value::Bound(m)
            }
            // One side lost tracking - drop. `MaybeUnavailable` from the framework's
            // `LocalState` join still hides consumed-on-one-branch values from `finish`.
            _ => Value::Other,
        }
    }

    fn join_impl(&mut self, _: &Self, _: &mut JoinResult) {}
}

impl SimpleExecutionContext for ExecutionContext {
    fn add_diag(&mut self, diag: Diagnostic) {
        self.diags.add(diag)
    }
}

/// No `&mut` arg (Sui: ignoring `(&mut) TxContext`).
fn call_is_pure(is_sui: bool, f: &ModuleCall) -> bool {
    !f.arguments
        .iter()
        .any(|arg| is_mutating_ref_arg(is_sui, &arg.ty))
}

fn is_mutating_ref_arg(is_sui: bool, ty: &Type) -> bool {
    let Type_::Single(sp!(_, st_)) = &ty.value else {
        return false;
    };
    let SingleType_::Ref(true, bt) = st_ else {
        return false;
    };
    if is_sui
        && let BaseType_::Apply(_, sp!(_, tn), _) = &bt.value
        && tn.is(
            &SUI_ADDR_VALUE,
            TX_CONTEXT_MODULE_NAME,
            TX_CONTEXT_TYPE_NAME,
        )
    {
        return false;
    }
    true
}

fn unused_return_value_warning(call_loc: Loc) -> Diagnostic {
    let msg = "Unused return value. This function takes no '&mut' arguments, \
               so its result is the only observable effect of the call";
    let mut d = diag!(StyleCodes::UnusedReturnValue.diag_info(), (call_loc, msg));
    d.add_note("Bind the result with 'let', or use 'let _ = ...' to discard it explicitly.");
    d
}
