// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fmt::Debug};

use crate::{
    cfgir::{
        absint::{AbstractDomain, AbstractInterpreter, JoinResult, TransferFunctions},
        ast as G,
        cfg::ImmForwardCFG,
        CFGContext,
    },
    command_line::compiler::Visitor,
    diagnostics::{Diagnostic, Diagnostics, WarningFilters},
    expansion::ast::ModuleIdent,
    hlir::ast::{self as H, Command, Exp, LValue, LValue_, Label, ModuleCall, Type, Type_, Var},
    parser::ast::{ConstantName, DatatypeName, FunctionName},
    shared::CompilationEnv,
};
use move_ir_types::location::*;
use move_proc_macros::growing_stack;

pub type AbsIntVisitorObj = Box<dyn AbstractInterpreterVisitor>;
pub type CFGIRVisitorObj = Box<dyn CFGIRVisitor>;

pub trait CFGIRVisitor {
    fn visit(&mut self, env: &mut CompilationEnv, program: &mut G::Program);

    fn visitor(self) -> Visitor
    where
        Self: 'static + Sized,
    {
        Visitor::CFGIRVisitor(Box::new(self))
    }
}

pub trait AbstractInterpreterVisitor {
    fn verify(
        &mut self,
        env: &CompilationEnv,
        context: &CFGContext,
        cfg: &ImmForwardCFG,
    ) -> Diagnostics;

    fn visitor(self) -> Visitor
    where
        Self: 'static + Sized,
    {
        Visitor::AbsIntVisitor(Box::new(self))
    }
}

//**************************************************************************************************
// simple ast visitor
//**************************************************************************************************

pub trait CFGIRVisitorConstructor {
    type Context<'a>: Sized + CFGIRVisitorContext;

    fn context<'a>(env: &'a mut CompilationEnv, program: &G::Program) -> Self::Context<'a>;

    fn visit(&mut self, env: &mut CompilationEnv, program: &mut G::Program) {
        let mut context = Self::context(env, program);
        context.visit(program);
    }
}

pub trait CFGIRVisitorContext {
    fn add_warning_filter_scope(&mut self, filter: WarningFilters);
    fn pop_warning_filter_scope(&mut self);

    fn visit_module_custom(
        &mut self,
        _ident: ModuleIdent,
        _mdef: &mut G::ModuleDefinition,
    ) -> bool {
        false
    }

    /// By default, the visitor will visit all all expressions in all functions in all modules. A
    /// custom version should of this function should be created if different type of analysis is
    /// required.
    fn visit(&mut self, program: &mut G::Program) {
        for (mident, mdef) in program.modules.key_cloned_iter_mut() {
            self.add_warning_filter_scope(mdef.warning_filter.clone());
            if self.visit_module_custom(mident, mdef) {
                self.pop_warning_filter_scope();
                continue;
            }

            for (struct_name, sdef) in mdef.structs.key_cloned_iter_mut() {
                self.visit_struct(mident, struct_name, sdef)
            }
            for (enum_name, edef) in mdef.enums.key_cloned_iter_mut() {
                self.visit_enum(mident, enum_name, edef)
            }
            for (constant_name, cdef) in mdef.constants.key_cloned_iter_mut() {
                self.visit_constant(mident, constant_name, cdef)
            }
            for (function_name, fdef) in mdef.functions.key_cloned_iter_mut() {
                self.visit_function(mident, function_name, fdef)
            }

            self.pop_warning_filter_scope();
        }
    }

    // TODO  type visiting

    fn visit_struct_custom(
        &mut self,
        _module: ModuleIdent,
        _struct_name: DatatypeName,
        _sdef: &mut H::StructDefinition,
    ) -> bool {
        false
    }
    fn visit_struct(
        &mut self,
        module: ModuleIdent,
        struct_name: DatatypeName,
        sdef: &mut H::StructDefinition,
    ) {
        self.add_warning_filter_scope(sdef.warning_filter.clone());
        if self.visit_struct_custom(module, struct_name, sdef) {
            self.pop_warning_filter_scope();
            return;
        }
        self.pop_warning_filter_scope();
    }

    fn visit_enum_custom(
        &mut self,
        _module: ModuleIdent,
        _enum_name: DatatypeName,
        _edef: &mut H::EnumDefinition,
    ) -> bool {
        false
    }
    fn visit_enum(
        &mut self,
        module: ModuleIdent,
        enum_name: DatatypeName,
        edef: &mut H::EnumDefinition,
    ) {
        self.add_warning_filter_scope(edef.warning_filter.clone());
        if self.visit_enum_custom(module, enum_name, edef) {
            self.pop_warning_filter_scope();
            return;
        }
        self.pop_warning_filter_scope();
    }

