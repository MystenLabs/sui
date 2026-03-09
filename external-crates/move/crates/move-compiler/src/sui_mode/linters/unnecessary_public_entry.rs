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
        // TODO link to docs about hot or not cliques
        const PUBLIC_ENTRY_NOTE: &str = "`public` functions can be called from PTBs. `entry` can \
            be used to allow non-`public` (private or `public(package)`) functions to be called \
            from PTBs, although there will be additional restrictions on the input arguments to \
            such functions.";
        let is_entry = fdef.entry.is_some();
        let is_public = matches!(fdef.visibility, Visibility::Public(_));

        if is_entry && is_public {
            let msg = "`entry` on `public` is meaningless. In conjunction with `public`, `entry` \
                adds no additional permissions or restrictions.";
            let mut d = diag!(PUBLIC_ENTRY_DIAG, (fdef.entry.unwrap(), msg));
            d.add_note(PUBLIC_ENTRY_NOTE);
            self.add_diag(d);
            return true;
        }

        false
    }
);
