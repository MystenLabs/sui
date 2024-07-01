//! Checks for empty `if` branches with no accompanying `else` branch, suggesting potential redundancy.
//! Aims to improve code clarity by highlighting conditional structures that perform no action.
//! Encourages developers to either complete the conditional logic or remove the unnecessary `if`.
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    shared::CompilationEnv,
    typing::{
        ast::{self as T, SequenceItem_, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;

use super::{LinterDiagnosticCategory, EMPTY_IF_NO_ELSE_DIAG_CODE, LINT_WARNING_PREFIX};

const EMPTY_IF_NO_ELSE_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Complexity as u8,
    EMPTY_IF_NO_ELSE_DIAG_CODE,
    "",
);

pub struct EmptyIfNoElse;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for EmptyIfNoElse {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        if let UnannotatedExp_::IfElse(_, if_block, else_block) = &exp.exp.value {
            let mut if_block_is_empty = false;
            if let UnannotatedExp_::Block(seq) = &if_block.exp.value {
                if seq.1.len() == 1 {
                    if let SequenceItem_::Seq(seq_exp) = &seq.1[0].value {
                        if matches!(seq_exp.exp.value, UnannotatedExp_::Unit { trailing: true }) {
                            if_block_is_empty = true;
                        }
                    }
                };
            }
            let no_else_block = matches!(
                else_block.exp.value,
                UnannotatedExp_::Unit { trailing: false }
            );
            if if_block_is_empty && no_else_block {
                report_empty_if_no_else(self.env, exp.exp.loc);
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

fn report_empty_if_no_else(env: &mut CompilationEnv, loc: Loc) {
    let diag = diag!(
        EMPTY_IF_NO_ELSE_DIAG,
        (loc, "Detected an empty `if` branch without an `else` branch. Consider removing or completing the conditional.")
    );
    env.add_diag(diag);
}