    fn visit_constant_custom(
        &mut self,
        _module: ModuleIdent,
        _constant_name: ConstantName,
        _cdef: &mut G::Constant,
    ) -> bool {
        false
    }
    fn visit_constant(
        &mut self,
        module: ModuleIdent,
        constant_name: ConstantName,
        cdef: &mut G::Constant,
    ) {
        self.add_warning_filter_scope(cdef.warning_filter.clone());
        if self.visit_constant_custom(module, constant_name, cdef) {
            self.pop_warning_filter_scope();
            return;
        }
        self.pop_warning_filter_scope();
    }

    fn visit_function_custom(
        &mut self,
        _module: ModuleIdent,
        _function_name: FunctionName,
        _fdef: &mut G::Function,
    ) -> bool {
        false
    }
    fn visit_function(
        &mut self,
        module: ModuleIdent,
        function_name: FunctionName,
        fdef: &mut G::Function,
    ) {
        self.add_warning_filter_scope(fdef.warning_filter.clone());
        if self.visit_function_custom(module, function_name, fdef) {
            self.pop_warning_filter_scope();
            return;
        }
        if let G::FunctionBody_::Defined {
            locals: _,
            start: _,
            block_info: _,
            blocks,
        } = &mut fdef.body.value
        {
            for (lbl, block) in blocks {
                self.visit_block(*lbl, block);
            }
        }
        self.pop_warning_filter_scope();
    }

    fn visit_block_custom(&mut self, _lbl: Label, _block: &mut G::BasicBlock) -> bool {
        false
    }
    fn visit_block(&mut self, _lbl: Label, block: &mut G::BasicBlock) {
        for cmd in block {
            self.visit_command(cmd)
        }
    }

    fn visit_command_custom(&mut self, _cmd: &mut H::Command) -> bool {
        false
    }
    fn visit_command(&mut self, cmd: &mut H::Command) {
        use H::Command_ as C;
        if self.visit_command_custom(cmd) {
            return;
        }
        match &mut cmd.value {
            C::Assign(_, _, e)
            | C::Abort(_, e)
            | C::Return { exp: e, .. }
            | C::IgnoreAndPop { exp: e, .. }
            | C::JumpIf { cond: e, .. }
            | C::VariantSwitch { subject: e, .. } => {
                self.visit_exp(e);
            }
            C::Mutate(el, er) => {
                self.visit_exp(el);
                self.visit_exp(er);
            }
            C::Jump { .. } => (),
            C::Break(_) | C::Continue(_) => panic!("ICE break/continue not translated to jumps"),
        }
    }

    fn visit_exp_custom(&mut self, _e: &mut H::Exp) -> bool {
        false
    }
    #[growing_stack]
    fn visit_exp(&mut self, e: &mut H::Exp) {
        use H::UnannotatedExp_ as E;
        if self.visit_exp_custom(e) {
            return;
        }
        match &mut e.exp.value {
            E::Unit { .. }
            | E::Move { .. }
            | E::Copy { .. }
            | E::Constant(_)
            | E::ErrorConstant { .. }
            | E::BorrowLocal(_, _)
            | E::Unreachable
            | E::UnresolvedError => (),

            E::Value(v) => self.visit_value(v),

            E::Freeze(e)
            | E::Dereference(e)
            | E::UnaryExp(_, e)
            | E::Borrow(_, e, _, _)
            | E::Cast(e, _) => self.visit_exp(e),

            E::BinopExp(el, _, er) => {
                self.visit_exp(el);
                self.visit_exp(er);
            }

            E::ModuleCall(m) => {
                for arg in &mut m.arguments {
                    self.visit_exp(arg)
                }
            }
            E::Vector(_, _, _, es) | E::Multiple(es) => {
                for e in es {
                    self.visit_exp(e)
                }
            }

            E::Pack(_, _, es) | E::PackVariant(_, _, _, es) => {
                for (_, _, e) in es {
                    self.visit_exp(e)
                }
            }
        }
    }

