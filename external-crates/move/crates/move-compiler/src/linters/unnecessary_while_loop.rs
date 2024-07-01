//! Encourages replacing `while(true)` with `loop` for infinite loops in Move for clarity and conciseness.
//! Identifies `while(true)` patterns, suggesting a more idiomatic approach using `loop`.
//! Aims to enhance code readability and adherence to Rust idioms.
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

use super::{LinterDiagnosticCategory, LINT_WARNING_PREFIX, WHILE_TRUE_TO_LOOP_DIAG_CODE};

const WHILE_TRUE_TO_LOOP_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Complexity as u8,
    WHILE_TRUE_TO_LOOP_DIAG_CODE,
    "",
);

pub struct WhileTrueToLoop;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for WhileTrueToLoop {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        if let UnannotatedExp_::While(_, cond, _) = &exp.exp.value {
            if is_condition_always_true(&cond.exp.value) {
                report_while_true_to_loop(self.env, exp.exp.loc);
            }
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

fn is_condition_always_true(condition: &UnannotatedExp_) -> bool {
    if let UnannotatedExp_::Value(val) = condition {
        if let Value_::Bool(b) = &val.value {
            return *b;
        }
    }
    false
}
fn report_while_true_to_loop(env: &mut CompilationEnv, loc: Loc) {
    let diag = diag!(
        WHILE_TRUE_TO_LOOP_DIAG,
        (
            loc,
            "Detected `while(true) {}` loop. Consider replacing with `loop {}`"
        )
    );
    env.add_diag(diag);
}
