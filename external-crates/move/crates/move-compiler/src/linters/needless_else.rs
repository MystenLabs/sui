//! Detects empty `else` branches in conditional structures, suggesting their removal for cleaner code.
//! Aims to flag potentially unnecessary or unimplemented placeholders within `if-else` statements.
//! Encourages code clarity and maintainability by eliminating redundant branches.
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

use super::{LinterDiagnosticCategory, EMPTY_ELSE_BRANCH_DIAG_CODE, LINT_WARNING_PREFIX};

const EMPTY_ELSE_BRANCH_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Complexity as u8,
    EMPTY_ELSE_BRANCH_DIAG_CODE,
    "",
);

pub struct EmptyElseBranch;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for EmptyElseBranch {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        if let UnannotatedExp_::IfElse(_, _, else_block) = &exp.exp.value {
            // Determine if the else block is empty
            let mut else_block_is_empty = false;
            if let UnannotatedExp_::Block(seq) = &else_block.exp.value {
                if seq.1.len() == 1 {
                    if let SequenceItem_::Seq(seq_exp) = &seq.1[0].value {
                        if matches!(seq_exp.exp.value, UnannotatedExp_::Unit { trailing: true }) {
                            else_block_is_empty = true;
                        }
                    }
                };
            }

            if else_block_is_empty {
                report_empty_else_branch(self.env, else_block.exp.loc);
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

fn report_empty_else_branch(env: &mut CompilationEnv, loc: Loc) {
    let diag = diag!(
        EMPTY_ELSE_BRANCH_DIAG,
        (
            loc,
            "Detected an empty `else` branch, which may be unnecessary."
        )
    );
    env.add_diag(diag);
}
