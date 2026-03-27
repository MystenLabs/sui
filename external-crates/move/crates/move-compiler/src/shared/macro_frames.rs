// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::naming::ast::Color;
use move_bytecode_source_map::source_map::MacroFrameKind;
use move_ir_types::location::Loc;

/// Maps a macro expansion color to the debugger frame it represents.
/// One entry is created per macro body, lambda, or argument expansion
/// during macro expansion (typing phase) and later converted to
/// bytecode-level `MacroFrameInfoEntry` records during code generation.
#[derive(Debug, Clone)]
pub struct ColorFrameInfo {
    /// Unique color assigned to this expansion scope during recoloring.
    pub color: Color,
    /// Color of the enclosing expansion scope (0 for top-level macro calls).
    pub parent_color: Color,
    /// Whether this scope is a macro body, lambda invocation, or argument
    /// substitution.
    pub kind: MacroFrameKind,
    /// Source location of the expanded construct (macro body definition,
    /// lambda definition, or argument expression).
    pub source_loc: Loc,
    /// Source location of the call site that triggered this expansion.
    pub call_loc: Loc,
}
