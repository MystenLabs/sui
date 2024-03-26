//! Detects and suggests simplification for `if c { true } else { false }` and its reverse pattern.
//! Encourages using the condition directly (or its negation) for clearer and more concise code.
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    expansion::ast::Value_,
    shared::{program_info::TypingProgramInfo, CompilationEnv},
    typing::{
        ast::{self as T, SequenceItem_, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;

use super::{LinterDiagCategory, LINTER_DEFAULT_DIAG_CODE, LINT_WARNING_PREFIX};

const REDUNDANT_CONDITIONAL_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagCategory::RedundantConditional as u8,
    LINTER_DEFAULT_DIAG_CODE,
    "",
);

pub struct RedundantConditional;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}

impl TypingVisitorConstructor for RedundantConditional {
    type Context<'a> = Context<'a>;

    fn context<'a>(
        env: &'a mut CompilationEnv,
        _program_info: &'a TypingProgramInfo,
        _program: &T::Program_,
    ) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        match &exp.exp.value {
            UnannotatedExp_::IfElse(condition, if_block, else_block) => {
                let condition_name: String = match condition.exp.value {
                    UnannotatedExp_::Copy { var, .. } => {
                        let name = var.value.name.clone();
                        name.as_str().to_owned()
                    }
                    _ => "".to_owned(),
                };
                let extract_bool_literal_from_block = |block: &T::Exp| -> Option<bool> {
                    if let UnannotatedExp_::Block(seq) = &block.exp.value {
                        if seq.1.len() == 1 {
                            if let SequenceItem_::Seq(seq_exp) = &seq.1[0].value {
                                if let UnannotatedExp_::Value(val) = &seq_exp.exp.value {
                                    if let Value_::Bool(b) = &val.value {
                                        return Some(*b);
                                    }
                                }
                            }
                        }
                    }
                    None
                };

                if let Some(if_block_bool) = extract_bool_literal_from_block(&if_block) {
                    if let Some(else_block_bool) = extract_bool_literal_from_block(&else_block) {
                        let mut recommend_condition = format!("{}", condition_name);
                        if !if_block_bool {
                            recommend_condition = format!("!{}", condition_name);
                        }
                        if if_block_bool != else_block_bool {
                            report_redundant_conditional(
                                self.env,
                                exp.exp.loc,
                                condition_name.as_str(),
                                if_block_bool.to_string().as_str(),
                                else_block_bool.to_string().as_str(),
                                recommend_condition.as_str(),
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
    condition: &str,
    if_body: &str,
    else_body: &str,
    corrected: &str,
) {
    let msg = format!(
        "Detected a redundant conditional expression `if {} {{ {} }} else {{ {} }}`. Consider using `{}` directly.",
        condition, if_body, else_body, corrected
    );
    let diag = diag!(REDUNDANT_CONDITIONAL_DIAG, (loc, msg));
    env.add_diag(diag);
}
