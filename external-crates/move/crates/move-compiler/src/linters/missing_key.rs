//! This linter rule checks for structs with an `id` field of type `UID` without the `key` ability.
//! It generates a warning if a struct incorrectly models unique identifiers without required abilities.
use crate::{
    diag,
    diagnostics::codes::{custom, DiagnosticInfo, Severity},
    naming::ast::{StructDefinition, StructFields},
    parser::ast::Ability_,
    shared::{program_info::TypingProgramInfo, CompilationEnv},
    typing::{
        ast::{self as T},
        visitor::TypingVisitor,
    },
};
use move_ir_types::location::{Loc, Spanned};

use super::{LinterDiagCategory, LINTER_DEFAULT_DIAG_CODE, LINT_WARNING_PREFIX};

const MISSING_KEY_ABILITY_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagCategory::MissingKey as u8,
    LINTER_DEFAULT_DIAG_CODE,
    "Struct has an 'id' field of type 'UID' but is missing the 'key' ability.",
);

pub struct MissingKey;

impl TypingVisitor for MissingKey {
    fn visit(
        &mut self,
        env: &mut CompilationEnv,
        _program_info: &TypingProgramInfo,
        program: &mut T::Program_,
    ) {
        for (_, _, mdef) in program.modules.iter() {
            if mdef.attributes.is_test_or_test_only() {
                continue;
            }
            mdef.structs
                .iter()
                .filter(|(_, _, sdef)| !sdef.attributes.is_test_or_test_only())
                .for_each(|(sloc, _, sdef)| check_key_abilities(env, sdef, sloc));
        }
    }
}

fn check_key_abilities(env: &mut CompilationEnv, sdef: &StructDefinition, sloc: Loc) {
    let has_id_field_of_type_uid = match &sdef.fields {
        StructFields::Defined(fields) => {
            fields.iter().any(
                |(_, symbol, ftype)| {
                    if symbol.as_str() == "id" {
                        true
                    } else {
                        false
                    }
                },
            )
        }
        StructFields::Native(_) => false,
    };

    let lacks_key_ability = !sdef
        .abilities
        .has_ability(&Spanned::new(sloc, Ability_::Key));

    if has_id_field_of_type_uid && lacks_key_ability {
        report_missing_key_ability(env, sloc);
    }
}

fn report_missing_key_ability(env: &mut CompilationEnv, loc: Loc) {
    let diag = diag!(
        MISSING_KEY_ABILITY_DIAG,
        (
            loc,
            "Struct has an 'id' field of type 'UID' but is missing the 'key' ability."
        )
    );
    env.add_diag(diag);
}
