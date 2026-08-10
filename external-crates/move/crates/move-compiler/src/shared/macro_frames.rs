// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_bytecode_source_map::source_map::MacroFrameKind;
use move_ir_types::location::Loc;
use std::sync::Arc;

/// Compiler mode under which per-function diagnostics describing macro frame
/// transitions are emitted (used by the `.macro_frames` compiler tests).
pub const MACRO_FRAMES_MODE: &str = "macro-frames";

/// Structural representation of a macro expansion frame. Parent chains are
/// encoded directly via `Arc` references instead of requiring a global lookup.
///
/// Note: the derived `PartialEq` compares chains *structurally*. Frame
/// *identity* — each expansion event is a distinct frame even when it looks
/// identical — is pointer-based; compare with [`expansion_color_eq`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroInfo {
    pub kind: MacroFrameKind,
    pub source_loc: Loc,
    pub call_loc: Loc,
    pub parent: Option<Arc<MacroInfo>>,
}

impl MacroInfo {
    /// Compact one-line rendering of the frame chain (outermost frame first)
    /// for debug output, e.g. `[MacroBody(apply), Lambda]`.
    pub fn debug_chain(&self) -> String {
        let mut kinds = vec![];
        let mut cur = Some(self);
        while let Some(info) = cur {
            let kind = match &info.kind {
                MacroFrameKind::MacroBody { function_name, .. } => {
                    format!("MacroBody({})", function_name)
                }
                MacroFrameKind::Lambda => "Lambda".to_string(),
                MacroFrameKind::Argument => "Argument".to_string(),
            };
            kinds.push(kind);
            cur = info.parent.as_deref();
        }
        kinds.reverse();
        format!("[{}]", kinds.join(", "))
    }
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
