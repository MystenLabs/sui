// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_compiler::{
    diagnostics::codes::{DiagnosticsID, WarningFilter},
    expansion::ast as E,
};

pub mod self_transfer;
pub mod share_owned;

pub const SHARE_OWNED_DIAG_CATEGORY: u8 = 1;
pub const SELF_TRANSFER_DIAG_CATEGORY: u8 = 2;

pub const ALLOW_ATTR_NAME: &str = "linter_allow";

pub const SHARE_OWNED_FILTER_NAME: &str = "share_owned";
pub const SELF_TRANSFER_FILTER_NAME: &str = "self_transfer";

pub fn known_filters() -> (E::AttributeName_, Vec<WarningFilter>) {
    (
        E::AttributeName_::Unknown(ALLOW_ATTR_NAME.into()),
        vec![
            WarningFilter::All,
            WarningFilter::Code(
                DiagnosticsID {
                    category: SHARE_OWNED_DIAG_CATEGORY,
                    code: 1,
                },
                Some(SHARE_OWNED_FILTER_NAME),
            ),
            WarningFilter::Code(
                DiagnosticsID {
                    category: SELF_TRANSFER_DIAG_CATEGORY,
                    code: 1,
                },
                Some(SELF_TRANSFER_FILTER_NAME),
            ),
        ],
    )
}
