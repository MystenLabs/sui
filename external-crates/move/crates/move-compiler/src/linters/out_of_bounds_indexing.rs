use crate::{
    diag,
    diagnostics::WarningFilters,
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

use super::StyleCodes;

pub struct OutOfBoundsArrayIndexing;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
    array_sizes: BTreeMap<Var_, usize>,
}

impl TypingVisitorConstructor for OutOfBoundsArrayIndexing {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context {
            env,
            array_sizes: BTreeMap::new(),
        }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn add_warning_filter_scope(&mut self, filter: WarningFilters) {
        self.env.add_warning_filter_scope(filter)
    }

    fn pop_warning_filter_scope(&mut self) {
        self.env.pop_warning_filter_scope()
    }

    fn visit_function_custom(
        &mut self,
        _module: ModuleIdent,
        _function_name: FunctionName,
        fdef: &mut T::Function,
    ) -> bool {
        if let T::FunctionBody_::Defined((_, seq)) = &fdef.body.value {
            for seq_item in seq {
                self.process_sequence_item(seq_item);
            }
        }
        self.array_sizes.clear();
        false
    }
}

impl Context<'_> {
    fn process_sequence_item(&mut self, seq_item: &T::SequenceItem) {
        use T::SequenceItem_ as SI;
        match &seq_item.value {
            SI::Bind(value_list, _, seq_exp) => {
                self.update_array_size(value_list, seq_exp);
                self.visit_expression(&seq_exp.exp);
            }
            SI::Seq(e) => self.visit_expression(&e.exp),
            SI::Declare(_) => {}
        }
    }

    fn update_array_size(&mut self, value_list: &T::LValueList, seq_exp: &T::Exp) {
        if let UnannotatedExp_::Vector(_, size, _, _) = &seq_exp.exp.value {
            if let Some(sp!(_, LValue_::Var { var, .. })) = value_list.value.get(0) {
                self.array_sizes.insert(var.value.clone(), *size);
            }
        }
    }

    fn visit_expression(&mut self, exp: &Spanned<UnannotatedExp_>) {
        if let UnannotatedExp_::ModuleCall(module_call) = &exp.value {
            self.process_module_call(module_call, exp.loc);
        }
    }

    fn process_module_call(&mut self, module_call: &ModuleCall, loc: Loc) {
        if is_vector_operation(module_call, "borrow")
            || is_vector_operation(module_call, "borrow_mut")
        {
            self.check_vector_bounds(module_call, loc);
        } else if is_vector_operation(module_call, "push_back") {
            self.update_array_size_after_push(module_call);
        } else if is_vector_operation(module_call, "pop_back") {
            self.update_array_size_after_pop(module_call);
        }
    }

    fn check_vector_bounds(&mut self, module_call: &ModuleCall, loc: Loc) {
        if let UnannotatedExp_::ExpList(exp_list) = &module_call.arguments.exp.value {
            if let (
                Some(ExpListItem::Single(arr_arg_exp, _)),
                Some(ExpListItem::Single(value_exp, _)),
            ) = (exp_list.get(0), exp_list.get(1))
            {
                if let UnannotatedExp_::BorrowLocal(_, sp!(_, array_arg)) = &arr_arg_exp.exp.value {
                    if let Some(array_size) = self.array_sizes.get(array_arg) {
                        if let UnannotatedExp_::Value(sp!(_, size)) = &value_exp.exp.value {
                            let index = extract_value(size);
                            if index > (*array_size as u128 - 1) {
                                report_out_of_bounds_indexing(self.env, array_arg, index, loc);
                            }
                        }
                    }
                }
            }
        }
    }

    fn update_array_size_after_push(&mut self, module_call: &ModuleCall) {
        if let UnannotatedExp_::ExpList(exp_list) = &module_call.arguments.exp.value {
            if let Some(ExpListItem::Single(arr_arg_exp, _)) = exp_list.get(0) {
                if let UnannotatedExp_::BorrowLocal(_, sp!(_, array_arg)) = &arr_arg_exp.exp.value {
                    if let Some(array_size) = self.array_sizes.get_mut(array_arg) {
                        *array_size += 1;
                    }
                }
            }
        }
    }

    fn update_array_size_after_pop(&mut self, module_call: &ModuleCall) {
        if let UnannotatedExp_::BorrowLocal(_, sp!(_, array_arg)) = &module_call.arguments.exp.value
        {
            if let Some(array_size) = self.array_sizes.get_mut(array_arg) {
                *array_size = array_size.saturating_sub(1);
            }
        }
    }
}

fn is_vector_operation(module_call: &ModuleCall, operation: &str) -> bool {
    module_call.name.0.value.as_str() == operation
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
    let diag = diag!(StyleCodes::OutOfBoundsIndexing.diag_info(), (loc, msg));
    env.add_diag(diag);
}
