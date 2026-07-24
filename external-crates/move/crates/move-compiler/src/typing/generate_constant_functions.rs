// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Synthesizes `public(package)` constant functions for constants that are used in the function
//! bodies of other modules. Constants live in the constant pool of their defining module, so a
//! cross-module use cannot be compiled as a constant load; instead it is lowered (during CFGIR
//! translation) to a call of a constant function, synthesized here in the defining module. The
//! functions are created on demand: a constant only referenced within its own module (or only in
//! other modules' constant definitions, which are resolved by constant folding) gets none. Since
//! `public(package)` functions are not part of the upgrade-compatibility surface, the generated
//! functions may appear and disappear across package versions as usage changes.
//!
//! The set of needed constant functions is recorded by `core::make_constant_type` as constant
//! references are type-checked (which covers macro-expanded code, since expansions are typed at
//! their call sites), alongside the friend records that make the generated `public(package)`
//! functions callable from the using modules.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use move_ir_types::location::*;
use move_symbol_pool::Symbol;

use crate::{
    diag,
    diagnostics::filter,
    expansion::ast::{ModuleIdent, Visibility},
    naming::ast::{self as N, UseFuns},
    parser::ast::{ConstantName, DocComment, FunctionName},
    shared::{CompilationEnv, unique_map::UniqueMap},
    typing::ast as T,
};

//**************************************************************************************************
// Entry
//**************************************************************************************************

pub fn program(
    env: &CompilationEnv,
    modules: &mut UniqueMap<ModuleIdent, T::ModuleDefinition>,
    needed: BTreeMap<(ModuleIdent, ConstantName), Loc>,
) {
    let mut by_module: BTreeMap<ModuleIdent, BTreeSet<ConstantName>> = BTreeMap::new();
    for ((mident, cname), loc) in needed {
        if modules.contains_key(&mident) {
            by_module.entry(mident).or_default().insert(cname);
        } else {
            // The constant's module is outside the current compilation (e.g. pre-compiled), so
            // no function can be generated for it
            let msg = format!(
                "Invalid access of '{}::{}'. Constants defined in modules outside of the \
                 current compilation cannot be accessed from other modules",
                mident, cname
            );
            env.diagnostic_reporter_at_top_level()
                .add_diag(diag!(TypeSafety::Visibility, (loc, msg)));
        }
    }
    for (mident, constants) in by_module {
        let mdef = modules.get_mut(&mident).unwrap();
        synthesize_constant_functions(mident, mdef, constants);
    }
}

//**************************************************************************************************
// Synthesis
//**************************************************************************************************

fn synthesize_constant_functions(
    mident: ModuleIdent,
    mdef: &mut T::ModuleDefinition,
    constants: BTreeSet<ConstantName>,
) {
    let next_index = mdef
        .functions
        .iter()
        .map(|(_, _, fdef)| fdef.index + 1)
        .max()
        .unwrap_or(0);
    for (i, cname) in constants.into_iter().enumerate() {
        // The name is guaranteed unique: name validation (expansion/name_validation.rs) rejects
        // user-defined module members starting with '_' in every member namespace, and constant
        // names are unique within the module. It is a valid bytecode identifier as long as the
        // constant name is one.
        let fn_symbol = Symbol::from(format!("_const_{}", cname));
        let cdef = mdef.constants.get_mut(&cname).unwrap();
        let loc = cdef.loc;
        let fname = FunctionName(sp(loc, fn_symbol));
        cdef.constant_fn_name = Some(fname);
        let signature = N::FunctionSignature {
            type_parameters: vec![],
            parameters: vec![],
            return_type: cdef.signature.clone(),
        };
        let body_exp = T::exp(
            cdef.signature.clone(),
            sp(loc, T::UnannotatedExp_::Constant(mident, cname)),
        );
        let seq_items = VecDeque::from([sp(loc, T::SequenceItem_::Seq(Box::new(body_exp)))]);
        let body = sp(loc, T::FunctionBody_::Defined((UseFuns::new(0), seq_items)));
        let fdef = T::Function {
            doc: DocComment::empty(),
            warning_filter: filter::empty_filter_scope(),
            index: next_index + i,
            attributes: UniqueMap::new(),
            loc,
            visibility: Visibility::Package(loc),
            compiled_visibility: Visibility::Package(loc),
            entry: None,
            macro_: None,
            signature,
            body,
        };
        mdef.functions
            .add(fname, fdef)
            .expect("ICE generated constant function name should be unique in the module");
    }
}
