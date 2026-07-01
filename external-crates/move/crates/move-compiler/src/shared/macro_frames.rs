// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_bytecode_source_map::source_map::MacroFrameKind;
use move_ir_types::location::Loc;
use std::sync::Arc;

/// Structural representation of a macro expansion frame. Parent chains are
/// encoded directly via `Arc` references instead of requiring a global lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroInfo {
    pub kind: MacroFrameKind,
    pub source_loc: Loc,
    pub call_loc: Loc,
    pub parent: Option<Arc<MacroInfo>>,
}

/// Type alias for macro expansion color used for debugger frame tracking.
/// `None` = no macro scope (regular code). `Some(...)` = active macro frame.
/// Distinct from `Color` (u16) which is used for scope resolution.
pub type ExpansionColor = Option<Arc<MacroInfo>>;

/// Compare two `ExpansionColor` values by `Arc` pointer identity.
pub fn expansion_color_eq(a: &ExpansionColor, b: &ExpansionColor) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}
