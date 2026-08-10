// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Syntactic context describing the chain of expansions that produced a piece
//! of code. Built during HLIR lowering, with one node per expansion, and
//! carried on HLIR expressions and CFGIR commands via [`SyntaxSpanned`].

use crate::shared::{ast_debug::*, macro_expansion::MacroExpansionInfo};
use move_ir_types::location::Loc;
use std::sync::Arc;

/// Syntactic context for code that was produced by an expansion (e.g., a macro), tracking the
/// chain of expansions that produced it, innermost first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxInfo {
    pub info: SyntaxInfoEntry,
    pub prev: Option<Arc<SyntaxInfo>>,
}

/// A single kind of syntactic context. An enum so that other kinds of
/// compiler-tracked context (beyond macro expansion) can be added over time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyntaxInfoEntry {
    MacroExpansion(MacroExpansionInfo),
}

/// The full syntactic context of a program point: the chain of expansions it
/// belongs to, or `None` for plain user code.
pub type SyntaxContext = Option<Arc<SyntaxInfo>>;

/// A location plus the expansion context (if any) the code at that location came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxLoc {
    pub loc: Loc,
    pub syntax_info: SyntaxContext,
}

/// Like `Spanned<T>`, but carrying a `SyntaxLoc` instead of a bare `Loc`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxSpanned<T> {
    pub sloc: SyntaxLoc,
    pub value: T,
}

/// Tuple-like constructor for [`SyntaxSpanned`], mirroring `sp`.
pub fn ssp<T>(loc: Loc, syntax_info: SyntaxContext, value: T) -> SyntaxSpanned<T> {
    SyntaxSpanned {
        sloc: SyntaxLoc { loc, syntax_info },
        value,
    }
}

/// Pattern-matching macro for [`SyntaxSpanned`], mirroring `sp!`.
/// Two-argument forms match `(sloc, value)`; three-argument forms match
/// `(loc, syntax_info, value)` by looking through [`SyntaxLoc`].
/// (In a nested module so its name can coexist with the `ssp` constructor fn.)
mod ssp_macro {
    macro_rules! ssp {
        (_, $value:pat) => {
            $crate::shared::syntax_info::SyntaxSpanned { value: $value, .. }
        };
        ($sloc:pat, _) => {
            $crate::shared::syntax_info::SyntaxSpanned { sloc: $sloc, .. }
        };
        ($sloc:pat, $value:pat) => {
            $crate::shared::syntax_info::SyntaxSpanned {
                sloc: $sloc,
                value: $value,
            }
        };
        (_, _, $value:pat) => {
            $crate::shared::syntax_info::SyntaxSpanned { value: $value, .. }
        };
        ($loc:pat, _, $value:pat) => {
            $crate::shared::syntax_info::SyntaxSpanned {
                sloc: $crate::shared::syntax_info::SyntaxLoc { loc: $loc, .. },
                value: $value,
            }
        };
        ($loc:pat, $info:pat, $value:pat) => {
            $crate::shared::syntax_info::SyntaxSpanned {
                sloc: $crate::shared::syntax_info::SyntaxLoc {
                    loc: $loc,
                    syntax_info: $info,
                },
                value: $value,
            }
        };
    }
    pub(crate) use ssp;
}
pub(crate) use ssp_macro::ssp;

impl SyntaxInfo {
    pub fn new(info: SyntaxInfoEntry, prev: Option<Arc<SyntaxInfo>>) -> Self {
        Self { info, prev }
    }

    /// Compact one-line rendering of the expansion chain (outermost first)
    /// for debug output, e.g. `[0x1::m::foo, lambda]`.
    pub fn debug_chain(&self) -> String {
        let mut kinds = vec![];
        let mut cur = Some(self);
        while let Some(info) = cur {
            let SyntaxInfoEntry::MacroExpansion(mei) = &info.info;
            kinds.push(mei.kind.debug_name());
            cur = info.prev.as_deref();
        }
        kinds.reverse();
        format!("[{}]", kinds.join(", "))
    }
}

impl SyntaxLoc {
    pub fn new(loc: Loc, syntax_info: SyntaxContext) -> Self {
        Self { loc, syntax_info }
    }
}

impl<T> SyntaxSpanned<T> {
    pub fn new(sloc: SyntaxLoc, value: T) -> Self {
        Self { sloc, value }
    }

    pub fn loc(&self) -> Loc {
        self.sloc.loc
    }
}

impl<T: AstDebug> AstDebug for SyntaxSpanned<T> {
    fn ast_debug(&self, w: &mut AstWriter) {
        self.value.ast_debug(w)
    }
}
