// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fmt::Debug};

use crate::{
    cfgir::{
        absint::{AbstractInterpreter, JoinResult, TransferFunctions},
        cfg::BlockCFG,
        CFGContext,
    },
    diagnostics::{Diagnostic, Diagnostics},
    hlir::ast::{
        Command, Command_, Exp, ExpListItem, LValue, LValue_, Label, ModuleCall, Type, Type_,
        UnannotatedExp_, Var,
    },
};
use move_ir_types::location::*;

use super::absint::AbstractDomain;

pub trait AbsIntVisitor: AbstractInterpreter {
    fn init_state(context: &CFGContext) -> <Self as TransferFunctions>::State;
    fn new(context: &CFGContext, init_state: &mut <Self as TransferFunctions>::State) -> Self;
    fn finish(
        &mut self,
        final_states: BTreeMap<Label, <Self as TransferFunctions>::State>,
        diags: Diagnostics,
    ) -> Diagnostics;
}

pub type AbsIntVisitorFn = Box<dyn FnMut(&CFGContext, &BlockCFG) -> Diagnostics>;

pub fn visitor<T: AbsIntVisitor>() -> AbsIntVisitorFn {
    Box::new(|context, cfg| {
        let mut init_state = T::init_state(context);
        let mut ai = T::new(context, &mut init_state);
        let (final_state, ds) = ai.analyze_function(cfg, init_state);
        ai.finish(final_state, ds)
    })
}

//**************************************************************************************************
// simple visitor
//**************************************************************************************************

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SimpleValue<V: Clone + Debug> {
    Default,
    Specified(V),
}

