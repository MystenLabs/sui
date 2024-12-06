// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! `ConstantNamingVisitor` enforces a naming convention for constants in Move programs,
//! requiring them to follow an ALL_CAPS_SNAKE_CASE or PascalCase format. This lint checks each constant's name
//! within a module against this convention.
use crate::{
    diag,
    expansion::ast::ModuleIdent,
    linters::StyleCodes,
    parser::ast::ConstantName,
    typing::{ast as T, visitor::simple_visitor},
};

simple_visitor!(
    ConstantNaming,
    fn visit_constant_custom(
        &mut self,
        _module: ModuleIdent,
        constant_name: ConstantName,
        cdef: &T::Constant,
    ) -> bool {
        let name = constant_name.0.value.as_str();
        if !is_valid_name(name) {
            let uid_msg =
                format!("'{name}' should be ALL_CAPS. Or for error constants, use PascalCase",);
            let diagnostic = diag!(StyleCodes::ConstantNaming.diag_info(), (cdef.loc, uid_msg));
            self.add_diag(diagnostic);
        }
        false
    }
);

/// Returns `true` if the string is in all caps snake case, including numeric characters.
fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(start) = chars.next() else {
        return false; /* ice? */
    };
    if !start.is_uppercase() {
        return false;
    }

    let mut all_uppers = true;
    let mut has_underscore = false;
    for char in chars {
        if char.is_lowercase() {
            all_uppers = false;
        } else if char == '_' {
            has_underscore = true;
        } else if !char.is_alphanumeric() {
            return false; // bail if it's not alphanumeric ?
        }

        // We have an underscore but we have non-uppercase letters
        if has_underscore && !all_uppers {
            return false;
        }
    }
    true
}
