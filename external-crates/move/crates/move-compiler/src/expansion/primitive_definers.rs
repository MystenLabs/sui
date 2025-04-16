// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    diagnostics::DiagnosticReporter,
    expansion::ast::{ModuleDefinition, ModuleIdent},
    ice,
    naming::ast::BuiltinTypeName_,
    shared::{
        known_attributes::{AttributeKind_, DefinesPrimitiveAttribute, KnownAttribute},
        unique_map::UniqueMap,
        CompilationEnv,
    },
    FullyCompiledProgram,
};
use std::{collections::BTreeMap, sync::Arc};

/// Gather primitive defines from module declarations, erroring on duplicates for a given base
/// type or for unknown base types.
pub fn modules(
    env: &CompilationEnv,
    pre_compiled_lib_opt: Option<Arc<FullyCompiledProgram>>,
    modules: &UniqueMap<ModuleIdent, ModuleDefinition>,
) {
    let reporter = env.diagnostic_reporter_at_top_level();
    let mut definers = BTreeMap::new();
    for (mident, m) in modules.key_cloned_iter() {
        check_prim_definer(
            &reporter,
            /* allow shadowing */ false,
            &mut definers,
            mident,
            m,
        )
    }
    if let Some(pre_compiled_lib) = pre_compiled_lib_opt {
        for (mident, m) in pre_compiled_lib.expansion.modules.key_cloned_iter() {
            check_prim_definer(
                &reporter,
                /* allow shadowing */ true,
                &mut definers,
                mident,
                m,
            )
        }
    }
    env.set_primitive_type_definers(definers)
}

fn check_prim_definer(
    reporter: &DiagnosticReporter,
    allow_shadowing: bool,
    definers: &mut BTreeMap<BuiltinTypeName_, crate::expansion::ast::ModuleIdent>,
    mident: ModuleIdent,
    m: &ModuleDefinition,
) {
    let defines_prim_attr = m.attributes.get_(&AttributeKind_::DefinesPrimitive);
    let Some(sp!(attr_loc, attr_)) = defines_prim_attr else {
        return;
    };
    let KnownAttribute::DefinesPrimitive(DefinesPrimitiveAttribute { name }) = attr_ else {
        reporter.add_diag(ice!((
            *attr_loc,
            "Expected a primitive definer attribute for the provided kind tag"
        )));
        return;
    };
    let Some(prim) = BuiltinTypeName_::resolve(name.value.as_str()) else {
        let msg = format!(
            "Invalid parameterization of '{}'. Unknown primitive type '{}'",
            DefinesPrimitiveAttribute::DEFINES_PRIM,
            name,
        );
        reporter.add_diag(diag!(Attributes::InvalidUsage, (name.loc, msg)));
        return;
    };

    if let Some(prev) = definers.get(&prim) {
        if !allow_shadowing {
            let msg = format!("Duplicate definer annotated for primitive type '{}'", prim);
            reporter.add_diag(diag!(
                Attributes::InvalidUsage,
                (*attr_loc, msg),
                (prev.loc, "Previously declared here")
            ));
        }
    } else {
        definers.insert(prim, mident);
    }
}
