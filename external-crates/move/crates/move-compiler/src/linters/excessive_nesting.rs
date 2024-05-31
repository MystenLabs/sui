//! Detects excessively nested blocks of code, warning when nesting exceeds a predefined threshold.
//! Aims to improve code readability and maintainability by encouraging simpler, flatter code structures.
//! Issues a single warning for each sequence of nested blocks that surpasses the limit, to avoid redundant alerts.
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    shared::CompilationEnv,
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;

use super::{LinterDiagnosticCategory, EXCESSIVE_NESTING_DIAG_CODE, LINT_WARNING_PREFIX};

const EXCESSIVE_NESTING_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Complexity as u8,
    EXCESSIVE_NESTING_DIAG_CODE,
    "",
);
const NESTING_THRESHOLD: usize = 3;
pub struct ExcessiveNesting;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
    nesting_level: usize,
    warning_issued: bool,
}

impl TypingVisitorConstructor for ExcessiveNesting {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context {
            env,
            nesting_level: 0,
            warning_issued: false,
        }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, _exp: &mut T::Exp) -> bool {
        if let UnannotatedExp_::Block(_) = _exp.exp.value {
            self.nesting_level += 1;

            if self.nesting_level > NESTING_THRESHOLD && !self.warning_issued {
                report_excessive_nesting(self.env, _exp.exp.loc);
                self.warning_issued = true;
            }
        } else if self.nesting_level <= NESTING_THRESHOLD {
            self.warning_issued = false;
        };
        false
    }

    fn add_warning_filter_scope(&mut self, filter: WarningFilters) {
        self.env.add_warning_filter_scope(filter)
    }

    fn pop_warning_filter_scope(&mut self) {
        self.env.pop_warning_filter_scope()
    }
}

fn report_excessive_nesting(env: &mut CompilationEnv, loc: Loc) {
    let msg = "Detected excessive block nesting. Consider refactoring to simplify the code.";
    let diag = diag!(EXCESSIVE_NESTING_DIAG, (loc, msg));
    env.add_diag(diag);
}
