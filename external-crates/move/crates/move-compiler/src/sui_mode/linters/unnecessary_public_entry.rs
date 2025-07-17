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
    "unnecessary public entry",
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
        let is_public = matches!(fdef.compiled_visibility, Visibility::Public(_));

        if is_entry && is_public {
            let mut d = diag!(
                PUBLIC_ENTRY_DIAG,
                (
                    fdef.entry.unwrap(),
                    "`entry` on `public` functions is meaningless except in niche use cases."
                )
            );

            d.add_note("`public` functions can be called from PTBs. As such, `entry` on `public` functions should be used only if you are concerned with the value flow limitations around `entry` functions. If you do not have an intricate set of private `entry` functions alongside it, an `entry` modifier on a `public` function is superfluous.");
            self.add_diag(d);
        }

        false
    }
);