    fn visit_value_custom(&mut self, _v: &mut H::Value) -> bool {
        false
    }
    #[growing_stack]
    fn visit_value(&mut self, v: &mut H::Value) {
        use H::Value_ as V;
        if self.visit_value_custom(v) {
            return;
        }
        match &mut v.value {
            V::Address(_)
            | V::U8(_)
            | V::U16(_)
            | V::U32(_)
            | V::U64(_)
            | V::U128(_)
            | V::U256(_)
            | V::Bool(_) => (),
            V::Vector(_, vs) => {
                for v in vs {
                    self.visit_value(v)
                }
            }
        }
    }
}

impl<V: CFGIRVisitor + 'static> From<V> for CFGIRVisitorObj {
    fn from(value: V) -> Self {
        Box::new(value)
    }
}

impl<V: CFGIRVisitorConstructor> CFGIRVisitor for V {
    fn visit(&mut self, env: &mut CompilationEnv, program: &mut G::Program) {
        self.visit(env, program)
    }
}

//**************************************************************************************************
// simple absint visitor
//**************************************************************************************************

/// The reason why a local variable is unavailable (mostly useful for error messages)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnavailableReason {
    Unassigned,
    Moved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// The state of a local variable, with its abstract value if it has one.
pub enum LocalState<V: Clone + Debug + Default> {
    Unavailable(Loc, UnavailableReason),
    Available(Loc, V),
    MaybeUnavailable {
        available: Loc,
        unavailable: Loc,
        unavailable_reason: UnavailableReason,
    },
}

/// A trait for a the context when visiting a `Command` in a block. At a minimum it must hold the diagnostics
/// and the abstract state
pub trait SimpleExecutionContext {
    /// Add a diagnostic
    fn add_diag(&mut self, diag: Diagnostic);
}

/// The domain used for the simple abstract interpreter template. Accessors for the local variables
/// must be provided, but it will manage the joining of the locals (given a way to join values).
pub trait SimpleDomain: AbstractDomain {
    /// The non-default abstract value
    type Value: Clone + Debug + Default + Eq;

    /// Constructs a new domain, given all locals where unassiagned locals have
    /// `LocalState::Unavailable` and parameters have
    /// `LocalState::Available(_, SimpleValue::Default)`
    fn new(context: &CFGContext, locals: BTreeMap<Var, LocalState<Self::Value>>) -> Self;

    /// Mutable access for the states of local variables
    fn locals_mut(&mut self) -> &mut BTreeMap<Var, LocalState<Self::Value>>;

    /// Immutable access for the states of local variables
    fn locals(&self) -> &BTreeMap<Var, LocalState<Self::Value>>;

    /// Joining values. Called during joining if a local is available in both states
    fn join_value(v1: &Self::Value, v2: &Self::Value) -> Self::Value;

    /// `join_impl` is called after joining locals in `join` if any custom joining logic is needed
    fn join_impl(&mut self, other: &Self, result: &mut JoinResult);
}

