// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Flags discarded return values of calls with no `&mut` arg (Sui: ignoring `(&mut) TxContext`).
//! `IgnoreAndPop` of a `Fresh` value warns immediately; otherwise a `Bound` value alive in a
//! return block's post-state on every return path warns once per originating call.

use crate::{
    cfgir::{
        CFGContext,
        absint::{BlockStates, JoinResult},
        cfg::ImmForwardCFG,
        visitor::{
            LocalState, SimpleAbsInt, SimpleAbsIntConstructor, SimpleDomain, SimpleExecutionContext,
        },
    },
    diag,
    diagnostics::{Diagnostic, Diagnostics},
    editions::Flavor,
    hlir::{
        ast::{
            Command, Command_, Exp, LValue, LValue_, Label, ModuleCall, SingleType, SingleType_,
            Type, Type_, UnannotatedExp_, Var,
        },
        translate::{DisplayVar, display_var},
    },
    linters::CoreLintCode,
    parser::ast::Ability_,
    sui_mode::{SUI_ADDR_VALUE, TX_CONTEXT_MODULE_NAME, TX_CONTEXT_TYPE_NAME},
};
use move_ir_types::location::*;
use std::collections::BTreeMap;

pub(crate) struct UnusedReturnValue;

pub(crate) struct UnusedReturnValueAI {
    is_sui: bool,
}

/// Function unique index derived from the block label + command index within the block
pub(crate) type CommandIndex = (Label, usize);

/// Information about a tracked pure call
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CallSite {
    /// The location
    loc: Loc,
    /// In Sui mode, was an &mut TxContext present but excluded
    tx_context_exempted: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Value {
    /// Pure-call result not yet bound to a named local.
    Fresh(CallSite),
    /// Bound to one or more temporary locals; `index -> originating call loc`
    Bound(BTreeMap<CommandIndex, CallSite>),
    #[default]
    Other,
}

#[derive(Clone, Debug)]
pub(crate) struct State {
    locals: BTreeMap<Var, LocalState<Value>>,
}

pub(crate) struct ExecutionContext {
    diags: Diagnostics,
    current_command: (Label, usize),
}

impl SimpleAbsIntConstructor for UnusedReturnValue {
    type AI<'a> = UnusedReturnValueAI;

    fn new<'a>(
        context: &'a CFGContext<'a>,
        _cfg: &ImmForwardCFG,
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
        Some(UnusedReturnValueAI { is_sui })
    }
}

impl SimpleAbsInt for UnusedReturnValueAI {
    type State = State;
    type ExecutionContext = ExecutionContext;

    fn finish(
        &mut self,
        _final_states: BTreeMap<Label, BlockStates<State>>,
        diags: Diagnostics,
    ) -> Diagnostics {
        diags
    }

    fn start_command(&self, label: Label, idx: usize, _: &mut State) -> ExecutionContext {
        ExecutionContext {
            diags: Diagnostics::new(),
            current_command: (label, idx),
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
        // Report any popped, unused value. Collecting based on call sites
        let values = self.exp(context, state, exp);
        let mut sites = BTreeMap::new();
        for v in values {
            match v {
                Value::Fresh(call_site) => {
                    sites.insert(context.current_command, call_site);
                }
                Value::Bound(v_sites) => {
                    sites.extend(v_sites);
                }
                Value::Other => (),
            }
        }
        for call_site in sites.into_values() {
            context.add_diag(unused_return_value_warning(call_site));
        }
        false
    }

    fn exp_custom(
        &self,
        _context: &mut ExecutionContext,
        state: &mut State,
        e: &Exp,
    ) -> Option<Vec<Value>> {
        // Copying or borrowing the local does not free the tracked value, but it does count as a
        // use, so we stop tracking it (mark it `Other`) to avoid a spurious unused-value warning.
        let var = match &e.exp.value {
            UnannotatedExp_::Copy { var, .. } | UnannotatedExp_::BorrowLocal(_, var) => var,
            _ => return None,
        };
        if state.locals().contains_key(var) {
            state
                .locals_mut()
                .insert(*var, LocalState::Available(e.exp.loc, Value::Other));
        }
        None
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
        let purity = call_purity(self.is_sui, f);
        let mk_value = |st: &SingleType| {
            let tx_context_exempted = match purity {
                Purity::Mutable => return Value::Other,
                // Non-drop slots are forced to be used by the type system, no need to track them.
                Purity::Pure { .. } if !st.value.has_ability_(Ability_::Drop) => {
                    return Value::Other;
                }
                Purity::Pure {
                    tx_context_exempted,
                } => tx_context_exempted,
            };
            Value::Fresh(CallSite {
                loc: *loc,
                tx_context_exempted,
            })
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
        let display_var = display_var(var.0.value);
        let new_value = match value {
            // If bound to a user-named local, stop tracking. We only want to track through
            // compiler generated temporaries.
            // The compiler already generates warnings for unused locals.
            _ if matches!(display_var, DisplayVar::Orig(_)) => Value::Other,
            Value::Fresh(call_site) => {
                Value::Bound(BTreeMap::from([(context.current_command, *call_site)]))
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
            (Value::Fresh(_), v) | (v, Value::Fresh(_)) => {
                debug_assert!(
                    false,
                    "A fresh value should never reach the end of the block"
                );
                v.clone()
            }
            (Value::Other, v) | (v, Value::Other) => v.clone(),
            (Value::Bound(m1), Value::Bound(m2)) => {
                let mut m = m1.clone();
                m.extend(m2);
                Value::Bound(m)
            }
        }
    }

    fn join_impl(&mut self, _: &Self, _: &mut JoinResult) {}
}

impl SimpleExecutionContext for ExecutionContext {
    fn add_diag(&mut self, diag: Diagnostic) {
        self.diags.add(diag)
    }
}

/// Whether a call should be treated as "pure" for the purposes of this lint.
enum Purity {
    /// No `&mut` arguments
    Pure {
        /// In Sui flavor, was there an excluded `&mut TxContext`
        tx_context_exempted: bool,
    },
    /// At least one `&mut` argument
    Mutable,
}

fn call_purity(is_sui: bool, f: &ModuleCall) -> Purity {
    let mut tx_context_exempted = false;
    for arg in &f.arguments {
        let Type_::Single(sp!(_, SingleType_::Ref(true, bt))) = &arg.ty.value else {
            continue;
        };
        let is_tx_context = is_sui
            && bt
                .value
                .is_apply(
                    &SUI_ADDR_VALUE,
                    TX_CONTEXT_MODULE_NAME,
                    TX_CONTEXT_TYPE_NAME,
                )
                .is_some();
        if !is_tx_context {
            return Purity::Mutable;
        }
        tx_context_exempted = true;
    }
    Purity::Pure {
        tx_context_exempted,
    }
}

fn unused_return_value_warning(call_site: CallSite) -> Diagnostic {
    let CallSite {
        loc,
        tx_context_exempted,
    } = call_site;
    let msg = "Unused return value. This function takes no '&mut' arguments, \
               so its result is the only observable effect of the call";
    let mut d = diag!(CoreLintCode::UnusedReturnValue.diag_info(), (loc, msg));
    if tx_context_exempted {
        d.add_note(
            "'TxContext' is not considered a mutable reference input for the purposes of this lint.",
        );
    }
    d.add_note("Bind the result with 'let', or use 'let _ = ...' to discard it explicitly.");
    d
}
