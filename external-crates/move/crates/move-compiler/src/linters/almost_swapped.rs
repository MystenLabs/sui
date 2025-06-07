// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! The `AlmostSwapped` detects and warns about unnecessary swap sequences in Move code.
//! It identifies sequences of assignments that can be simplified to a single assignment.
use crate::{
    diag,
    diagnostics::Diagnostic,
    expansion::ast::ModuleIdent,
    linters::StyleCodes,
    naming::ast::Var_,
    parser::ast::FunctionName,
    typing::{
        ast::{self as T, LValue_, SequenceItem_, UnannotatedExp_},
        visitor::simple_visitor,
    },
};
use move_ir_types::location::Loc;
use std::collections::VecDeque;

#[derive(Debug)]
enum AssignmentType {
    Bind { var_name: Var_, source: Var_ },
    Assign { target: Var_, source: Var_ },
}

#[derive(Debug)]
struct Assignment {
    assignment_type: AssignmentType,
    location: Loc,
}

pub struct SwapSequence {
    assignments: VecDeque<Assignment>,
}

impl SwapSequence {
    fn new() -> Self {
        Self {
            assignments: VecDeque::with_capacity(3),
        }
    }

    fn clear_tracking(&mut self) {
        self.assignments.clear();
    }

    fn handle_sequence_item(&mut self, item: &T::SequenceItem_) -> Option<Diagnostic> {
        match item {
            SequenceItem_::Bind(sp!(_, bindings), _, exp) => {
                if let Some(sp!(_, binding)) = bindings.first() {
                    if let UnannotatedExp_::Copy {
                        var: sp!(_, source_var),
                        ..
                    } = &exp.exp.value
                    {
                        if let LValue_::Var { var, .. } = &binding {
                            let assignment = Assignment {
                                assignment_type: AssignmentType::Bind {
                                    var_name: var.value,
                                    source: *source_var,
                                },
                                location: exp.exp.loc,
                            };
                            self.assignments.push_back(assignment);
                        }
                    }
                }
            }
            SequenceItem_::Seq(seq) => {
                if let UnannotatedExp_::Assign(sp!(_, value_list), _, rhs) = &seq.exp.value {
                    if let Some(sp!(_, LValue_::Var { var, .. })) = value_list.first() {
                        if let UnannotatedExp_::Copy {
                            var: sp!(_, source_var),
                            ..
                        } = &rhs.exp.value
                        {
                            let assignment = Assignment {
                                assignment_type: AssignmentType::Assign {
                                    target: var.value,
                                    source: *source_var,
                                },
                                location: seq.exp.loc,
                            };
                            self.assignments.push_back(assignment);

                            // Check for swap pattern after each assignment
                            if let Some(diagnostic) = self.check_swap_pattern() {
                                return Some(diagnostic);
                            }
                        }
                    }
                }
            }
            _ => self.clear_tracking(),
        }
        None
    }

    fn check_swap_pattern(&self) -> Option<Diagnostic> {
        if self.assignments.len() < 3 {
            return None;
        }

        let assignments: Vec<_> = self.assignments.iter().rev().take(3).collect();
        if let [last, middle, first] = assignments[..] {
            if let (
                AssignmentType::Bind {
                    var_name: temp,
                    source: source1,
                },
                AssignmentType::Assign {
                    target: target1,
                    source: source2,
                },
                AssignmentType::Assign {
                    target: target2,
                    source,
                },
            ) = (
                &first.assignment_type,
                &middle.assignment_type,
                &last.assignment_type,
            ) {
                if source == temp && target1 == source1 && target2 == source2 {
                    return Some(self.create_swap_diagnostic(
                        last.location,
                        "Unnecessary swap sequence detected - consider using a tuple assignment",
                    ));
                }
            }
        }
        None
    }

    fn create_swap_diagnostic(&self, loc: Loc, message: &str) -> Diagnostic {
        diag!(StyleCodes::AlmostSwapped.diag_info(), (loc, message))
    }
}

simple_visitor!(
    AlmostSwapped,
    fn visit_function_custom(
        &mut self,
        _module: ModuleIdent,
        _function_name: FunctionName,
        fdef: &T::Function,
    ) -> bool {
        let mut swap_detector = SwapSequence::new();

        if let T::FunctionBody_::Defined((_, vec_item)) = &fdef.body.value {
            for sp!(_, seq_item) in vec_item {
                if let Some(diagnostic) = swap_detector.handle_sequence_item(seq_item) {
                    self.add_diag(diagnostic);
                }
            }
        }
        false
    }
);
