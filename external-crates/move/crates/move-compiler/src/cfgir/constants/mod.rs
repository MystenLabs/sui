// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Constant compilation phases:
//! - `folding::modules` folds every module's constant definition to a value, in the order of the
//!   constants' own dependency graph, and hands back the values.
//! - `cross_module_gen::module` rewrites each module's cross-module constant uses into uses of
//!   module-local copies synthesized from those values

pub(super) mod cross_module_gen;
pub(super) mod folding;

use crate::{
    diag, diagnostics::Diagnostic, expansion::ast::ModuleIdent, hlir::ast as H,
    parser::ast::ConstantName,
};

use move_ir_types::location::*;

use std::collections::BTreeMap;

//**************************************************************************************************
// Types
//**************************************************************************************************

type ConstantId = (ModuleIdent, ConstantName);

type ConstantValues = BTreeMap<ConstantId, H::Value>;

/// The fold state of a constant definition
enum ConstantEntry {
    /// Constant that failed to fold (due to error, etc)
    Failed,
    /// A precompiled constant with no usable value
    PrecompiledFailed,
    /// Folded; the value is in the shared constant value map
    Defined {
        loc: Loc,
        signature: Box<H::BaseType>,
    },
}

/// The program's constant definitions, folded to values.
#[derive(Default)]
pub(crate) struct Constants {
    defs: BTreeMap<ConstantId, ConstantEntry>,
    values: ConstantValues,
}

//**************************************************************************************************
// Errors
//**************************************************************************************************

/// The error reported at each use of a pre-compiled constant whose own compilation could not
/// evaluate it to a value; the use site is the only place this compilation can put the error
fn unfoldable_constant_use_error(
    m: &ModuleIdent,
    c: &ConstantName,
    use_loc: Loc,
    defined_loc: Loc,
) -> Diagnostic {
    let msg = format!("Constant '{}::{}' could not be evaluated to a value", m, c);
    let mut diag = diag!(CodeGeneration::UnfoldableConstant, (use_loc, msg));
    diag.add_secondary_label((defined_loc, format!("'{}' is defined here", c)));
    diag
}
