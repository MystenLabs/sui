// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use consensus_config::{AuthorityIndex, Committee, Parameters};
use sui_protocol_config::ProtocolConfig;

use crate::metrics::Metrics;

/// Context contains per-epoch configuration and metrics shared by all components
/// of this authority.
#[allow(dead_code)]
pub(crate) struct Context {
    /// Index of this authority in the committee.
    pub own_index: AuthorityIndex,
    /// Committee of the current epoch.
    pub committee: Committee,
    /// Parameters of this authority.
    pub parameters: Parameters,
    /// Protocol configuration of current epoch.
    pub protocol_config: ProtocolConfig,
    /// Metrics of this authority.
    pub metrics: Arc<Metrics>,
}

impl Context {
    #[allow(dead_code)]
    pub(crate) fn new(
        own_index: AuthorityIndex,
        committee: Committee,
        parameters: Parameters,
        protocol_config: ProtocolConfig,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            own_index,
            committee,
            parameters,
            protocol_config,
            metrics,
        }
    }
}
