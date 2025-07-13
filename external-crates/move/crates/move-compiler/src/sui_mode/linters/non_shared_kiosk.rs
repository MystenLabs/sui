// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tracks transfer operations on `sui::kiosk::Kiosk` type. If a `Kiosk` is passed
//! to a `public_transfer` or `public_freeze_object` function, it will emit a warning,
//! suggesting to use `transfer::public_share_object` instead.

// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This linter rule checks for structs with an `id` field of type `UID` without the `key` ability.

use super::{LINT_WARNING_PREFIX, LinterDiagnosticCategory, LinterDiagnosticCode};
use crate::{
    diag,
    diagnostics::codes::{DiagnosticInfo, Severity, custom},
    sui_mode::{
        SUI_ADDR_VALUE,
        linters::{KIOSK_MOD_NAME, KIOSK_STRUCT_NAME, PUBLIC_SHARE_FUN, TRANSFER_MOD_NAME},
    },
    typing::{
        ast::{self as T},
        visitor::simple_visitor,
    },
};

const NON_SHARED_KIOSK_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Sui as u8,
    LinterDiagnosticCode::NonSharedKiosk as u8,
    "Kiosk should be shared with `public_share_object`",
);

simple_visitor!(
    NonSharedKioskVisitor,
    fn visit_exp_custom(&mut self, exp: &T::Exp) -> bool {
        match &exp.exp.value {
            // Match all function calls in the transfer module that are not `public_share_object`.
            T::UnannotatedExp_::ModuleCall(call)
                if (call.module.value.is(&SUI_ADDR_VALUE, TRANSFER_MOD_NAME)
                    && !call.name.eq(PUBLIC_SHARE_FUN)) =>
            {
                // Check if the type argument is `sui::kiosk::Kiosk`.
                if call.type_arguments[0].value.is(
                    &SUI_ADDR_VALUE,
                    KIOSK_MOD_NAME,
                    KIOSK_STRUCT_NAME,
                ) {
                    self.add_diag(diag!(
                        NON_SHARED_KIOSK_DIAG,
                        (
                            call.arguments.exp.loc,
                            "Kiosk should be shared with `public_share_object`"
                        )
                    ));
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }
);
