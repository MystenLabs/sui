//! Detects nested `if` statements that can be simplified by combining conditions with `&&`.
//! Encourages more concise and readable conditional logic in code.
//! Aims to improve code maintainability and reduce unnecessary nesting.
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

use super::{LinterDiagnosticCategory, COLLAPSIBLE_NESTED_IF_DIAG_CODE, LINT_WARNING_PREFIX};

const COLLAPSIBLE_NESTED_IF_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Complexity as u8,
    COLLAPSIBLE_NESTED_IF_DIAG_CODE,
    "",
);

pub struct CollapsibleNestedIf;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for CollapsibleNestedIf {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        if let UnannotatedExp_::IfElse(_, if_block, _) = &exp.exp.value {
            if let UnannotatedExp_::Block(seq) = &if_block.exp.value {
                if seq.1.len() == 1 {
                    if let T::SequenceItem_::Seq(seq_exp) = &seq.1[0].value {
                        if let UnannotatedExp_::IfElse(_, _, _) = &seq_exp.exp.value {
                            report_collapsible_nested_if(self.env, exp.exp.loc);
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

fn report_collapsible_nested_if(env: &mut CompilationEnv, loc: Loc) {
    let diag = diag!(
        COLLAPSIBLE_NESTED_IF_DIAG,
        (loc, "This `if` statement can be collapsed")
    );
    env.add_diag(diag);
}
