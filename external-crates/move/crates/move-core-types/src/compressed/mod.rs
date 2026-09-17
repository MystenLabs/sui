// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod annotated;
pub mod backend;
pub mod runtime;

// =============================================================================
// Shared types used by both runtime and annotated compressed layouts
// =============================================================================

/// Tag identifying an enum variant. This is a type alias for `u16` — the
/// canonical `VariantTag` lives in `move-binary-format` but cannot be
/// referenced from here (circular dependency), so we define a local alias.
pub type VariantTag = u16;
