// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0
//! Defines a linter for identifying likely comparison mistakes in Move functions by analyzing variable comparisons.
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    expansion::ast::{ModuleIdent, Mutability, Value_},
    naming::ast::{BuiltinTypeName_, Type, TypeName_, Type_, Var, Var_},
    parser::ast::{BinOp_, FunctionName},
    shared::CompilationEnv,
    typing::{
        ast::{self as T, FunctionBody_, LValue_, SequenceItem, SequenceItem_, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;
use std::collections::{BTreeMap, VecDeque};

use super::{LinterDiagnosticCategory, LIKELY_MISTAKE_DIAG_CODE, LINT_WARNING_PREFIX};

const LIKELY_MISTAKE_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Correctness as u8,
    LIKELY_MISTAKE_DIAG_CODE, // Replace with specific code if desired
    "Expression suggests an unintended comparison",
);

pub struct LikelyComparisonMistake;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
    max_var_list: BTreeMap<Var_, BuiltinTypeName_>,
    min_var_list: BTreeMap<Var_, BuiltinTypeName_>,
    params: BTreeMap<Var_, BuiltinTypeName_>,
    declared_vars: BTreeMap<Var_, BuiltinTypeName_>,
}

impl TypingVisitorConstructor for LikelyComparisonMistake {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context {
            env,
            max_var_list: BTreeMap::new(),
            min_var_list: BTreeMap::new(),
            params: BTreeMap::new(),
            declared_vars: BTreeMap::new(),
        }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn visit_function_custom(
        &mut self,
        _module: ModuleIdent,
        _function_name: FunctionName,
        fdef: &mut T::Function,
    ) -> bool {
        self.collect_function_params(&fdef.signature.parameters);
        if let FunctionBody_::Defined(seq) = &fdef.body.value {
            self.collect_variable_types(&seq.1);
        }
        false
    }
    fn visit_exp_custom(&mut self, exp: &mut T::Exp) -> bool {
        if let UnannotatedExp_::BinopExp(e1, sp!(_, op), _, e2) = &exp.exp.value {
            self.check_comparison_operations(e1, e2, op, exp.exp.loc);
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

impl Context<'_> {
    fn collect_function_params(&mut self, params: &Vec<(Mutability, Var, Type)>) {
        for (_, sp!(_, var), var_type) in params {
            if let Type_::Apply(_, sp!(_, TypeName_::Builtin(sp!(_, builtin_type_name))), _) =
                var_type.value
            {
                self.params.insert(*var, builtin_type_name);
            }
        }
    }

    fn collect_variable_types(&mut self, seq: &VecDeque<SequenceItem>) {
        for sp!(_, seq_item) in seq {
            if let SequenceItem_::Bind(sp!(_, value_list), _, seq_exp) = seq_item {
                if let Some(value) = value_list.get(0) {
                    if let LValue_::Var { var, ty, .. } = &value.value {
                        match &seq_exp.exp.value {
                            UnannotatedExp_::Value(sp!(_, value)) => {
                                self.classify_variable(&ty.value, value, &var.value);
                            }
                            UnannotatedExp_::Annotate(exp, _) => {
                                if let UnannotatedExp_::Value(sp!(_, value)) = &exp.exp.value {
                                    self.classify_variable(&ty.value, value, &var.value);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    fn check_comparison_operations(&mut self, e1: &T::Exp, e2: &T::Exp, op: &BinOp_, loc: Loc) {
        if let UnannotatedExp_::Copy {
            var: sp!(_, var2), ..
        } = e2.exp.value
        {
            match op {
                BinOp_::Gt | BinOp_::Lt => self.report_if_mistake(e1, &var2, op, loc),
                _ => {}
            }
        }
    }

    fn report_if_mistake(&mut self, e1: &T::Exp, var2: &Var_, op: &BinOp_, loc: Loc) {
        if let UnannotatedExp_::Copy {
            var: sp!(_, var1), ..
        } = e1.exp.value
        {
            let is_max_value_comparison =
                matches!(op, BinOp_::Gt) && self.max_var_list.contains_key(var2);
            let is_min_value_comparison =
                matches!(op, BinOp_::Lt) && self.min_var_list.contains_key(var2);

            if (is_max_value_comparison || is_min_value_comparison)
                && (self.params.get(&var1) == self.max_var_list.get(var2)
                    || self.declared_vars.get(&var1) == self.max_var_list.get(var2))
            {
                let message = format!(
                    "Cannot compare {} with {} value",
                    if is_max_value_comparison {
                        "parameter"
                    } else {
                        "declared variable"
                    },
                    if is_max_value_comparison {
                        "max"
                    } else {
                        "min"
                    }
                );
                let diag = diag!(LIKELY_MISTAKE_DIAG, (loc, message));
                self.env.add_diag(diag);
            }
        }
    }

    fn classify_variable(&mut self, ty: &Type_, value: &Value_, var: &Var_) {
        if let Some(builtin_type_name) = ty.get_builtin_type() {
            match value {
                Value_::U8(u8::MAX)
                | Value_::U16(u16::MAX)
                | Value_::U32(u32::MAX)
                | Value_::U64(u64::MAX)
                | Value_::U128(u128::MAX) => {
                    self.max_var_list.insert(*var, builtin_type_name);
                }
                Value_::U8(u8::MIN)
                | Value_::U16(u16::MIN)
                | Value_::U32(u32::MIN)
                | Value_::U64(u64::MIN)
                | Value_::U128(u128::MIN) => {
                    self.min_var_list.insert(*var, builtin_type_name);
                }
                _ => {
                    self.declared_vars.insert(*var, builtin_type_name);
                }
            }
        }
    }
}

trait TypeExtensions {
    fn get_builtin_type(&self) -> Option<BuiltinTypeName_>;
}

impl TypeExtensions for Type_ {
    fn get_builtin_type(&self) -> Option<BuiltinTypeName_> {
        if let Type_::Apply(_, sp!(_, TypeName_::Builtin(sp!(_, builtin_type_name))), _) = self {
            Some(*builtin_type_name)
        } else {
            None
        }
    }
}
