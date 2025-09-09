// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::types::gas::GasCostSummary;
use async_graphql::Object;
use sui_types::sui_system_state::{SuiSystemState, SuiSystemStateTrait};

/// Information about whether epoch changes are using safe mode.
pub(crate) struct SafeMode {
    pub enabled: Option<bool>,

    pub gas_summary: Option<GasCostSummary>,
}

#[Object]
impl SafeMode {
    /// Whether safe mode was used for the last epoch change.
    /// The system will retry a full epoch change on every epoch boundary and automatically reset this flag if so.
    async fn enabled(&self) -> Option<bool> {
        self.enabled
    }
    /// Accumulated fees for computation and cost that have not been added to the various reward pools, because the full epoch change did not happen.
    async fn gas_summary(&self) -> Option<&GasCostSummary> {
        self.gas_summary.as_ref()
    }
}

pub(crate) fn from_system_state(system_state: &SuiSystemState) -> SafeMode {
    SafeMode {
        enabled: Some(system_state.safe_mode()),
        gas_summary: Some(system_state.safe_mode_gas_cost_summary().into()),
    }
}
