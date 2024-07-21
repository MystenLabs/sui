// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Detects and suggests simplification for `if c { true } else { false }` and its reverse pattern.
//! Encourages using the condition directly (or its negation) for clearer and more concise code.
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    expansion::ast::Value_,
    shared::CompilationEnv,
    typing::{
        ast::{self as T, SequenceItem_, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;

use super::{LinterDiagnosticCategory, LINT_WARNING_PREFIX, REDUNDANT_CONDITIONAL_DIAG_CODE};

const REDUNDANT_CONDITIONAL_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Complexity as u8,
    REDUNDANT_CONDITIONAL_DIAG_CODE,
    "",
);

pub struct RedundantConditional;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for RedundantConditional {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        match &exp.exp.value {
            UnannotatedExp_::IfElse(_, if_block, else_block) => {
                let extract_bool_literal_from_block = |block: &T::Exp| -> Option<bool> {
                    match &block.exp.value {
                        UnannotatedExp_::Block(seq) => {
                            if seq.1.len() == 1 {
                                if let SequenceItem_::Seq(seq_exp) = &seq.1[0].value {
                                    if let sp!(_, UnannotatedExp_::Value(sp!(_, Value_::Bool(b)))) =
                                        &seq_exp.exp
                                    {
                                        return Some(*b);
                                    }
                                }
                            }
                        }
                        UnannotatedExp_::Value(sp!(_, Value_::Bool(b))) => return Some(*b),
                        UnannotatedExp_::Annotate(anno_exp, _) => {
                            if let sp!(_, UnannotatedExp_::Value(sp!(_, Value_::Bool(b)))) =
                                anno_exp.exp
                            {
                                return Some(b);
                            }
                        }
                        _ => (),
                    }
                    if let UnannotatedExp_::Block(seq) = &block.exp.value {
                        if seq.1.len() == 1 {
                            if let SequenceItem_::Seq(seq_exp) = &seq.1[0].value {
                                if let sp!(_, UnannotatedExp_::Value(sp!(_, Value_::Bool(b)))) =
                                    &seq_exp.exp
                                {
                                    return Some(*b);
                                }
                            }
                        }
                    }
                    None
                };

                if let Some(if_block_bool) = extract_bool_literal_from_block(&if_block) {
                    let else_block_bool = extract_bool_literal_from_block(&else_block);
                    if let Some(else_block_bool) = else_block_bool {
                        if if_block_bool != else_block_bool {
                            report_redundant_conditional(
                                self.env,
                                exp.exp.loc,
                                if_block_bool.to_string().as_str(),
                                else_block_bool.to_string().as_str(),
                            )
                        }
                    }
                }
            }
            _ => {}
        }
        false
    }
    fn add_warning_filter_scope(&mut self, filter: WarningFilters) {
        self.env.add_warning_filter_scope(filter)
    }

    fn pop_warning_filter_scope(&mut self) {
        self.env.pop_warning_filter_scope()
    }
}

fn report_redundant_conditional(
    env: &mut CompilationEnv,
    loc: Loc,
    if_body: &str,
    else_body: &str,
) {
    let msg = format!(
        "Detected a redundant conditional expression `if (...) {} else {}`. Consider using the condition directly.",
        if_body, else_body
    );
    let diag = diag!(REDUNDANT_CONDITIONAL_DIAG, (loc, msg));
    env.add_diag(diag);
}
