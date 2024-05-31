// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Detects and reports explicit self-assignments in code, such as `x = x;`, which are generally unnecessary
//! and could indicate potential errors or misunderstandings in the code logic.
use super::StyleCodes;
use crate::typing::ast::ExpListItem::Single;
use crate::typing::ast::LValueList;
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    shared::CompilationEnv,
    typing::{
        ast::{self as T, LValue_, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;

pub struct SelfAssignmentVisitor;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for SelfAssignmentVisitor {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn add_warning_filter_scope(&mut self, filter: WarningFilters) {
        self.env.add_warning_filter_scope(filter)
    }

    fn pop_warning_filter_scope(&mut self) {
        self.env.pop_warning_filter_scope()
    }

    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        match &exp.exp.value {
            UnannotatedExp_::Mutate(lhs, rhs) => self.check_mutate(lhs, rhs, exp.exp.loc),
            UnannotatedExp_::Assign(value_list, _, assign_exp) => {
                self.check_assign(value_list, assign_exp, exp.exp.loc)
            }
            _ => false,
        }
    }
}

impl Context<'_> {
    fn check_mutate(&mut self, lhs: &T::Exp, rhs: &T::Exp, loc: Loc) -> bool {
        if let UnannotatedExp_::Dereference(inner_exp) = &rhs.exp.value {
            match (&lhs.exp.value, &inner_exp.exp.value) {
                (
                    UnannotatedExp_::Borrow(_, inner_lhs, lhs_field),
                    UnannotatedExp_::Borrow(_, inner_rhs, rhs_field),
                ) if inner_lhs == inner_rhs && lhs_field == rhs_field => {
                    self.report_self_assignment(loc);
                    true
                }
                (
                    UnannotatedExp_::Copy {
                        var: sp!(_, lhs), ..
                    },
                    UnannotatedExp_::Copy {
                        var: sp!(_, rhs), ..
                    },
                ) if lhs == rhs => {
                    self.report_self_assignment(loc);
                    true
                }
                _ => false,
            }
        } else {
            false
        }
    }

    fn check_assign(&mut self, value_list: &LValueList, assign_exp: &T::Exp, loc: Loc) -> bool {
        if let UnannotatedExp_::ExpList(rhs_expressions) = &assign_exp.exp.value {
            let is_self_assignment =
                value_list
                    .value
                    .iter()
                    .zip(rhs_expressions.iter())
                    .all(|(lhs_value, rhs_exp)| {
                        if let (
                            sp!(
                                _,
                                LValue_::Var {
                                    var: sp!(_, lhs),
                                    ..
                                }
                            ),
                            Single(inner_exp, _),
                        ) = (lhs_value, rhs_exp)
                        {
                            matches!(
                                &inner_exp.exp.value,
                                UnannotatedExp_::Copy {
                                    var: sp!(_, rhs),
                                    ..
                                }
                                | UnannotatedExp_::Move {
                                    var: sp!(_, rhs),
                                    ..
                                }
                                if lhs == rhs
                            )
                        } else {
                            false
                        }
                    });

            if is_self_assignment && !value_list.value.is_empty() {
                self.report_self_assignment(loc);
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    fn report_self_assignment(&mut self, loc: Loc) {
        self.env.add_diag(diag!(
        StyleCodes::SelfAssignment.diag_info(),
            (
                loc,
                "Explicit self-assignment detected for variable. Consider removing it to clarify intent."
            )
        ));
    }
}
