//! Detects empty loop expressions, including `while(true) {}` and `loop {}` without exit mechanisms, highlighting potential infinite loops.
//! Aims to identify and warn against loops that may lead to hangs or excessive resource consumption due to lack of content.
//! Encourages adding meaningful logic within loops or ensuring proper exit conditions to improve code reliability and maintainability.
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    expansion::ast::Value_,
    shared::CompilationEnv,
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;

use super::{LinterDiagnosticCategory, EMPTY_LOOP_DIAG_CODE, LINT_WARNING_PREFIX};

const EMPTY_LOOP_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Complexity as u8,
    EMPTY_LOOP_DIAG_CODE,
    "",
);

pub struct EmptyLoop;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for EmptyLoop {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        match &exp.exp.value {
            UnannotatedExp_::Loop {
                name: _,
                has_break,
                body,
            } => {
                if !has_break {
                    report_empty_loop(self.env, exp.exp.loc);
                }
                if is_body_empty(&body.exp.value) {
                    report_empty_loop(self.env, exp.exp.loc);
                }
            }
            UnannotatedExp_::While(_, cond, body) => {
                if is_condition_always_true(&cond.exp.value) && is_body_empty(&body.exp.value) {
                    report_empty_loop(self.env, exp.exp.loc);
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

fn is_body_empty(body: &UnannotatedExp_) -> bool {
    if let UnannotatedExp_::Block(seq) = body {
        if seq.1.len() == 1 {
            if let T::SequenceItem_::Seq(seq_exp) = &seq.1[0].value {
                if matches!(seq_exp.exp.value, UnannotatedExp_::Unit { trailing: true }) {
                    return true;
                }
            }
        }
    }
    false
}

fn is_condition_always_true(condition: &UnannotatedExp_) -> bool {
    if let UnannotatedExp_::Value(val) = condition {
        if let Value_::Bool(b) = &val.value {
            return *b;
        }
    }
    false
}

fn report_empty_loop(env: &mut CompilationEnv, loc: Loc) {
    let diag = diag!(
        EMPTY_LOOP_DIAG,
        (
            loc,
            "Detected an empty loop expression potentially leading to an infinite loop."
        )
    );
    env.add_diag(diag);
}