pub type SimpleValues<V> = Vec<SimpleValue<V>>;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum UnavailableReason {
    Unassigned,
    Moved,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LocalState<V: Clone + Debug> {
    Unavailable(Loc, UnavailableReason),
    Available(Loc, SimpleValue<V>),
    MaybeUnavailable {
        available: Loc,
        unavailable: Loc,
        unavailable_reason: UnavailableReason,
    },
}

pub trait SimpleExecutionContext {
    type State;
    fn add_diag(&mut self, diag: Diagnostic);
    fn state(&mut self) -> &mut Self::State;
}

pub trait SimpleDomain: AbstractDomain {
    type Value: Clone + Debug;
    fn new(context: &CFGContext, locals: BTreeMap<Var, LocalState<Self::Value>>) -> Self;
    fn locals_mut(&mut self) -> &mut BTreeMap<Var, LocalState<Self::Value>>;
    fn locals(&self) -> &BTreeMap<Var, LocalState<Self::Value>>;
    fn join_value(
        result: &mut JoinResult,
        v1: &SimpleValue<Self::Value>,
        v2: &SimpleValue<Self::Value>,
    ) -> SimpleValue<Self::Value>;
    // join implementation called after locals have been joined
    fn join_impl(&mut self, other: &Self, result: &mut JoinResult);
    fn join(&mut self, other: &Self) -> JoinResult {
        use LocalState as L;
        let self_locals = self.locals();
        let other_locals = other.locals();
        assert!(
            self_locals.keys().all(|v| other_locals.contains_key(v)),
            "ICE. Incorrectly implemented abstract interpreter. \
            Local variables should not be removed from the map"
        );
        assert!(
            other_locals.keys().all(|v| self_locals.contains_key(v)),
            "ICE. Incorrectly implemented abstract interpreter. \
            Local variables should not be removed from the map"
        );
        let mut result = JoinResult::Unchanged;
        for (local, other_state) in other_locals {
            match (self.locals().get(&local).unwrap(), other_state) {
                // both available, join the value
                (L::Available(loc, v1), L::Available(_, v2)) => {
                    let loc = *loc;
                    let joined = Self::join_value(&mut result, v1, v2);
                    self.locals_mut().insert(*local, L::Available(loc, joined));
                }
                // equal so nothing to do
                (L::Unavailable(_, _), L::Unavailable(_, _))
                | (L::MaybeUnavailable { .. }, L::MaybeUnavailable { .. }) => (),
                // if its partially assigned, stays partially assigned
                (L::MaybeUnavailable { .. }, _) => (),

                // if was partially assigned in other, its now partially assigned
                (_, L::MaybeUnavailable { .. }) => {
                    result = JoinResult::Changed;
                    self.locals_mut().insert(*local, other_state.clone());
                }

                // Available in one but not the other, so maybe unavailable
                (L::Available(available, _), L::Unavailable(unavailable, reason))
                | (L::Unavailable(unavailable, reason), L::Available(available, _)) => {
                    result = JoinResult::Changed;
                    let available = *available;
                    let unavailable = *unavailable;
                    let state = L::MaybeUnavailable {
                        available,
                        unavailable,
                        unavailable_reason: *reason,
                    };
                    self.locals_mut().insert(*local, state);
                }
            }
        }
        self.join_impl(other, &mut result);
        result
    }
}

pub trait SimpleAbsInt: AbstractInterpreter + Sized
where
    Self::State: SimpleDomain,
{
    type ExecutionContext: SimpleExecutionContext<State = Self::State>;

    fn new(context: &CFGContext, init_state: &mut <Self as TransferFunctions>::State) -> Self;

    fn finish(
        &mut self,
        final_states: BTreeMap<Label, <Self as TransferFunctions>::State>,
        diags: Diagnostics,
    ) -> Diagnostics;

    fn verify(context: &CFGContext, cfg: &BlockCFG) -> Diagnostics {
        let mut locals = context
            .locals
            .key_cloned_iter()
            .map(|(v, _)| {
                (
                    v,
                    LocalState::Unavailable(v.0.loc, UnavailableReason::Unassigned),
                )
            })
            .collect::<BTreeMap<_, _>>();
        for (param, _) in &context.signature.parameters {
            locals.insert(
                *param,
                LocalState::Available(param.0.loc, SimpleValue::Default),
            );
        }
        let mut init_state = Self::State::new(context, locals);
        let mut ai = Self::new(context, &mut init_state);
        let (final_state, ds) = ai.analyze_function(cfg, init_state);
        ai.finish(final_state, ds)
    }

    fn start_command(&self, pre: &Self::State) -> Self::ExecutionContext;

    fn finish_command(&self, context: Self::ExecutionContext) -> Diagnostics;

    fn execute(
        &mut self,
        pre: &mut Self::State,
        _lbl: Label,
        _idx: usize,
        cmd: &Command,
    ) -> Diagnostics {
        let mut context = self.start_command(pre);
        self.command(&mut context, cmd);
        self.finish_command(context)
    }

    /// custom visit for a command. It will skip `command` if `command_custom` returns true.
    fn command_custom(&self, context: &mut Self::ExecutionContext, cmd: &Command) -> bool;
    fn command(&self, context: &mut Self::ExecutionContext, cmd: &Command) {
        use Command_ as C;
        if self.command_custom(context, cmd) {
            return;
        }
        let sp!(_, cmd_) = cmd;
        match cmd_ {
            C::Assign(ls, e) => {
                let values = self.exp(context, e);
                self.lvalues(context, ls, values);
            }
            C::Mutate(el, er) => {
                self.exp(context, er);
                self.exp(context, el);
            }
            C::JumpIf { cond: e, .. }
            | C::IgnoreAndPop { exp: e, .. }
            | C::Return { exp: e, .. }
            | C::Abort(e) => {
                self.exp(context, e);
            }
            C::Jump { .. } => (),
            C::Break | C::Continue => panic!("ICE break/continue not translated to jumps"),
        }
    }

    fn lvalues(
        &self,
        context: &mut Self::ExecutionContext,
        ls: &[LValue],
        values: SimpleValues<<Self::State as SimpleDomain>::Value>,
    ) {
        // pad with defautl to account for errors
        let padded_values = values
            .into_iter()
            .chain(std::iter::repeat(SimpleValue::Default));
        for (l, value) in ls.iter().zip(padded_values) {
            self.lvalue(context, l, value)
        }
    }

    /// custom visit for an lvalue. It will skip `lvalue` if `lvalue_custom` returns true.
    fn lvalue_custom(
        &self,
        context: &mut Self::ExecutionContext,
        l: &LValue,
        value: &SimpleValue<<Self::State as SimpleDomain>::Value>,
    ) -> bool;
    fn lvalue(
        &self,
        context: &mut Self::ExecutionContext,
        l: &LValue,
        value: SimpleValue<<Self::State as SimpleDomain>::Value>,
    ) {
        use LValue_ as L;
        if self.lvalue_custom(context, l, &value) {
            return;
        }
        let sp!(loc, l_) = l;
        match l_ {
            L::Ignore => (),
            L::Var(v, _) => {
                let locals = context.state().locals_mut();
                locals.insert(*v, LocalState::Available(*loc, value));
            }
            L::Unpack(_, _, fields) => {
                for (_, l) in fields {
                    self.lvalue(context, l, SimpleValue::Default)
                }
            }
        }
    }

    /// custom visit for an exp. It will skip `exp` and `call_custom` if `exp_custom` returns Some.
    fn exp_custom(
        &self,
        context: &mut Self::ExecutionContext,
        parent_e: &Exp,
    ) -> Option<SimpleValues<<Self::State as SimpleDomain>::Value>>;
    fn call_custom(
        &self,
        context: &mut Self::ExecutionContext,
        f: &ModuleCall,
        args: SimpleValues<<Self::State as SimpleDomain>::Value>,
    ) -> Option<SimpleValues<<Self::State as SimpleDomain>::Value>>;
    fn exp(
        &self,
        context: &mut Self::ExecutionContext,
        parent_e: &Exp,
    ) -> SimpleValues<<Self::State as SimpleDomain>::Value> {
        use UnannotatedExp_ as E;
        if let Some(vs) = self.exp_custom(context, parent_e) {
            return vs;
        }
        let eloc = &parent_e.exp.loc;
        match &parent_e.exp.value {
            E::Move { var, .. } => {
                let locals = context.state().locals_mut();
                let prev = locals.insert(
                    *var,
                    LocalState::Unavailable(*eloc, UnavailableReason::Moved),
                );
                match prev {
                    Some(LocalState::Available(_, value)) => {
                        vec![value]
                    }
                    // Possible error case
                    _ => default_values(1),
                }
            }
            E::Copy { var, .. } => {
                let locals = context.state().locals_mut();
                match locals.get(var) {
                    Some(LocalState::Available(_, value)) => vec![value.clone()],
                    // Possible error case
                    _ => default_values(1),
                }
            }
            E::BorrowLocal(_, _) => default_values(1),
            E::Freeze(e)
            | E::Dereference(e)
            | E::Borrow(_, e, _)
            | E::Cast(e, _)
            | E::UnaryExp(_, e) => {
                self.exp(context, e);
                default_values(1)
            }
            E::Builtin(_, e) => {
                self.exp(context, e);
                default_values_for_ty(&parent_e.ty)
            }
            E::Vector(_, n, _, e) => {
                self.exp(context, e);
                default_values(*n)
            }
            E::ModuleCall(mcall) => {
                let evalues = self.exp(context, &mcall.arguments);
                if let Some(vs) = self.call_custom(context, &mcall, evalues) {
                    return vs;
                }

                default_values_for_ty(&parent_e.ty)
            }

            E::Unit { .. } => vec![],
            E::Value(_) | E::Constant(_) | E::Spec(_, _) | E::UnresolvedError => default_values(1),

            E::BinopExp(e1, _, e2) => {
                self.exp(context, e1);
                self.exp(context, e2);
                default_values(1)
            }
            E::Pack(_, _, fields) => {
                for (_, _, e) in fields {
                    self.exp(context, e);
                }
                default_values(1)
            }
            E::ExpList(es) => es
                .iter()
                .flat_map(|item| self.exp_list_item(context, item))
                .collect(),

            E::Unreachable => panic!("ICE should not analyze dead code"),
        }
    }

    fn exp_list_item(
        &self,
        context: &mut Self::ExecutionContext,
        item: &ExpListItem,
    ) -> SimpleValues<<Self::State as SimpleDomain>::Value> {
        match item {
            ExpListItem::Single(e, _) | ExpListItem::Splat(_, e, _) => self.exp(context, e),
        }
    }
}

pub fn default_values_for_ty<V: Clone + Debug>(ty: &Type) -> SimpleValues<V> {
    match &ty.value {
        Type_::Unit => vec![],
        Type_::Single(_) => default_values(1),
        Type_::Multiple(ts) => default_values(ts.len()),
    }
}

#[inline(always)]
pub fn default_values<V: Clone + Debug>(c: usize) -> SimpleValues<V> {
    vec![SimpleValue::Default; c]
}
