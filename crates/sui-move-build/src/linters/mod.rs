// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_compiler::{
    diagnostics::codes::{DiagnosticsID, WarningFilter},
    expansion::ast as E,
};
use move_ir_types::location::Loc;

pub mod coin_field;
pub mod custom_state_change;
pub mod self_transfer;
pub mod share_owned;

pub const SUI_PKG_NAME: &str = "sui";

pub const TRANSFER_MOD_NAME: &str = "transfer";
pub const TRANSFER_FUN: &str = "transfer";
pub const PUBLIC_TRANSFER_FUN: &str = "public_transfer";
pub const SHARE_FUN: &str = "share_object";
pub const PUBLIC_SHARE_FUN: &str = "public_share_object";
pub const FREEZE_FUN: &str = "freeze_object";

pub const COIN_MOD_NAME: &str = "coin";
pub const COIN_STRUCT_NAME: &str = "Coin";

pub const ALLOW_ATTR_NAME: &str = "lint_allow";
pub const LINT_WARNING_PREFIX: &str = "Lint ";

pub const SHARE_OWNED_FILTER_NAME: &str = "share_owned";
pub const SELF_TRANSFER_FILTER_NAME: &str = "self_transfer";
pub const CUSTOM_STATE_CHANGE_FILTER_NAME: &str = "custom_state_change";
pub const COIN_FIELD_FILTER_NAME: &str = "coin_field";

pub const INVALID_LOC: Loc = Loc::invalid();

pub enum LinterDiagCategory {
    ShareOwned,
    SelfTransfer,
    CustomStateChange,
    CoinField,
}

/// A default code for each linter category (as long as only one code per category is used, no other
/// codes are needed, otherwise they should be defined to be unique per-category).
pub const LINTER_DEFAULT_DIAG_CODE: u8 = 1;

pub fn known_filters() -> (E::AttributeName_, Vec<WarningFilter>) {
    (
        E::AttributeName_::Unknown(ALLOW_ATTR_NAME.into()),
        vec![
            WarningFilter::All(Some(LINT_WARNING_PREFIX)),
            WarningFilter::Code(
                DiagnosticsID::new(
                    LinterDiagCategory::ShareOwned as u8,
                    LINTER_DEFAULT_DIAG_CODE,
                    Some(LINT_WARNING_PREFIX),
                ),
                Some(SHARE_OWNED_FILTER_NAME),
            ),
            WarningFilter::Code(
                DiagnosticsID::new(
                    LinterDiagCategory::SelfTransfer as u8,
                    LINTER_DEFAULT_DIAG_CODE,
                    Some(LINT_WARNING_PREFIX),
                ),
                Some(SELF_TRANSFER_FILTER_NAME),
            ),
            WarningFilter::Code(
                DiagnosticsID::new(
                    LinterDiagCategory::CustomStateChange as u8,
                    LINTER_DEFAULT_DIAG_CODE,
                    Some(LINT_WARNING_PREFIX),
                ),
                Some(CUSTOM_STATE_CHANGE_FILTER_NAME),
            ),
            WarningFilter::Code(
                DiagnosticsID::new(
                    LinterDiagCategory::CoinField as u8,
                    LINTER_DEFAULT_DIAG_CODE,
                    Some(LINT_WARNING_PREFIX),
                ),
                Some(COIN_FIELD_FILTER_NAME),
            ),
        ],
    )
}
