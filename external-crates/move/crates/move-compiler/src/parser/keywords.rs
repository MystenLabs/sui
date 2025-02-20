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

pub const CONTEXTUAL_KEYWORDS: &[&str] = &["address", "mut", "phantom", "Self", "entry", "macro"];

pub const PRIMITIVE_TYPES: &[&str] = &["u8", "u16", "u32", "u64", "u128", "u256", "bool", "vector"];

pub const BUILTINS: &[&str] = &["assert", "freeze"];
