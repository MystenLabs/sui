// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Implements lint rule for Move code to detect unnecessary `public entry` functions.
// It identifies and reports functions that contain both `public` and `entry` modifiers.

use crate::{
    diag,
    expansion::ast::ModuleIdent,
    linters::StyleCodes,
    parser::ast::FunctionName,
    typing::{
        ast::{self as T},
        visitor::simple_visitor,
    },
};

simple_visitor!(
    UnnecessaryPublicEntry,
    fn visit_function_custom(
        &mut self,
        _module: ModuleIdent,
        _function_name: FunctionName,
        fdef: &T::Function,
    ) -> bool {
        let is_entry = fdef.entry.is_some();
        let is_public = fdef.compiled_visibility.is_public();

        if is_entry && is_public {
            self.add_diag(diag!(
                StyleCodes::UnnecessaryPublicEntry.diag_info(),
                (fdef.loc, "Unnecessary usage of `public` and `entry` modifiers. `public` functions can be called in other modules and transactions, consider using private `entry` for transaction-only visibility")
            ));
        }

        false
    }
);
