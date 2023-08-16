// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::command_line::compiler::Visitor;
use crate::shared::CompilationEnv;
use crate::typing::{ast as T, core::ProgramInfo};

pub type TypingVisitorObj = Box<dyn TypingVisitor>;

pub trait TypingVisitor {
    fn visitor(self) -> Visitor
    where
        Self: 'static + Sized,
    {
        Visitor::TypingVisitor(Box::new(self))
    }

    /// By default, the visitor will visit all all expressions in all functions in all modules. A
    /// custom version should of this function should be created if different type of analysis is
    /// required.
    fn visit(
        &mut self,
        env: &mut CompilationEnv,
        program_info: &ProgramInfo,
        program: &mut T::Program,
    ) {
        for (_, _, mdef) in program.modules.iter() {
            env.add_warning_filter_scope(mdef.warning_filter.clone());

            for (_, _, fdef) in mdef.functions.iter() {
                env.add_warning_filter_scope(fdef.warning_filter.clone());

                if let T::FunctionBody_::Defined(seq) = &fdef.body.value {
                    self.visit_seq(seq, env, program_info, program);
                }

                env.pop_warning_filter_scope();
            }

            env.pop_warning_filter_scope();
        }
    }

    fn visit_seq(
        &mut self,
        seq: &T::Sequence,
        env: &mut CompilationEnv,
        program_info: &ProgramInfo,
        program: &T::Program,
    ) {
        for s in seq {
            self.visit_seq_item(s, env, program_info, program);
        }
    }

    fn visit_seq_item(
        &mut self,
        sp!(_, seq_item): &T::SequenceItem,
        env: &mut CompilationEnv,
        program_info: &ProgramInfo,
        program: &T::Program,
    ) {
        use T::SequenceItem_ as SI;
        match seq_item {
            SI::Seq(e) => self.visit_exp(e, env, program_info, program),
            SI::Declare(_) => (),
            SI::Bind(_, _, e) => self.visit_exp(e, env, program_info, program),
        }
    }

    /// Custom visit for an expression. It will skip `visit_exp` if `visit_exp_custom` returns true.
    fn visit_exp_custom(
        &mut self,
        _exp: &T::Exp,
        _env: &mut CompilationEnv,
        _program_info: &ProgramInfo,
        _program: &T::Program,
    ) -> bool {
        false
    }

    fn visit_exp(
        &mut self,
        exp: &T::Exp,
        env: &mut CompilationEnv,
        program_info: &ProgramInfo,
        program: &T::Program,
    ) {
        use T::UnannotatedExp_ as E;
        if self.visit_exp_custom(exp, env, program_info, program) {
            return;
        }
        let sp!(_, uexp) = &exp.exp;
        match uexp {
            E::ModuleCall(c) => self.visit_exp(&c.arguments, env, program_info, program),
            E::Builtin(_, e) => self.visit_exp(e, env, program_info, program),
            E::Vector(_, _, _, e) => self.visit_exp(e, env, program_info, program),
            E::IfElse(e1, e2, e3) => {
                self.visit_exp(e1, env, program_info, program);
                self.visit_exp(e2, env, program_info, program);
                self.visit_exp(e3, env, program_info, program);
            }
            E::While(e1, e2) => {
                self.visit_exp(e1, env, program_info, program);
                self.visit_exp(e2, env, program_info, program);
            }
            E::Loop { has_break: _, body } => self.visit_exp(body, env, program_info, program),
            E::Block(seq) => self.visit_seq(seq, env, program_info, program),
            E::Assign(_, _, e) => self.visit_exp(e, env, program_info, program),
            E::Mutate(e1, e2) => {
                self.visit_exp(e1, env, program_info, program);
                self.visit_exp(e2, env, program_info, program);
            }
            E::Return(e) => self.visit_exp(e, env, program_info, program),
            E::Abort(e) => self.visit_exp(e, env, program_info, program),
            E::Dereference(e) => self.visit_exp(e, env, program_info, program),
            E::UnaryExp(_, e) => self.visit_exp(e, env, program_info, program),
            E::BinopExp(e1, _, _, e2) => {
                self.visit_exp(e1, env, program_info, program);
                self.visit_exp(e2, env, program_info, program);
            }
            E::Pack(_, _, _, fields) => fields
                .iter()
                .for_each(|(_, _, (_, (_, e)))| self.visit_exp(e, env, program_info, program)),
            E::ExpList(list) => {
                for l in list {
                    match l {
                        T::ExpListItem::Single(e, _) => {
                            self.visit_exp(e, env, program_info, program)
                        }
                        T::ExpListItem::Splat(_, e, _) => {
                            self.visit_exp(e, env, program_info, program)
                        }
                    }
                }
            }
            E::Borrow(_, e, _) => self.visit_exp(e, env, program_info, program),
            E::TempBorrow(_, e) => self.visit_exp(e, env, program_info, program),
            E::Cast(e, _) => self.visit_exp(e, env, program_info, program),
            E::Annotate(e, _) => self.visit_exp(e, env, program_info, program),
            E::Unit { .. }
            | E::Value(_)
            | E::Move { .. }
            | E::Copy { .. }
            | E::Use(_)
            | E::Constant(..)
            | E::Break
            | E::Continue
            | E::BorrowLocal(..)
            | E::Spec(..)
            | E::UnresolvedError => (),
        }
    }
}

impl<V: TypingVisitor + 'static> From<V> for TypingVisitorObj {
    fn from(value: V) -> Self {
        Box::new(value)
    }
}
