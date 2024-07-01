//! This linter rule checks for structs with an `id` field of type `UID` without the `key` ability.
//! It generates a warning if a struct incorrectly models unique identifiers without required abilities.
use crate::{
    diag,
    diagnostics::codes::{custom, DiagnosticInfo, Severity},
    linters::MISSING_KEY_DIAG_CODE,
    naming::ast::{StructDefinition, StructFields, TypeName_, Type_},
    parser::ast::Ability_,
    shared::CompilationEnv,
    typing::{ast as T, visitor::TypingVisitor},
};
use move_ir_types::location::Loc;

use super::{LinterDiagnosticCategory, LINT_WARNING_PREFIX};

const MISSING_KEY_ABILITY_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagnosticCategory::Sui as u8,
    MISSING_KEY_DIAG_CODE,
    "Struct has an 'id' field of type 'UID' but is missing the 'key' ability.",
);

pub struct MissingKey;

impl TypingVisitor for MissingKey {
    fn visit(&mut self, env: &mut CompilationEnv, program: &mut T::Program) {
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
        StructFields::Defined(_, fields) => {
            if let Some((_, symbol, ftype)) = fields.iter().nth(0) {
                matches!(ftype.1.value, Type_::Apply(_, sp!(_, TypeName_::ModuleType(_, struct_name)), _) if struct_name.0.value ==  symbol!("UID"))
                    && *symbol == symbol!("id")
            } else {
                false
            }
        }
        StructFields::Native(_) => false,
    };

    let lacks_key_ability = !sdef.abilities.has_ability_(Ability_::Key);

    if has_id_field_of_type_uid && lacks_key_ability {
        let diag = diag!(
            MISSING_KEY_ABILITY_DIAG,
            (
                sloc,
                "Struct has an 'id' field of type 'UID' but is missing the 'key' ability."
            )
        );
        env.add_diag(diag);
    }
}