impl<V: SimpleDomain> AbstractDomain for V {
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
            match (self.locals().get(local).unwrap(), other_state) {
                // both available, join the value
                (L::Available(loc, v1), L::Available(_, v2)) => {
                    let loc = *loc;
                    let joined = Self::join_value(v1, v2);
                    if v1 != &joined {
                        result = JoinResult::Changed
                    }
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

/// Trait for simple abstract interpreter passes. Custom hooks can be implemented with additional
/// logic as needed. The provided implementation will do all of the plumbing of abstract values
/// through the expressions, commands, and locals.
pub trait SimpleAbsIntConstructor: Sized {
    type AI<'a>: Sized + SimpleAbsInt;
    /// Given the initial state/domain, construct a new abstract interpreter.
    /// Return None if it should not be run given this context
    fn new<'a>(
        env: &CompilationEnv,
        context: &'a CFGContext<'a>,
        init_state: &mut <Self::AI<'a> as SimpleAbsInt>::State,
    ) -> Option<Self::AI<'a>>;

    fn verify(
        &mut self,
        env: &CompilationEnv,
        context: &CFGContext,
        cfg: &ImmForwardCFG,
    ) -> Diagnostics {
        let mut locals = context
            .locals
            .key_cloned_iter()
            .map(|(v, _)| {
                let unassigned = LocalState::Unavailable(v.0.loc, UnavailableReason::Unassigned);
                (v, unassigned)
            })
            .collect::<BTreeMap<_, _>>();
        for (_, param, _) in &context.signature.parameters {
            locals.insert(
                *param,
                LocalState::Available(
                    param.0.loc,
                    <<Self::AI<'_> as SimpleAbsInt>::State as SimpleDomain>::Value::default(),
                ),
            );
        }
        let mut init_state = <Self::AI<'_> as SimpleAbsInt>::State::new(context, locals);
        let Some(mut ai) = Self::new(env, context, &mut init_state) else {
            return Diagnostics::new();
        };
        let (final_state, ds) = ai.analyze_function(cfg, init_state);
        ai.finish(final_state, ds)
    }
}

pub trait SimpleAbsInt: Sized {
    type State: SimpleDomain;
    /// The execution context local to a command
    type ExecutionContext: SimpleExecutionContext;

    /// A hook for an additional processing after visiting all codes. The `final_states` are the
    /// pre-states for each block (keyed by the label for the block). The `diags` are collected from
    /// all code visited.
    fn finish(
        &mut self,
        final_states: BTreeMap<Label, Self::State>,
        diags: Diagnostics,
    ) -> Diagnostics;

    /// A hook for any pre-processing at the start of a command
    fn start_command(&self, pre: &mut Self::State) -> Self::ExecutionContext;

    /// A hook for any post-processing after a command has been visited
    fn finish_command(
        &self,
        context: Self::ExecutionContext,
        state: &mut Self::State,
    ) -> Diagnostics;

    /// custom visit for a command. It will skip `command` if `command_custom` returns true.
    fn command_custom(
        &self,
        _context: &mut Self::ExecutionContext,
        _state: &mut Self::State,
        _cmd: &Command,
    ) -> bool {
        false
    }
    fn command(
        &self,
        context: &mut Self::ExecutionContext,
        state: &mut Self::State,
        cmd: &Command,
    ) {
        use H::Command_ as C;
        if self.command_custom(context, state, cmd) {
            return;
        }
        let sp!(_, cmd_) = cmd;
        match cmd_ {
            C::Assign(_, ls, e) => {
                let values = self.exp(context, state, e);
                self.lvalues(context, state, ls, values);
            }
            C::Mutate(el, er) => {
                self.exp(context, state, er);
                self.exp(context, state, el);
            }
            C::JumpIf { cond: e, .. }
            | C::VariantSwitch { subject: e, .. }
            | C::IgnoreAndPop { exp: e, .. }
            | C::Return { exp: e, .. }
            | C::Abort(_, e) => {
                self.exp(context, state, e);
            }
            C::Jump { .. } => (),
            C::Break(_) | C::Continue(_) => panic!("ICE break/continue not translated to jumps"),
        }
    }

    fn lvalues(
        &self,
        context: &mut Self::ExecutionContext,
        state: &mut Self::State,
        ls: &[LValue],
        values: Vec<<Self::State as SimpleDomain>::Value>,
    ) {
        // pad with defautl to account for errors
        let padded_values = values.into_iter().chain(std::iter::repeat(
            <Self::State as SimpleDomain>::Value::default(),
        ));
        for (l, value) in ls.iter().zip(padded_values) {
            self.lvalue(context, state, l, value)
        }
    }

    /// custom visit for an lvalue. It will skip `lvalue` if `lvalue_custom` returns true.
    fn lvalue_custom(
        &self,
        _context: &mut Self::ExecutionContext,
        _state: &mut Self::State,
        _l: &LValue,
        _value: &<Self::State as SimpleDomain>::Value,
    ) -> bool {
        false
    }
    fn lvalue(
        &self,
        context: &mut Self::ExecutionContext,
        state: &mut Self::State,
        l: &LValue,
        value: <Self::State as SimpleDomain>::Value,
    ) {
        use LValue_ as L;
        if self.lvalue_custom(context, state, l, &value) {
            return;
        }
        let sp!(loc, l_) = l;
        match l_ {
            L::Ignore => (),
            L::Var { var: v, .. } => {
                let locals = state.locals_mut();
                locals.insert(*v, LocalState::Available(*loc, value));
            }
            L::Unpack(_, _, fields) => {
                for (_, l) in fields {
                    let v = <Self::State as SimpleDomain>::Value::default();
                    self.lvalue(context, state, l, v)
                }
            }
            L::UnpackVariant(_, _, _, _, _, fields) => {
                for (_, l) in fields {
                    let v = <Self::State as SimpleDomain>::Value::default();
                    self.lvalue(context, state, l, v)
                }
            }
        }
    }

    /// custom visit for an exp. It will skip `exp` and `call_custom` if `exp_custom` returns Some.
    fn exp_custom(
        &self,
        _context: &mut Self::ExecutionContext,
        _state: &mut Self::State,
        _parent_e: &Exp,
    ) -> Option<Vec<<Self::State as SimpleDomain>::Value>> {
        None
    }
    fn call_custom(
        &self,
        _context: &mut Self::ExecutionContext,
        _state: &mut Self::State,
        _loc: &Loc,
        _return_ty: &Type,
        _f: &ModuleCall,
        _args: Vec<<Self::State as SimpleDomain>::Value>,
    ) -> Option<Vec<<Self::State as SimpleDomain>::Value>> {
        None
    }
    #[growing_stack]
    fn exp(
        &self,
        context: &mut Self::ExecutionContext,
        state: &mut Self::State,
        parent_e: &Exp,
    ) -> Vec<<Self::State as SimpleDomain>::Value> {
        use H::UnannotatedExp_ as E;
        if let Some(vs) = self.exp_custom(context, state, parent_e) {
            return vs;
        }
        let eloc = &parent_e.exp.loc;
        match &parent_e.exp.value {
            E::Move { var, .. } => {
                let locals = state.locals_mut();
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
                let locals = state.locals_mut();
                match locals.get(var) {
                    Some(LocalState::Available(_, value)) => vec![value.clone()],
                    // Possible error case
                    _ => default_values(1),
                }
            }
            E::BorrowLocal(_, _) => default_values(1),
            E::Freeze(e)
            | E::Dereference(e)
            | E::Borrow(_, e, _, _)
            | E::Cast(e, _)
            | E::UnaryExp(_, e) => {
                self.exp(context, state, e);
                default_values(1)
            }
            E::Vector(_, n, _, args) => {
                for arg in args {
                    self.exp(context, state, arg);
                }
                default_values(*n)
            }
            E::ModuleCall(mcall) => {
                let evalues = mcall
                    .arguments
                    .iter()
                    .flat_map(|arg| self.exp(context, state, arg))
                    .collect();
                if let Some(vs) =
                    self.call_custom(context, state, eloc, &parent_e.ty, mcall, evalues)
                {
                    return vs;
                }

                default_values_for_ty(&parent_e.ty)
            }

            E::Unit { .. } => vec![],
            E::Value(_) | E::Constant(_) | E::UnresolvedError | E::ErrorConstant { .. } => {
                default_values(1)
            }

            E::BinopExp(e1, _, e2) => {
                self.exp(context, state, e1);
                self.exp(context, state, e2);
                default_values(1)
            }
            E::Pack(_, _, fields) => {
                for (_, _, e) in fields {
                    self.exp(context, state, e);
                }
                default_values(1)
            }
            E::PackVariant(_, _, _, fields) => {
                for (_, _, e) in fields {
                    self.exp(context, state, e);
                }
                default_values(1)
            }
            E::Multiple(es) => es
                .iter()
                .flat_map(|e| self.exp(context, state, e))
                .collect(),
            E::Unreachable => panic!("ICE should not analyze dead code"),
        }
    }
}

/// Provides default values depending on the arity of the type
pub fn default_values_for_ty<V: Clone + Default>(ty: &Type) -> Vec<V> {
    match &ty.value {
        Type_::Unit => vec![],
        Type_::Single(_) => default_values(1),
        Type_::Multiple(ts) => default_values(ts.len()),
    }
}

#[inline(always)]
/// A simple constructor for n default values
pub fn default_values<V: Clone + Default>(c: usize) -> Vec<V> {
    vec![V::default(); c]
}

impl<V: SimpleAbsInt> TransferFunctions for V {
    type State = V::State;

    fn execute(
        &mut self,
        pre: &mut Self::State,
        _lbl: Label,
        _idx: usize,
        cmd: &Command,
    ) -> Diagnostics {
        let mut context = self.start_command(pre);
        self.command(&mut context, pre, cmd);
        self.finish_command(context, pre)
    }
}
impl<V: SimpleAbsInt> AbstractInterpreter for V {}

impl<V: SimpleAbsIntConstructor> AbstractInterpreterVisitor for V {
    fn verify(
        &mut self,
        env: &CompilationEnv,
        context: &CFGContext,
        cfg: &ImmForwardCFG,
    ) -> Diagnostics {
        SimpleAbsIntConstructor::verify(self, env, context, cfg)
    }
}
