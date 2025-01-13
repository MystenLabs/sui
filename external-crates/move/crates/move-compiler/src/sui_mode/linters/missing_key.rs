// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This linter rule checks for structs with an `id` field of type `UID` without the `key` ability.

use super::{LinterDiagnosticCategory, LinterDiagnosticCode, LINT_WARNING_PREFIX};
use crate::expansion::ast::ModuleIdent;
use crate::parser::ast::DatatypeName;
use crate::{
    diag,
    diagnostics::codes::{custom, DiagnosticInfo, Severity},
    naming::ast::{StructDefinition, StructFields},
    parser::ast::Ability_,
    sui_mode::{ID_FIELD_NAME, OBJECT_MODULE_NAME, SUI_ADDR_VALUE, UID_TYPE_NAME},
    typing::visitor::simple_visitor,
};

const MISSING_KEY_ABILITY_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Sui as u8,
    LinterDiagnosticCode::MissingKey as u8,
    "struct with id but missing key ability",
);

simple_visitor!(
    MissingKeyVisitor,
    fn visit_struct_custom(
        &mut self,
        _module: ModuleIdent,
        _struct_name: DatatypeName,
        sdef: &StructDefinition,
    ) -> bool {
        if first_field_has_id_field_of_type_uid(sdef) && lacks_key_ability(sdef) {
            let uid_msg =
                "Struct's first field has an 'id' field of type 'sui::object::UID' but is missing the 'key' ability.";
            let diagnostic = diag!(MISSING_KEY_ABILITY_DIAG, (sdef.loc, uid_msg));
            self.add_diag(diagnostic);
        }
        false
    }
);

fn first_field_has_id_field_of_type_uid(sdef: &StructDefinition) -> bool {
    match &sdef.fields {
        StructFields::Defined(_, fields) => fields.iter().any(|(_, symbol, (idx, (_, ty)))| {
            *idx == 0
                && symbol == &ID_FIELD_NAME
                && ty
                    .value
                    .is(&SUI_ADDR_VALUE, OBJECT_MODULE_NAME, UID_TYPE_NAME)
        }),
        StructFields::Native(_) => false,
    }
}

fn lacks_key_ability(sdef: &StructDefinition) -> bool {
    !sdef.abilities.has_ability_(Ability_::Key)
}
