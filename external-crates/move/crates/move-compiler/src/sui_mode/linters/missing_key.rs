// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This linter rule checks for structs with an `id` field of type `UID` without the `key` ability.

use super::{LinterDiagnosticCategory, LinterDiagnosticCode, LINT_WARNING_PREFIX};
use crate::expansion::ast::ModuleIdent;
use crate::parser::ast::DatatypeName;
use crate::{
    diag,
    diagnostics::{
        codes::{custom, DiagnosticInfo, Severity},
        WarningFilters,
    },
    naming::ast::{StructDefinition, StructFields},
    parser::ast::Ability_,
    shared::CompilationEnv,
    typing::{
        ast as T,
        visitor::{TypingVisitorConstructor, TypingVisitorContext},
    },
};

const MISSING_KEY_ABILITY_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Sui as u8,
    LinterDiagnosticCode::MissingKey as u8,
    "The struct's first field is 'id' of type 'sui::object::UID' but is missing the 'key' ability.",
);

pub struct MissingKeyVisitor;

pub struct Context<'a> {
    env: &'a mut CompilationEnv,
}
impl TypingVisitorConstructor for MissingKeyVisitor {
    type Context<'a> = Context<'a>;

    fn context<'a>(env: &'a mut CompilationEnv, _program: &T::Program) -> Self::Context<'a> {
        Context { env }
    }
}

impl TypingVisitorContext for Context<'_> {
    fn add_warning_filter_scope(&mut self, filter: WarningFilters) {
        self.env.add_warning_filter_scope(filter)
    }

    fn pop_warning_filter_scope(&mut self) {
        self.env.pop_warning_filter_scope()
    }

    const VISIT_TYPES: bool = true;

    fn visit_struct_custom(
        &mut self,
        _module: ModuleIdent,
        _struct_name: DatatypeName,
        sdef: &mut StructDefinition,
    ) -> bool {
        if first_field_has_id_field_of_type_uid(sdef) && lacks_key_ability(sdef) {
            let uid_msg =
                "Struct's first field has an 'id' field of type 'sui::object::UID' but is missing the 'key' ability.";
            let diagnostic = diag!(MISSING_KEY_ABILITY_DIAG, (sdef.loc, uid_msg));
            self.env.add_diag(diagnostic);
        }
        false
    }
}

fn first_field_has_id_field_of_type_uid(sdef: &StructDefinition) -> bool {
    matches!(&sdef.fields, StructFields::Defined(_, fields) if fields.iter().any(|(_, symbol, ftype)| {
        ftype.0 == 0 && symbol == &symbol!("id") && ftype.1.value.is("sui", "object", "UID")
    }))
}

fn lacks_key_ability(sdef: &StructDefinition) -> bool {
    !sdef.abilities.has_ability_(Ability_::Key)
}
