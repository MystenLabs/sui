// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Implements lint rule for Move code to detect unnecessary `public entry` functions.
// It identifies and reports functions that contain both `public` and `entry` modifiers.

use super::{LINT_WARNING_PREFIX, LinterDiagnosticCategory, LinterDiagnosticCode};
use crate::{
    diag,
    diagnostics::codes::{DiagnosticInfo, Severity, custom},
    expansion::ast::{ModuleIdent, Visibility},
    parser::ast::FunctionName,
    typing::{
        ast::{self as T},
        visitor::simple_visitor,
    },
};

const PUBLIC_ENTRY_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Sui as u8,
    LinterDiagnosticCode::UnnecessaryPublicEntry as u8,
    "unnecessary `entry` on a `public` function",
);

simple_visitor!(
    UnnecessaryPublicEntry,
    fn visit_function_custom(
        &mut self,
        _module: ModuleIdent,
        _function_name: FunctionName,
        fdef: &T::Function,
    ) -> bool {
        let is_entry = fdef.entry.is_some();
        let is_public = matches!(fdef.visibility, Visibility::Public(_));

        if is_entry && is_public {
            let mut d = diag!(
                PUBLIC_ENTRY_DIAG,
                (
                    fdef.entry.unwrap(),
                    "`entry` on `public` functions limits composability as it adds restrictions, e.g. the type of each return value must have `drop`. `entry` on `public` is only meaningful in niche scenarios."
                )
            );

            d.add_note("`public` functions can be called from PTBs. `entry` can be used to allow non-`public` functions to be called from PTBs, but it adds restrictions on the usage of input arguments and on the type of return values. Unless this `public` function interacts with an intricate set of other `entry` functions, the `entry` modifier should be removed.");
            self.add_diag(d);
            return true;
        }

        false
    }
);
