// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfgir::visitor::AbstractInterpreterVisitor,
    command_line::compiler::Visitor,
    diagnostics::warning_filters::WarningFilter,
    expansion::ast as E,
    hlir::ast::{BaseType_, SingleType, SingleType_},
    linters::{LintLevel, LinterDiagnosticCategory, ALLOW_ATTR_CATEGORY, LINT_WARNING_PREFIX},
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

pub const SHARE_OWNED_FILTER_NAME: &str = "share_owned";
pub const SELF_TRANSFER_FILTER_NAME: &str = "self_transfer";
pub const CUSTOM_STATE_CHANGE_FILTER_NAME: &str = "custom_state_change";
pub const COIN_FIELD_FILTER_NAME: &str = "coin_field";
pub const FREEZE_WRAPPED_FILTER_NAME: &str = "freeze_wrapped";
pub const COLLECTION_EQUALITY_FILTER_NAME: &str = "collection_equality";
pub const PUBLIC_RANDOM_FILTER_NAME: &str = "public_random";
pub const MISSING_KEY_FILTER_NAME: &str = "missing_key";
pub const FREEZING_CAPABILITY_FILTER_NAME: &str = "freezing_capability";
pub const PREFER_MUTABLE_TX_CONTEXT_FILTER_NAME: &str = "prefer_mut_tx_context";

pub const RANDOM_MOD_NAME: &str = "random";
pub const RANDOM_STRUCT_NAME: &str = "Random";
pub const RANDOM_GENERATOR_STRUCT_NAME: &str = "RandomGenerator";

pub const INVALID_LOC: Loc = Loc::invalid();

#[repr(u8)]
pub enum LinterDiagnosticCode {
    ShareOwned,
    SelfTransfer,
    CustomStateChange,
    CoinField,
    FreezeWrapped,
    CollectionEquality,
    PublicRandom,
    MissingKey,
    FreezingCapability,
    PreferMutableTxContext,
}

pub fn known_filters() -> (Option<Symbol>, Vec<WarningFilter>) {
    let filters = vec![
        WarningFilter::All(Some(LINT_WARNING_PREFIX)),
        WarningFilter::code(
            Some(LINT_WARNING_PREFIX),
            LinterDiagnosticCategory::Sui as u8,
            LinterDiagnosticCode::ShareOwned as u8,
            Some(SHARE_OWNED_FILTER_NAME),
        ),
        WarningFilter::code(
            Some(LINT_WARNING_PREFIX),
            LinterDiagnosticCategory::Sui as u8,
            LinterDiagnosticCode::SelfTransfer as u8,
            Some(SELF_TRANSFER_FILTER_NAME),
        ),
        WarningFilter::code(
            Some(LINT_WARNING_PREFIX),
            LinterDiagnosticCategory::Sui as u8,
            LinterDiagnosticCode::CustomStateChange as u8,
            Some(CUSTOM_STATE_CHANGE_FILTER_NAME),
        ),
        WarningFilter::code(
            Some(LINT_WARNING_PREFIX),
            LinterDiagnosticCategory::Sui as u8,
            LinterDiagnosticCode::CoinField as u8,
            Some(COIN_FIELD_FILTER_NAME),
        ),
        WarningFilter::code(
            Some(LINT_WARNING_PREFIX),
            LinterDiagnosticCategory::Sui as u8,
            LinterDiagnosticCode::FreezeWrapped as u8,
            Some(FREEZE_WRAPPED_FILTER_NAME),
        ),
        WarningFilter::code(
            Some(LINT_WARNING_PREFIX),
            LinterDiagnosticCategory::Sui as u8,
            LinterDiagnosticCode::CollectionEquality as u8,
            Some(COLLECTION_EQUALITY_FILTER_NAME),
        ),
        WarningFilter::code(
            Some(LINT_WARNING_PREFIX),
            LinterDiagnosticCategory::Sui as u8,
            LinterDiagnosticCode::PublicRandom as u8,
            Some(PUBLIC_RANDOM_FILTER_NAME),
        ),
        WarningFilter::code(
            Some(LINT_WARNING_PREFIX),
            LinterDiagnosticCategory::Sui as u8,
            LinterDiagnosticCode::MissingKey as u8,
            Some(MISSING_KEY_FILTER_NAME),
        ),
        WarningFilter::code(
            Some(LINT_WARNING_PREFIX),
            LinterDiagnosticCategory::Sui as u8,
            LinterDiagnosticCode::FreezingCapability as u8,
            Some(FREEZING_CAPABILITY_FILTER_NAME),
        ),
        WarningFilter::code(
            Some(LINT_WARNING_PREFIX),
            LinterDiagnosticCategory::Sui as u8,
            LinterDiagnosticCode::PreferMutableTxContext as u8,
            Some(PREFER_MUTABLE_TX_CONTEXT_FILTER_NAME),
        ),
    ];

    (Some(ALLOW_ATTR_CATEGORY.into()), filters)
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
