// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub const KEYWORDS: &[&str] = &[
    "abort",
    "acquires",
    "as",
    "break",
    "const",
    "continue",
    "copy",
    "else",
    "false",
    "friend",
    "fun",
    "has",
    "if",
    "invariant",
    "let",
    "loop",
    "module",
    "move",
    "native",
    "public",
    "return",
    "spec",
    "struct",
    "true",
    "use",
    "while",
    "enum",
    "for",
];

pub const CONTEXTUAL_KEYWORDS: &[&str] = &[
    crate::shared::builtin_types::ADDRESS,
    "mut",
    "phantom",
    "Self",
    "entry",
    "macro",
];

/// Re-export of all primitive type names. Not feature-gated; all types (including signed integers)
/// are included regardless of edition so that the parser can always recognize them as keywords.
pub use crate::shared::builtin_types::PRIMITIVE_TYPES;

pub const BUILTINS: &[&str] = &["assert", "freeze"];
