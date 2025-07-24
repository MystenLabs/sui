// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::types::gas::GasCostSummary;
use async_graphql::SimpleObject;
use sui_types::sui_system_state::sui_system_state_inner_v1::SuiSystemStateInnerV1;
use sui_types::sui_system_state::sui_system_state_inner_v2::SuiSystemStateInnerV2;

/// Information about whether epoch changes are using safe mode.
#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct SafeMode {
    /// Whether safe mode was used for the last epoch change.
    /// The system will retry a full epoch change on every epoch boundary and automatically reset this flag if so.
    pub enabled: Option<bool>,

    /// Accumulated fees for computation and cost that have not been added to the various reward pools, because the full epoch change did not happen.
    pub gas_summary: Option<GasCostSummary>,
}

impl From<SuiSystemStateInnerV1> for SafeMode {
    fn from(value: SuiSystemStateInnerV1) -> Self {
        SafeMode {
            enabled: Some(value.safe_mode),
            gas_summary: Some(value.into()),
        }
    }
}

impl From<SuiSystemStateInnerV2> for SafeMode {
    fn from(value: SuiSystemStateInnerV2) -> Self {
        SafeMode {
            enabled: Some(value.safe_mode),
            gas_summary: Some(value.into()),
        }
    }
}
