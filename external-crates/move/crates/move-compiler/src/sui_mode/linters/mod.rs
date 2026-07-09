// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfgir::visitor::AbstractInterpreterVisitor,
    command_line::compiler::Visitor,
    diagnostics::{codes::DiagnosticsID, filter::FilterName},
    editions::Flavor,
    expansion::ast as E,
    hlir::ast::{BaseType_, SingleType, SingleType_},
    linters::{ALLOW_ATTR_CATEGORY, LintLevel, filters_from_table, lints},
    typing::visitor::TypingVisitor,
};
use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;

pub mod coin_field;
pub mod collection_equality;
pub mod custom_state_change;
pub mod freeze_wrapped;
pub mod freezing_capability;
pub mod missing_key;
pub mod public_mut_tx_context;
pub mod public_random;
pub mod self_transfer;
pub mod share_owned;
pub mod uncallable_function;
pub mod unnecessary_public_entry;
pub mod unused_object_with_fields;

pub const TRANSFER_MOD_NAME: &str = "transfer";
pub const TRANSFER_FUN: &str = "transfer";
pub const PUBLIC_TRANSFER_FUN: &str = "public_transfer";
pub const SHARE_FUN: &str = "share_object";
pub const PUBLIC_SHARE_FUN: &str = "public_share_object";
pub const FREEZE_FUN: &str = "freeze_object";
pub const PUBLIC_FREEZE_FUN: &str = "public_freeze_object";
pub const RECEIVE_FUN: &str = "receive";
pub const PUBLIC_RECEIVE_FUN: &str = "public_receive";

pub const COIN_MOD_NAME: &str = "coin";
pub const COIN_STRUCT_NAME: &str = "Coin";

pub const BAG_MOD_NAME: &str = "bag";
pub const BAG_STRUCT_NAME: &str = "Bag";

pub const OBJECT_BAG_MOD_NAME: &str = "object_bag";
pub const OBJECT_BAG_STRUCT_NAME: &str = "ObjectBag";

pub const TABLE_MOD_NAME: &str = "table";
pub const TABLE_STRUCT_NAME: &str = "Table";

pub const OBJECT_TABLE_MOD_NAME: &str = "object_table";
pub const OBJECT_TABLE_STRUCT_NAME: &str = "ObjectTable";

pub const LINKED_TABLE_MOD_NAME: &str = "linked_table";
pub const LINKED_TABLE_STRUCT_NAME: &str = "LinkedTable";

pub const TABLE_VEC_MOD_NAME: &str = "table_vec";
pub const TABLE_VEC_STRUCT_NAME: &str = "TableVec";

pub const VEC_MAP_MOD_NAME: &str = "vec_map";
pub const VEC_MAP_STRUCT_NAME: &str = "VecMap";

pub const VEC_SET_MOD_NAME: &str = "vec_set";
pub const VEC_SET_STRUCT_NAME: &str = "VecSet";

pub const RANDOM_MOD_NAME: &str = "random";
pub const RANDOM_STRUCT_NAME: &str = "Random";
pub const RANDOM_GENERATOR_STRUCT_NAME: &str = "RandomGenerator";

pub const INVALID_LOC: Loc = Loc::invalid();

// Sui is a lint source, not a category: Sui lints share the categories in
// `LinterDiagnosticCategory` within the flavor's tens block (`Flavor::lint_category_marker`) and
// render as `Lint W9XNNN`, where 9 marks the Sui flavor and X is the shared category.
//
// Append-only: a lint's code is its index in this table, and rendered codes are a published
// compatibility surface (see `lints!`).
lints!(
    SuiLintCode,
    Flavor::Sui.lint_category_marker(),
    SUI_LINT_WARNING_FILTERS,
    (
        ShareOwned,
        Suspicious,
        "share_owned",
        "possible owned object share"
    ),
    (
        SelfTransfer,
        Style,
        "self_transfer",
        "non-composable transfer to sender"
    ),
    (
        CustomStateChange,
        Suspicious,
        "custom_state_change",
        "potentially unenforceable custom transfer/share/freeze policy"
    ),
    (
        CoinField,
        Style,
        "coin_field",
        "sub-optimal 'sui::coin::Coin' field type"
    ),
    (
        FreezeWrapped,
        Suspicious,
        "freeze_wrapped",
        "attempting to freeze wrapped objects"
    ),
    (
        CollectionEquality,
        Suspicious,
        "collection_equality",
        "possibly useless collections compare"
    ),
    (
        PublicRandom,
        Suspicious,
        "public_random",
        "Risky use of 'sui::random'"
    ),
    (
        MissingKey,
        Suspicious,
        "missing_key",
        "struct with id but missing key ability"
    ),
    (
        FreezingCapability,
        Suspicious,
        "freezing_capability",
        "freezing potential capability"
    ),
    (
        PreferMutableTxContext,
        Style,
        "prefer_mut_tx_context",
        "prefer '&mut TxContext' over '&TxContext'"
    ),
    (
        UnnecessaryPublicEntry,
        Complexity,
        "public_entry",
        "unnecessary `entry` on a `public` function"
    ),
    (
        UncallableFunction,
        Correctness,
        "uncallable_function",
        "it will not be possible to call this function"
    ),
    (
        UnusedObjWithFields,
        Suspicious,
        "unused_object_with_fields",
        "unused object with fields"
    ),
);

