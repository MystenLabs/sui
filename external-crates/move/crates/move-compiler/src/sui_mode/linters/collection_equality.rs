// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This analysis flags situations when instances of a sui::table::Table or sui::table_vec::TableVec
//! or sui::bag::Bag are being compared for (in)equality at this type of comparison is not very
//! useful and DOES NOT take into consideration structural (in)equality.

use move_core_types::account_address::AccountAddress;
use move_symbol_pool::Symbol;

use crate::{
    diag,
    diagnostics::codes::{custom, DiagnosticInfo, Severity},
    parser::ast as P,
    sui_mode::{SUI_ADDR_NAME, SUI_ADDR_VALUE},
    typing::{ast as T, visitor::simple_visitor},
};

use super::{
    LinterDiagnosticCategory, LinterDiagnosticCode, BAG_MOD_NAME, BAG_STRUCT_NAME,
    LINKED_TABLE_MOD_NAME, LINKED_TABLE_STRUCT_NAME, LINT_WARNING_PREFIX, OBJECT_BAG_MOD_NAME,
    OBJECT_BAG_STRUCT_NAME, OBJECT_TABLE_MOD_NAME, OBJECT_TABLE_STRUCT_NAME, TABLE_MOD_NAME,
    TABLE_STRUCT_NAME, TABLE_VEC_MOD_NAME, TABLE_VEC_STRUCT_NAME, VEC_MAP_MOD_NAME,
    VEC_MAP_STRUCT_NAME, VEC_SET_MOD_NAME, VEC_SET_STRUCT_NAME,
};

const COLLECTIONS_EQUALITY_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Sui as u8,
    LinterDiagnosticCode::CollectionEquality as u8,
    "possibly useless collections compare",
);

const COLLECTION_TYPES: &[(Symbol, AccountAddress, &str, &str)] = &[
    (SUI_ADDR_NAME, SUI_ADDR_VALUE, BAG_MOD_NAME, BAG_STRUCT_NAME),
    (
        SUI_ADDR_NAME,
        SUI_ADDR_VALUE,
        OBJECT_BAG_MOD_NAME,
        OBJECT_BAG_STRUCT_NAME,
    ),
    (
        SUI_ADDR_NAME,
        SUI_ADDR_VALUE,
        TABLE_MOD_NAME,
        TABLE_STRUCT_NAME,
    ),
    (
        SUI_ADDR_NAME,
        SUI_ADDR_VALUE,
        OBJECT_TABLE_MOD_NAME,
        OBJECT_TABLE_STRUCT_NAME,
    ),
    (
        SUI_ADDR_NAME,
        SUI_ADDR_VALUE,
        LINKED_TABLE_MOD_NAME,
        LINKED_TABLE_STRUCT_NAME,
    ),
    (
        SUI_ADDR_NAME,
        SUI_ADDR_VALUE,
        TABLE_VEC_MOD_NAME,
        TABLE_VEC_STRUCT_NAME,
    ),
    (
        SUI_ADDR_NAME,
        SUI_ADDR_VALUE,
        VEC_MAP_MOD_NAME,
        VEC_MAP_STRUCT_NAME,
    ),
    (
        SUI_ADDR_NAME,
        SUI_ADDR_VALUE,
        VEC_SET_MOD_NAME,
        VEC_SET_STRUCT_NAME,
    ),
];

simple_visitor!(
    CollectionEqualityVisitor,
    fn visit_exp_custom(&mut self, exp: &T::Exp) -> bool {
        use T::UnannotatedExp_ as E;
        if let E::BinopExp(_, op, t, _) = &exp.exp.value {
            if op.value != P::BinOp_::Eq && op.value != P::BinOp_::Neq {
                // not a comparison
                return false;
            }
            let Some(sp!(_, tn_)) = t.value.unfold_to_type_name() else {
                // no type name
                return false;
            };
            if let Some((caddr_name, _, cmodule, cname)) = COLLECTION_TYPES
                .iter()
                .find(|(_, caddr_value, cmodule, cname)| tn_.is(caddr_value, *cmodule, *cname))
            {
                let msg = format!(
                    "Comparing collections of type '{caddr_name}::{cmodule}::{cname}' \
                    may yield unexpected result."
                );
                let note_msg = format!(
                    "Equality for collections of type '{caddr_name}::{cmodule}::{cname}' \
                    IS NOT a structural check based on content"
                );
                let mut d = diag!(COLLECTIONS_EQUALITY_DIAG, (op.loc, msg),);
                d.add_note(note_msg);
                self.add_diag(d);
                return true;
            }
        }
        false
    }
);
