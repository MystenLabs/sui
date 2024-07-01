// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module defines a lint rule to detect and report unused mutable parameters in functions.
//! It tracks parameter usage within function bodies to determine if 'mut' qualifiers are necessary.
use std::collections::BTreeMap;

use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    expansion::ast::ModuleIdent,
    naming::ast::{Type_, Var_},
    parser::ast::FunctionName,
    shared::CompilationEnv,
    typing::{
        ast::{self as T, ExpListItem, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;

use super::{LinterDiagnosticCategory, LINT_WARNING_PREFIX, UNUSED_MUT_PARAMS_DIAG_CODE};

const UNUSED_MUT_PARAMS_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Suspicious as u8,
    UNUSED_MUT_PARAMS_DIAG_CODE,
    "Detected a mutable parameter that is never modified or passed as mutable. Consider removing the 'mut' qualifier.",
);

pub struct UnusedMutableParamsCheck;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
    not_mutable_params: BTreeMap<Var_, Loc>,
}

impl TypingVisitorConstructor for UnusedMutableParamsCheck {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context {
            env,
            not_mutable_params: BTreeMap::new(),
        }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        match &exp.exp.value {
            UnannotatedExp_::Mutate(exp1, _) => {
                if let UnannotatedExp_::Borrow(_, borrow_exp, _) = &exp1.exp.value {
                    if let Some(var) = extract_var_from_exp(borrow_exp) {
                        if self.not_mutable_params.contains_key(&var) {
                            self.not_mutable_params.remove(&var);
                        }
                    }
                }
            }
            UnannotatedExp_::ModuleCall(module_call) => {
                if let UnannotatedExp_::ExpList(exp_list) = &module_call.arguments.exp.value {
                    exp_list.iter().for_each(|exp_item| {
                        if let ExpListItem::Single(arg, _) = exp_item {
                            if let Some(var) = extract_var_from_exp(arg) {
                                if self.not_mutable_params.contains_key(&var) {
                                    self.not_mutable_params.remove(&var);
                                }
                            }
                        }
                    });
                }
            }
            _ => (),
        }

        false
    }

    fn visit_function_custom(
        &mut self,
        module: ModuleIdent,
        _function_name: FunctionName,
        fdef: &mut T::Function,
    ) -> bool {
        self.not_mutable_params.clear(); // Clear previous state
        if module.value.address.to_string() != "std" {
            for (_, sp!(_, var), var_type) in &fdef.signature.parameters {
                if matches!(var_type.value, Type_::Ref(true, _)) {
                    self.not_mutable_params.insert(*var, var_type.loc); // Initially assume 'mut' is not used
                }
            }

            if let T::FunctionBody_::Defined(seq) = &mut fdef.body.value {
                self.visit_seq(seq);
            }

            self.not_mutable_params.iter().for_each(|(var, loc)| {
                report_unused_mut_param(self.env, *loc, &var.name.as_str());
            });
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

fn extract_var_from_exp(exp: &T::Exp) -> Option<Var_> {
    match &exp.exp.value {
        UnannotatedExp_::Copy {
            var: sp!(_, var), ..
        } => Some(*var),
        _ => None,
    }
}

fn report_unused_mut_param(env: &mut CompilationEnv, loc: Loc, var_name: &str) {
    let msg = format!(
        "The mutable parameter '{}' is never modified or passed as mutable. Consider removing the 'mut' qualifier.",
        var_name
    );
    let diag = diag!(UNUSED_MUT_PARAMS_DIAG, (loc, msg));
    env.add_diag(diag);
}
