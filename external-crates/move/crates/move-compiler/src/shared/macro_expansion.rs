// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{expansion::ast::ModuleIdent, parser::ast::FunctionName};
use move_ir_types::location::Loc;

/// The kind of expansion that produced a region of code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MacroExpansionKind {
    /// Expansion of a macro function body.
    MacroBody {
        module: ModuleIdent,
        function: FunctionName,
    },
    /// Expansion of a lambda invocation inside a macro body.
    Lambda,
    /// Evaluation of a by-name argument substituted into a macro body.
    Argument,
}

/// Describes one macro expansion boundary: what kind of expansion it is,
/// where it was triggered, and where the expanded code came from.
/// Created during macro expansion in typing and carried through the
/// naming and typing ASTs. HLIR lowering uses it to build the syntax
/// contexts attached to expressions and CFGIR commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacroExpansionInfo {
    pub kind: MacroExpansionKind,
    /// Location of the site that triggered this expansion:
    /// - `MacroBody`: the `macro_name!(args)` invocation.
    /// - `Lambda`: the `$f(args)` call inside the macro body.
    /// - `Argument`: the `$x` reference inside the macro body.
    pub invocation_location: Loc,
    /// Location of the expanded construct:
    /// - `MacroBody`: the macro function definition.
    /// - `Lambda`: the lambda expression at the call site.
    /// - `Argument`: the argument expression at the call site.
    pub expansion_location: Loc,
}

impl MacroExpansionKind {
    /// Short human-readable rendering for debug output.
    pub fn debug_name(&self) -> String {
        match self {
            Self::MacroBody { module, function } => format!("{}::{}", module, function),
            Self::Lambda => "lambda".to_string(),
            Self::Argument => "argument".to_string(),
        }
    }
}
