// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This linter checks for unnecessary mutable parameters in Move functions.
//! A parameter is considered unnecessary if it is marked as mutable but never modified within the function.
//! This linter is useful to identify and remove unused mutable parameters, which can help improve code readability and maintainability.

use crate::{
    diag,
    expansion::ast::{Address, ModuleIdent, Mutability},
    linters::StyleCodes,
    naming::ast::{Type, Type_, Var, Var_},
    parser::ast::FunctionName,
    typing::{
        ast::{self as T, ExpListItem, UnannotatedExp_},
        visitor::simple_visitor,
    },
};
use move_ir_types::location::Loc;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct MutableParamTracker {
    unused_params: BTreeMap<Var_, Loc>,
    used_params: BTreeSet<Var_>,
}

impl MutableParamTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn analyze_function(
        &mut self,
        parameters: &[(Mutability, Var, Type)],
        body: &T::FunctionBody_,
    ) {
        self.track_mutable_parameters(parameters);
        self.analyze_function_body(body);
    }

    pub fn get_unused_locations(&self) -> impl Iterator<Item = (&Var_, &Loc)> {
        self.unused_params
            .iter()
            .filter(|(var, _)| !self.used_params.contains(var))
    }

    fn track_mutable_parameters(&mut self, parameters: &[(Mutability, Var, Type)]) {
        self.unused_params.clear();
        self.used_params.clear();

        for (_, var, sp!(_, param_type)) in parameters {
            if Self::is_mutable_type(param_type) {
                self.unused_params.insert(var.value, var.loc);
            }
        }
    }

    fn is_mutable_type(param_type: &Type_) -> bool {
        matches!(param_type, Type_::Ref(true, _))
    }

    fn analyze_function_body(&mut self, body: &T::FunctionBody_) {
        if let T::FunctionBody_::Defined((_, items)) = body {
            for sp!(_, item) in items {
                if let T::SequenceItem_::Seq(exp) = item {
                    self.analyze_expression(exp);
                }
            }
        }
    }

    fn analyze_expression(&mut self, exp: &T::Exp) {
        match &exp.exp.value {
            UnannotatedExp_::Mutate(target, _) => self.handle_mutation(target),
            UnannotatedExp_::ModuleCall(call) => self.handle_module_call(&call.arguments),
            _ => {}
        }
    }

    fn handle_mutation(&mut self, target: &T::Exp) {
        if let UnannotatedExp_::Borrow(_, borrowed, _) = &target.exp.value {
            if let Some(var) = Self::extract_variable(borrowed) {
                self.used_params.insert(var);
            }
        }
    }

    fn handle_module_call(&mut self, arguments: &T::Exp) {
        if let UnannotatedExp_::ExpList(items) = &arguments.exp.value {
            for item in items {
                if let ExpListItem::Single(arg, _) = item {
                    if let Some(var) = Self::extract_variable(arg) {
                        self.used_params.insert(var);
                    }
                }
            }
        }
    }

    fn extract_variable(exp: &T::Exp) -> Option<Var_> {
        match &exp.exp.value {
            UnannotatedExp_::Copy {
                var: sp!(_, var), ..
            } => Some(*var),
            _ => None,
        }
    }
}

simple_visitor!(
    UnusedMutableParams,
    fn visit_function_custom(
        &mut self,
        module: ModuleIdent,
        _function_name: FunctionName,
        fdef: &T::Function,
    ) -> bool {
        // Skip std library modules
        if let Address::Numerical {
            name: Some(sp!(_, n)),
            ..
        } = module.value.address
        {
            if n == symbol!("std") {
                return false;
            }
        }

        let mut tracker = MutableParamTracker::new();
        tracker.analyze_function(&fdef.signature.parameters, &fdef.body.value);

        for (var, loc) in tracker.get_unused_locations() {
            self.add_diag(diag!(
                StyleCodes::UnnecessaryMutParams.diag_info(),
                (
                    *loc,
                    format!(
                        "Parameter '{}' is marked as mutable but never modified",
                        var.name.as_str()
                    )
                )
            ));
        }
        false
    }
);
