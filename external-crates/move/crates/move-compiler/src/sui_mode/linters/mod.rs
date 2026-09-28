// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use crate::{
    cfgir::visitor::AbstractInterpreterVisitor,
    command_line::compiler::Visitor,
    diagnostics::{
        codes::{DiagnosticOrigin, DiagnosticsID},
        filter::FilterName,
    },
    expansion::ast as E,
    hlir::ast::{BaseType_, SingleType, SingleType_},
    linters::{LintLevel, filters_from_table, lints},
    shared::known_attributes::DiagnosticAttribute,
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

// Append-only: codes are positional and published (see `lints!`).
lints!(
    SuiLintCode,
    DiagnosticOrigin::SuiLint,
    SUI_LINT_WARNING_FILTERS,
    (
        ShareOwned,
        Suspicious,
        "share_owned",
        "possible owned object share"
    ),
    (
        SelfTransfer,
        Conventions,
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
        Conventions,
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
        Security,
        "public_random",
        "risky use of 'sui::random'"
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
        Conventions,
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
    (
        Some(DiagnosticAttribute::LINT_SYMBOL),
        filters_from_table(SuiLintCode::ORIGIN, SUI_LINT_WARNING_FILTERS),
    )
}

pub fn linter_visitors(level: LintLevel) -> Vec<Visitor> {
    linter_visitors_with_config(level, &BTreeSet::new())
}

pub fn linter_visitors_with_config(
    level: LintLevel,
    configured: &BTreeSet<FilterName>,
) -> Vec<Visitor> {
    let all = match level {
        LintLevel::None => return vec![],
        LintLevel::Default => {
            configured.contains(&Symbol::from(crate::diagnostics::filter::FILTER_ALL))
        }
        LintLevel::All => true,
    };

    let mut visitors = vec![
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
        crate::linters::unused_return_value::UnusedReturnValue.visitor(),
    ];
    if all || configured.contains(&Symbol::from("freezing_capability")) {
        visitors.push(freezing_capability::WarnFreezeCapability.visitor());
    }
    if all || configured.contains(&Symbol::from("prefer_mut_tx_context")) {
        visitors.push(public_mut_tx_context::PreferMutableTxContext.visitor());
    }
    visitors
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

    #[test]
    fn configured_non_default_filters_enable_their_visitors() {
        let default_count = linter_visitors(LintLevel::Default).len();
        for name in ["freezing_capability", "prefer_mut_tx_context"] {
            let configured = BTreeSet::from([Symbol::from(name)]);
            assert_eq!(
                linter_visitors_with_config(LintLevel::Default, &configured).len(),
                default_count + 1,
                "{name}"
            );
        }
    }

    #[test]
    fn configured_all_enables_all_visitors() {
        let configured = BTreeSet::from([Symbol::from(crate::diagnostics::filter::FILTER_ALL)]);
        assert_eq!(
            linter_visitors_with_config(LintLevel::Default, &configured).len(),
            linter_visitors(LintLevel::All).len()
        );
    }
}
