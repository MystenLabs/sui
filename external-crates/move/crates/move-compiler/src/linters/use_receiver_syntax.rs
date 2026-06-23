// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Suggests using receiver (method) syntax `e.f()` instead of `m::f(e)` when the first
//! argument's type is defined in the same module as the called function.

use crate::{
    diag,
    editions::FeatureGate,
    expansion::ast::ModuleIdent,
    linters::StyleCodes,
    naming::ast::{TypeInner, TypeName_},
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::simple_visitor,
    },
};

simple_visitor!(
    UseReceiverSyntax,
    fn visit_module_custom(
        &mut self,
        _ident: ModuleIdent,
        mdef: &T::ModuleDefinition,
    ) -> bool {
        // Receiver syntax requires the DotCall feature (Move 2024+).
        !self.env.supports_feature(mdef.package_name, FeatureGate::DotCall)
    },
    fn visit_exp_custom(&mut self, exp: &T::Exp) -> bool {
        let UnannotatedExp_::ModuleCall(mcall) = &exp.exp.value else {
            return false;
        };

        // Already using receiver syntax.
        if mcall.method_name.is_some() {
            return false;
        }

        // Must have at least one parameter.
        if mcall.parameter_types.is_empty() {
            return false;
        }

        // Check if the first parameter's type (possibly behind a reference) is defined
        // in the same module as the called function.
        let first_param_ty = &mcall.parameter_types[0];
        let inner_ty = match first_param_ty.value.inner() {
            TypeInner::Ref(_, inner) => inner.value.inner(),
            other => other,
        };

        let type_matches_module = match inner_ty {
            TypeInner::Apply(_, sp!(_, TypeName_::ModuleType(type_module, _)), _) => {
                **type_module == mcall.module
            }
            // Builtins whose std module supports receiver syntax:
            // vector, u8, u16, u32, u64, u128, u256, bool, etc.
            TypeInner::Apply(_, sp!(_, TypeName_::Builtin(sp!(_, builtin))), _) => {
                mcall.module.value.named_address_is("std", &builtin.to_string())
            }
            _ => false,
        };

        if !type_matches_module {
            return false;
        }

        let name = &mcall.name;
        let module = &mcall.module;
        let n_params = mcall.parameter_types.len();
        let msg = if n_params > 1 {
            let rest: Vec<_> = (1..n_params).map(|i| format!("arg{i}")).collect();
            let rest = rest.join(", ");
            format!(
                "Consider using receiver syntax: \
                 'arg0.{name}({rest})' instead of '{module}::{name}(arg0, {rest})'",
            )
        } else {
            format!(
                "Consider using receiver syntax: \
                 'arg0.{name}()' instead of '{module}::{name}(arg0)'",
            )
        };
        self.add_diag(diag!(
            StyleCodes::UseReceiverSyntax.diag_info(),
            (exp.exp.loc, msg),
        ));

        false
    }
);
