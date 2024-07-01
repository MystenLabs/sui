// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Detects out-of-bounds array (or vector) indexing with a constant index in Move code.
//! This lint aims to statically identify instances where an index access on an array or vector
//! exceeds the bounds known at compile time, potentially indicating logical errors in the code.
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    expansion::ast::{ModuleIdent, Value_},
    naming::ast::Var_,
    parser::ast::FunctionName,
    shared::CompilationEnv,
    typing::{
        ast::{self as T, ExpListItem, LValue_, ModuleCall, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::{Loc, Spanned};
use std::collections::BTreeMap;

use super::{LinterDiagnosticCategory, LINT_WARNING_PREFIX, OUT_OF_BOUNDS_INDEXING_DIAG_CODE};

const OUT_OF_BOUNDS_INDEXING_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Correctness as u8,
    OUT_OF_BOUNDS_INDEXING_DIAG_CODE,
    "Array index out of bounds: attempting to access index {} in array '{}' with size known at compile time.",
);

pub struct OutOfBoundsArrayIndexing;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
    array_list: BTreeMap<Var_, usize>,
}

impl TypingVisitorConstructor for OutOfBoundsArrayIndexing {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context {
            env,
            array_list: BTreeMap::new(),
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
        if let T::FunctionBody_::Defined((_, seq)) = &fdef.body.value {
            for seq_item in seq {
                use T::SequenceItem_ as SI;
                if let T::SequenceItem_::Bind(value_list, _, seq_exp) = &seq_item.value {
                    if let UnannotatedExp_::Vector(_, size, _, _) = &seq_exp.exp.value {
                        if let Some(sp!(_, LValue_::Var { var, .. })) = &value_list.value.get(0) {
                            self.array_list.insert(var.value, *size);
                        }
                    }
                }
                match &seq_item.value {
                    SI::Seq(e) => {
                        visit_function_exp_custom(&mut self.env, &mut self.array_list, &e.exp)
                    }
                    SI::Declare(_) => (),
                    SI::Bind(_, _, e) => {
                        visit_function_exp_custom(&mut self.env, &mut self.array_list, &e.exp)
                    }
                };
            }
        }
        self.array_list.clear();
        false
    }

    fn add_warning_filter_scope(&mut self, filter: WarningFilters) {
        self.env.add_warning_filter_scope(filter)
    }

    fn pop_warning_filter_scope(&mut self) {
        self.env.pop_warning_filter_scope()
    }
}

fn visit_function_exp_custom(
    env: &mut CompilationEnv,
    array_list: &mut BTreeMap<Var_, usize>,
    exp: &Spanned<UnannotatedExp_>,
) {
    if let UnannotatedExp_::ModuleCall(module_call) = &exp.value {
        if is_array_push(module_call) {
            if let UnannotatedExp_::ExpList(exp_list) = &module_call.arguments.exp.value {
                if let Some(ExpListItem::Single(arr_arg_exp, _)) = &exp_list.get(0) {
                    if let UnannotatedExp_::BorrowLocal(_, sp!(_, array_arg)) =
                        &arr_arg_exp.exp.value
                    {
                        if let Some(array_size) = array_list.get(array_arg) {
                            array_list.insert(*array_arg, array_size + 1);
                        }
                    }
                }
            }
        }
        if is_array_pop(module_call) {
            if let UnannotatedExp_::BorrowLocal(_, sp!(_, array_arg)) =
                &module_call.arguments.exp.value
            {
                if let Some(array_size) = array_list.get(array_arg) {
                    array_list.insert(*array_arg, array_size - 1);
                }
            }
        }
        if is_vector_borrow(module_call) {
            if let UnannotatedExp_::ExpList(exp_list) = &module_call.arguments.exp.value {
                if let Some(ExpListItem::Single(arr_arg_exp, _)) = &exp_list.get(0) {
                    if let UnannotatedExp_::BorrowLocal(_, sp!(_, array_arg)) =
                        &arr_arg_exp.exp.value
                    {
                        if let Some(array_size) = array_list.get(array_arg) {
                            if let Some(ExpListItem::Single(value_exp, _)) = &exp_list.get(1) {
                                if let UnannotatedExp_::Value(sp!(_, size)) = &value_exp.exp.value {
                                    let index = extract_value(&size);
                                    if index > (*array_size as u128 - 1) {
                                        report_out_of_bounds_indexing(
                                            env, array_arg, index, exp.loc,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn is_vector_borrow(module_call: &ModuleCall) -> bool {
    (module_call.name.0.value.as_str() == "borrow"
        || module_call.name.0.value.as_str() == "borrow_mut")
        && module_call.module.value.module.0.value.as_str() == "vector"
}

fn is_array_push(module_call: &ModuleCall) -> bool {
    module_call.name.0.value.as_str() == "push_back"
        && module_call.module.value.module.0.value.as_str() == "vector"
}

fn is_array_pop(module_call: &ModuleCall) -> bool {
    module_call.name.0.value.as_str() == "pop_back"
        && module_call.module.value.module.0.value.as_str() == "vector"
}

fn extract_value(value: &Value_) -> u128 {
    match value {
        Value_::U8(v) => *v as u128,
        Value_::U16(v) => *v as u128,
        Value_::U32(v) => *v as u128,
        Value_::U64(v) => *v as u128,
        Value_::U128(v) => *v,
        _ => 0,
    }
}
fn report_out_of_bounds_indexing(env: &mut CompilationEnv, var: &Var_, index: u128, loc: Loc) {
    let msg = format!(
        "Array index out of bounds: attempting to access index {} in array '{}' with size known at compile time.",
        index, var.name.as_str()
    );
    let diag = diag!(OUT_OF_BOUNDS_INDEXING_DIAG, (loc, msg));
    env.add_diag(diag);
}