pub fn known_filters() -> (Option<Symbol>, Vec<(FilterName, Vec<DiagnosticsID>)>) {
    // `lint(all)` is registered by the core linter (`linters::known_filters`) as a wildcard over
    // the whole `LINT_WARNING_PREFIX`, which covers the Sui lints too; don't register it again
    // here or `filter_from_str` returns duplicate ids.
    (
        Some(ALLOW_ATTR_CATEGORY.into()),
        filters_from_table(SUI_LINT_WARNING_FILTERS),
    )
}

pub fn linter_visitors(level: LintLevel) -> Vec<Visitor> {
    match level {
        LintLevel::None => vec![],
        LintLevel::Default => vec![
            share_owned::ShareOwnedVerifier.visitor(),
            self_transfer::SelfTransferVerifier.visitor(),
            custom_state_change::CustomStateChangeVerifier.visitor(),
            coin_field::CoinFieldVisitor.visitor(),
            freeze_wrapped::FreezeWrappedVisitor.visitor(),
            collection_equality::CollectionEqualityVisitor.visitor(),
            public_random::PublicRandomVisitor.visitor(),
            missing_key::MissingKeyVisitor.visitor(),
            unnecessary_public_entry::UnnecessaryPublicEntry.visitor(),
            uncallable_function::UncallableFunction.visitor(),
            unused_object_with_fields::UnusedObjWithFieldsVerifier.visitor(),
            // This is not on by default outside of Sui mode
            crate::linters::unused_return_value::UnusedReturnValue.visitor(),
        ],
        LintLevel::All => {
            let mut visitors = linter_visitors(LintLevel::Default);
            visitors.extend([
                freezing_capability::WarnFreezeCapability.visitor(),
                public_mut_tx_context::PreferMutableTxContext.visitor(),
            ]);
            visitors
        }
    }
}

/// Returns abilities of a given type, if any.
pub fn type_abilities(sp!(_, st_): &SingleType) -> Option<E::AbilitySet> {
    let sp!(_, bt_) = match st_ {
        SingleType_::Base(v) => v,
        SingleType_::Ref(_, v) => v,
    };
    if let BaseType_::Apply(abilities, _, _) = bt_ {
        return Some(abilities.clone());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // Rendered lint codes are a published compatibility surface. If this test fails, an edit to
    // the `lints!` table renumbered existing lints — append new entries at the end instead of
    // reordering or inserting, and never change an existing entry's category.
    #[test]
    fn sui_lint_code_assignments_are_stable() {
        let expected: &[(u8, u8, &str)] = &[
            (92, 1, "share_owned"),
            (94, 2, "self_transfer"),
            (92, 3, "custom_state_change"),
            (94, 4, "coin_field"),
            (92, 5, "freeze_wrapped"),
            (92, 6, "collection_equality"),
            (92, 7, "public_random"),
            (92, 8, "missing_key"),
            (92, 9, "freezing_capability"),
            (94, 10, "prefer_mut_tx_context"),
            (91, 11, "public_entry"),
            (90, 12, "uncallable_function"),
            (92, 13, "unused_object_with_fields"),
        ];
        assert_eq!(SUI_LINT_WARNING_FILTERS, expected);
    }
}
