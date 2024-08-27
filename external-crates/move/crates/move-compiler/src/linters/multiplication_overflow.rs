// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This linter rule detects potential overflow in multiplication operations across various integer types.
//! It handles both same-type and mixed-type multiplications, issuing warnings when overflow is possible.

use crate::expansion::ast::ModuleIdent;
use crate::linters::StyleCodes;
use crate::naming::ast::Var_;
use crate::parser::ast::FunctionName;
use crate::typing::ast::{Function, LValue_};
use crate::{
    diag,
    diagnostics::WarningFilters,
    expansion::ast::Value_,
    naming::ast::{BuiltinTypeName_, TypeName_, Type_},
    parser::ast::BinOp_,
    shared::CompilationEnv,
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};
use move_ir_types::location::Loc;
use std::collections::{BTreeMap, VecDeque};
use std::str::FromStr;

pub struct MultiplicationOverflow;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
    variable_values: BTreeMap<Var_, Value_>,
}

impl TypingVisitorConstructor for MultiplicationOverflow {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context {
            env,
            variable_values: BTreeMap::new(),
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
        fdef: &mut Function,
    ) -> bool {
        if let T::FunctionBody_::Defined((_, seq)) = &mut fdef.body.value {
            for seq_item in seq {
                use T::SequenceItem_ as SI;
                if let SI::Bind(binds, _types, exp) = &seq_item.value {
                    self.process_bindings(binds, exp);
                }
            }
        }
        false
    }
}

impl Context<'_> {
    fn process_bindings(&mut self, binds: &T::LValueList, exp: &T::Exp) {
        for bind in binds.value.iter() {
            self.check_for_overflow(exp);
            self.update_variable_value(bind, exp);
        }
    }

    fn check_for_overflow(&mut self, exp: &T::Exp) {
        let potential_overflows = self.collect_and_check_mul_expressions(exp);
        for loc in potential_overflows {
            self.env.add_diag(diag!(
                StyleCodes::MultiplicationOverflow.diag_info(),
                (
                    loc,
                    "Potential overflow detected in multiplication operation"
                )
            ));
        }
    }

    fn update_variable_value(&mut self, bind: &T::LValue, exp: &T::Exp) {
        if let LValue_::Var { var, .. } = &bind.value {
            if let UnannotatedExp_::Annotate(var_exp, _) = &exp.exp.value {
                if let UnannotatedExp_::Value(sp!(_, value)) = &var_exp.exp.value {
                    self.variable_values
                        .insert(var.value.clone(), value.clone());
                }
            }
        }
    }
    fn get_value(&self, var: &Value_) -> Option<u128> {
        match var {
            Value_::U16(v) => Some(*v as u128),
            Value_::U32(v) => Some(*v as u128),
            Value_::U64(v) => Some(*v as u128),
            Value_::U128(v) => Some(*v),
            Value_::InferredNum(v) | Value_::U256(v) => {
                let u256_val = move_core_types::u256::U256::from_str(&v.to_string()).ok()?;
                if u256_val > u128::MAX.into() {
                    None
                } else {
                    Some(u256_val.unchecked_as_u128())
                }
            }
            _ => None,
        }
    }

    fn check_mul_operation(
        &mut self,
        lhs: &T::Exp,
        rhs: &T::Exp,
        loc: Loc,
        overflows: &mut Vec<Loc>,
    ) {
        if let (Some(lhs_value), Some(rhs_value)) =
            (self.estimate_value(lhs), self.estimate_value(rhs))
        {
            let lhs_type = get_integer_type(&lhs.ty.value);
            let rhs_type = get_integer_type(&rhs.ty.value);
            if let (Some(lhs_type), Some(rhs_type)) = (lhs_type, rhs_type) {
                if self.check_overflow(lhs_type, rhs_type, lhs_value, rhs_value) {
                    overflows.push(loc);
                }
            }
        }
    }

    fn estimate_value(&mut self, exp: &T::Exp) -> Option<u128> {
        match &exp.exp.value {
            UnannotatedExp_::Value(v) => self.get_value(&v.value),
            UnannotatedExp_::Copy { var, .. } => self
                .variable_values
                .get(&var.value)
                .and_then(|v| self.get_value(v)),
            _ => None,
        }
    }

    fn collect_and_check_mul_expressions(&mut self, exp: &T::Exp) -> Vec<Loc> {
        let mut to_visit = VecDeque::new();
        let mut potential_overflows = Vec::new();
        to_visit.push_back(exp);
        while let Some(current_exp) = to_visit.pop_front() {
            if let UnannotatedExp_::BinopExp(lhs, op, _, rhs) = &current_exp.exp.value {
                if matches!(op.value, BinOp_::Mul) {
                    self.check_mul_operation(lhs, rhs, op.loc, &mut potential_overflows);
                }
                to_visit.push_back(lhs);
                to_visit.push_back(rhs);
            }
        }

        potential_overflows
    }

    fn check_overflow(
        &mut self,
        lhs_type: BuiltinTypeName_,
        rhs_type: BuiltinTypeName_,
        lhs: u128,
        rhs: u128,
    ) -> bool {
        let result_type = get_result_type(lhs_type, rhs_type);
        match result_type {
            BuiltinTypeName_::U8 => (lhs as u8).checked_mul(rhs as u8).is_none(),
            BuiltinTypeName_::U16 => (lhs as u16).checked_mul(rhs as u16).is_none(),
            BuiltinTypeName_::U32 => (lhs as u32).checked_mul(rhs as u32).is_none(),
            BuiltinTypeName_::U64 => (lhs as u64).checked_mul(rhs as u64).is_none(),
            BuiltinTypeName_::U128 => lhs.checked_mul(rhs).is_none(),
            BuiltinTypeName_::U256 => {
                lhs > u128::MAX || rhs > u128::MAX || lhs.checked_mul(rhs).is_none()
            }
            _ => false,
        }
    }
}

fn get_integer_type(exp_type: &Type_) -> Option<BuiltinTypeName_> {
    match exp_type {
        Type_::Apply(_, sp!(_, TypeName_::Builtin(sp!(_, typ))), _) => Some(typ.clone()),
        _ => None,
    }
}

fn get_result_type(lhs_type: BuiltinTypeName_, rhs_type: BuiltinTypeName_) -> BuiltinTypeName_ {
    use BuiltinTypeName_::*;
    match (lhs_type, rhs_type) {
        (U256, _) | (_, U256) => U256,
        (U128, _) | (_, U128) => U128,
        (U64, _) | (_, U64) => U64,
        (U32, _) | (_, U32) => U32,
        (U16, _) | (_, U16) => U16,
        (U8, U8) => U8,
        _ => U128, // Default case, though this should not happen in practice
    }
}
