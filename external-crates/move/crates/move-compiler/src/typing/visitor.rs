// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::command_line::compiler::Visitor;
use crate::diagnostics::WarningFilters;
use crate::expansion::ast::ModuleIdent;
use crate::parser::ast::{ConstantName, FunctionName};
use crate::shared::{program_info::TypingProgramInfo, CompilationEnv};
use crate::typing::ast as T;

pub type TypingVisitorObj = Box<dyn TypingVisitor>;

pub trait TypingVisitor {
    fn visit(
        &mut self,
        env: &mut CompilationEnv,
        program_info: &TypingProgramInfo,
        program: &mut T::Program_,
    );

    fn visitor(self) -> Visitor
    where
        Self: 'static + Sized,
    {
        Visitor::TypingVisitor(Box::new(self))
    }
}

pub trait TypingVisitorConstructor {
    type Context<'a>: Sized + TypingVisitorContext;

    fn context<'a>(
        env: &'a mut CompilationEnv,
        program_info: &'a TypingProgramInfo,
        program: &T::Program_,
    ) -> Self::Context<'a>;

    fn visit(
        &mut self,
        env: &mut CompilationEnv,
        program_info: &TypingProgramInfo,
        program: &mut T::Program_,
    ) {
        let mut context = Self::context(env, program_info, program);
        context.visit(program);
    }
}

pub trait TypingVisitorContext {
    fn add_warning_filter_scope(&mut self, filter: WarningFilters);
    fn pop_warning_filter_scope(&mut self);

    fn visit_module_custom(
        &mut self,
        _ident: ModuleIdent,
        _mdef: &mut T::ModuleDefinition,
    ) -> bool {
        false
    }

