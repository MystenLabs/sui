// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Tracks transfer operations on `sui::kiosk::Kiosk` type. If a `Kiosk` is passed
//! to a `public_transfer` or `public_freeze_object` function, it will emit a warning,
//! suggesting to use `transfer::public_share_object` instead.

use super::{LINT_WARNING_PREFIX, LinterDiagnosticCategory, LinterDiagnosticCode};
use crate::{
    diag,
    diagnostics::codes::{DiagnosticInfo, Severity, custom},
    expansion::ast::ModuleIdent,
    naming::ast as N,
    parser::ast::DatatypeName,
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
    // Check that no defined structs have a field of type `Kiosk`.
    fn visit_struct_custom(
        &mut self,
        _module: ModuleIdent,
        _struct_name: DatatypeName,
        sdef: &N::StructDefinition,
    ) -> bool {
        if let N::StructFields::Defined(_, fields) = &sdef.fields {
            for (_, _, (_, field)) in fields {
                if is_kiosk_type(&field.1) {
                    self.add_diag(diag!(
                        NON_SHARED_KIOSK_DIAG,
                        (field.1.loc, "Kiosk should not be used as a field, use it as a top-level shared object with `public_share_object`")
                    ));
                    return true;
                }
            }
        }

        false
    },
    fn visit_exp_custom(&mut self, exp: &T::Exp) -> bool {
        match &exp.exp.value {
            // Match all function calls in the transfer module that are not `public_share_object`.
            T::UnannotatedExp_::ModuleCall(call)
                if (call.module.value.is(&SUI_ADDR_VALUE, TRANSFER_MOD_NAME)
                    && !call.name.eq(PUBLIC_SHARE_FUN)) =>
            {
                // Check if the type argument is `sui::kiosk::Kiosk`.
                if is_kiosk_type(&call.type_arguments[0]) {
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
            T::UnannotatedExp_::Pack(_, _, types, _) => {
                for type_ in types {
                    if is_kiosk_type(type_) {
                        self.add_diag(diag!(
                            NON_SHARED_KIOSK_DIAG,
                            (type_.loc, "Kiosk should not be used as a field, use it as a top-level shared object with `public_share_object`")
                        ));
                    }
                }
                true
            }
            _ => false,
        }
    }
);

fn is_kiosk_type(ty: &N::Type) -> bool {
    match &ty.value {
        N::Type_::Apply(_ability, tn, _type_args) => {
            tn.value
                .is(&SUI_ADDR_VALUE, KIOSK_MOD_NAME, KIOSK_STRUCT_NAME)
        }
        _ => false,
    }
}
