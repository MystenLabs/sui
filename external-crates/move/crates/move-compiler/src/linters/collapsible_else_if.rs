//! Detects `else { if ... }` structures that can be simplified to `else if ...`, reducing unnecessary nesting.
//! Encourages cleaner, more readable code by eliminating the extra block around the nested `if`.
//! Aims to streamline conditional logic, making it easier to follow and maintain.
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

use super::{LinterDiagnosticCategory, COLLAPSIBLE_ELSEFILTER_DIAG_CODE, LINT_WARNING_PREFIX};

const COLLAPSIBLE_ELSE_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Complexity as u8,
    COLLAPSIBLE_ELSEFILTER_DIAG_CODE,
    "",
);

pub struct CollapsibleElseIf;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for CollapsibleElseIf {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        if let UnannotatedExp_::IfElse(_, _, else_block) = &exp.exp.value {
            if let UnannotatedExp_::Block(sed) = &else_block.exp.value {
                if sed.1.len() == 1 {
                    if let T::SequenceItem_::Seq(seq_exp) = &sed.1[0].value {
                        if let UnannotatedExp_::IfElse(_, _, _) = &seq_exp.exp.value {
                            report_collapsible_else(self.env, else_block.exp.loc);
                        }
                    }
                }
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

fn report_collapsible_else(env: &mut CompilationEnv, loc: Loc) {
    let diag = diag!(
        COLLAPSIBLE_ELSE_DIAG,
        (loc, "Detected a collapsible `else { if ... }` expression. Consider refactoring to `else if ...`.")
    );
    env.add_diag(diag);
}