    /// By default, the visitor will visit all all expressions in all functions in all modules. A
    /// custom version should of this function should be created if different type of analysis is
    /// required.
    fn visit(&mut self, program: &mut T::Program_) {
        for (mident, mdef) in program.modules.key_cloned_iter_mut() {
            self.add_warning_filter_scope(mdef.warning_filter.clone());
            if self.visit_module_custom(mident, mdef) {
                self.pop_warning_filter_scope();
                continue;
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

    // TODO struct and type visiting

    fn visit_constant_custom(
        &mut self,
        _module: ModuleIdent,
        _constant_name: ConstantName,
        _cdef: &mut T::Constant,
    ) -> bool {
        false
    }
    fn visit_constant(
        &mut self,
        module: ModuleIdent,
        constant_name: ConstantName,
        cdef: &mut T::Constant,
    ) {
        self.add_warning_filter_scope(cdef.warning_filter.clone());
        if self.visit_constant_custom(module, constant_name, cdef) {
            self.pop_warning_filter_scope();
            return;
        }
        self.visit_exp(&mut cdef.value);
        self.pop_warning_filter_scope();
    }

    fn visit_function_custom(
        &mut self,
        _module: ModuleIdent,
        _function_name: FunctionName,
        _fdef: &mut T::Function,
    ) -> bool {
        false
    }
    fn visit_function(
        &mut self,
        module: ModuleIdent,
        function_name: FunctionName,
        fdef: &mut T::Function,
    ) {
        self.add_warning_filter_scope(fdef.warning_filter.clone());
        if self.visit_function_custom(module, function_name, fdef) {
            self.pop_warning_filter_scope();
            return;
        }
        if let T::FunctionBody_::Defined(seq) = &mut fdef.body.value {
            self.visit_seq(seq);
        }
        self.pop_warning_filter_scope();
    }

    fn visit_seq(&mut self, seq: &mut T::Sequence) {
        for s in seq {
            self.visit_seq_item(s);
        }
    }

    fn visit_seq_item(&mut self, sp!(_, seq_item): &mut T::SequenceItem) {
        use T::SequenceItem_ as SI;
        match seq_item {
            SI::Seq(e) => self.visit_exp(e),
            SI::Declare(_) => (),
            SI::Bind(_, _, e) => self.visit_exp(e),
        }
    }

    /// Custom visit for an expression. It will skip `visit_exp` if `visit_exp_custom` returns true.
    fn visit_exp_custom(&mut self, _exp: &mut T::Exp) -> bool {
        false
    }

    fn visit_exp(&mut self, exp: &mut T::Exp) {
        use T::UnannotatedExp_ as E;
        if self.visit_exp_custom(exp) {
            return;
        }
        let sp!(_, uexp) = &mut exp.exp;
        match uexp {
            E::ModuleCall(c) => self.visit_exp(&mut c.arguments),
            E::Builtin(_, e) => self.visit_exp(e),
            E::Vector(_, _, _, e) => self.visit_exp(e),
            E::IfElse(e1, e2, e3) => {
                self.visit_exp(e1);
                self.visit_exp(e2);
                self.visit_exp(e3);
            }
            E::Match(esubject, arms) => {
                self.visit_exp(esubject);
                for sp!(_, arm) in arms.value.iter_mut() {
                    if let Some(guard) = arm.guard.as_mut() {
                        self.visit_exp(guard)
                    }
                    self.visit_exp(&mut arm.rhs);
                }
            }
            E::VariantMatch(esubject, _, arms) => {
                self.visit_exp(esubject);
                for (_, earm) in arms.iter_mut() {
                    self.visit_exp(earm);
                }
            }
            E::While(e1, _, e2) => {
                self.visit_exp(e1);
                self.visit_exp(e2);
            }
            E::Loop { body, .. } => self.visit_exp(body),
            E::NamedBlock(_, seq) => self.visit_seq(seq),
            E::Block(seq) => self.visit_seq(seq),
            E::Assign(_, _, e) => self.visit_exp(e),
            E::Mutate(e1, e2) => {
                self.visit_exp(e1);
                self.visit_exp(e2);
            }
            E::Return(e) => self.visit_exp(e),
            E::Abort(e) => self.visit_exp(e),
            E::Give(_, e) => self.visit_exp(e),
            E::Dereference(e) => self.visit_exp(e),
            E::UnaryExp(_, e) => self.visit_exp(e),
            E::BinopExp(e1, _, _, e2) => {
                self.visit_exp(e1);
                self.visit_exp(e2);
            }
            E::Pack(_, _, _, fields) => fields
                .iter_mut()
                .for_each(|(_, _, (_, (_, e)))| self.visit_exp(e)),
            E::PackVariant(_, _, _, _, fields) => fields
                .iter_mut()
                .for_each(|(_, _, (_, (_, e)))| self.visit_exp(e)),
            E::ExpList(list) => {
                for l in list {
                    match l {
                        T::ExpListItem::Single(e, _) => self.visit_exp(e),
                        T::ExpListItem::Splat(_, e, _) => self.visit_exp(e),
                    }
                }
            }
            E::Borrow(_, e, _) => self.visit_exp(e),
            E::TempBorrow(_, e) => self.visit_exp(e),
            E::Cast(e, _) => self.visit_exp(e),
            E::Annotate(e, _) => self.visit_exp(e),
            E::Unit { .. }
            | E::Value(_)
            | E::Move { .. }
            | E::Copy { .. }
            | E::Use(_)
            | E::Constant(..)
            | E::Continue(_)
            | E::BorrowLocal(..)
            | E::UnresolvedError => (),
        }
    }
}

impl<V: TypingVisitor + 'static> From<V> for TypingVisitorObj {
    fn from(value: V) -> Self {
        Box::new(value)
    }
}

impl<V: TypingVisitorConstructor> TypingVisitor for V {
    fn visit(
        &mut self,
        env: &mut CompilationEnv,
        program_info: &TypingProgramInfo,
        program: &mut T::Program_,
    ) {
        self.visit(env, program_info, program)
    }
}
