// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfgir::visitor::AbstractInterpreterVisitor,
    command_line::compiler::Visitor,
    diagnostics::codes::WarningFilter,
    expansion::ast as E,
    hlir::ast::{BaseType_, SingleType, SingleType_},
    naming::ast as N,
    typing::visitor::TypingVisitor,
};
use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;

pub mod coin_field;
pub mod collection_equality;
pub mod custom_state_change;
pub mod freeze_wrapped;
pub mod self_transfer;
pub mod share_owned;

pub const SUI_PKG_NAME: &str = "sui";

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

pub const ALLOW_ATTR_CATEGORY: &str = "lint";
pub const LINT_WARNING_PREFIX: &str = "Lint ";

pub const SHARE_OWNED_FILTER_NAME: &str = "share_owned";
pub const SELF_TRANSFER_FILTER_NAME: &str = "self_transfer";
pub const CUSTOM_STATE_CHANGE_FILTER_NAME: &str = "custom_state_change";
pub const COIN_FIELD_FILTER_NAME: &str = "coin_field";
pub const FREEZE_WRAPPED_FILTER_NAME: &str = "freeze_wrapped";
pub const COLLECTION_EQUALITY_FILTER_NAME: &str = "collection_equality";

pub const INVALID_LOC: Loc = Loc::invalid();

pub enum LinterDiagCategory {
    ShareOwned,
    SelfTransfer,
    CustomStateChange,
    CoinField,
    FreezeWrapped,
    CollectionEquality,
}

/// A default code for each linter category (as long as only one code per category is used, no other
/// codes are needed, otherwise they should be defined to be unique per-category).
pub const LINTER_DEFAULT_DIAG_CODE: u8 = 1;

pub fn known_filters() -> (Option<Symbol>, Vec<WarningFilter>) {
    let filters = vec![
        WarningFilter::All(Some(LINT_WARNING_PREFIX)),
        WarningFilter::code(
            Some(LINT_WARNING_PREFIX),
            LinterDiagCategory::ShareOwned as u8,
            LINTER_DEFAULT_DIAG_CODE,
            Some(SHARE_OWNED_FILTER_NAME),
        ),
        WarningFilter::code(
            Some(LINT_WARNING_PREFIX),
            LinterDiagCategory::SelfTransfer as u8,
            LINTER_DEFAULT_DIAG_CODE,
            Some(SELF_TRANSFER_FILTER_NAME),
        ),
        WarningFilter::code(
            Some(LINT_WARNING_PREFIX),
            LinterDiagCategory::CustomStateChange as u8,
            LINTER_DEFAULT_DIAG_CODE,
            Some(CUSTOM_STATE_CHANGE_FILTER_NAME),
        ),
        WarningFilter::code(
            Some(LINT_WARNING_PREFIX),
            LinterDiagCategory::CoinField as u8,
            LINTER_DEFAULT_DIAG_CODE,
            Some(COIN_FIELD_FILTER_NAME),
        ),
        WarningFilter::code(
            Some(LINT_WARNING_PREFIX),
            LinterDiagCategory::FreezeWrapped as u8,
            LINTER_DEFAULT_DIAG_CODE,
            Some(FREEZE_WRAPPED_FILTER_NAME),
        ),
        WarningFilter::code(
            Some(LINT_WARNING_PREFIX),
            LinterDiagCategory::CollectionEquality as u8,
            LINTER_DEFAULT_DIAG_CODE,
            Some(COLLECTION_EQUALITY_FILTER_NAME),
        ),
    ];
    (Some(ALLOW_ATTR_CATEGORY.into()), filters)
}

pub fn linter_visitors() -> Vec<Visitor> {
    vec![
        share_owned::ShareOwnedVerifier.visitor(),
        self_transfer::SelfTransferVerifier.visitor(),
        custom_state_change::CustomStateChangeVerifier.visitor(),
        coin_field::CoinFieldVisitor.visitor(),
        freeze_wrapped::FreezeWrappedVisitor.visitor(),
        collection_equality::CollectionEqualityVisitor.visitor(),
    ]
}

pub fn base_type(t: &N::Type) -> Option<&N::Type> {
    use N::Type_ as T;
    match &t.value {
        T::Ref(_, inner_t) => base_type(inner_t),
        T::Apply(_, _, _) | T::Param(_) => Some(t),
        T::Unit | T::Var(_) | T::Anything | T::UnresolvedError | T::Fun(_, _) => None,
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
