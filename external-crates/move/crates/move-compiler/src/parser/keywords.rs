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
    crate::shared::builtin_type_names::ADDRESS,
    "mut",
    "phantom",
    "Self",
    "entry",
    "macro",
];

pub use crate::shared::builtin_type_names::PRIMITIVE_TYPES;

pub const BUILTINS: &[&str] = &["assert", "freeze"];
