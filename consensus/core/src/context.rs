// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use consensus_config::{AuthorityIndex, Committee, Parameters};
use sui_protocol_config::ProtocolConfig;

use crate::metrics::Metrics;

#[cfg(test)]
use crate::metrics::test_metrics;

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

#[allow(dead_code)]
impl Context {
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

    /// Create a test context with an optional committee of given size and even stake
    #[cfg(test)]
    pub(crate) fn new_for_test(committee_size: Option<usize>) -> Self {
        let size = committee_size.unwrap_or(4);
        let (committee, _) = Committee::new_for_test(0, vec![1; size]);
        let metrics = test_metrics();
        Context::new(
            AuthorityIndex::new_for_test(0),
            committee,
            Parameters::default(),
            ProtocolConfig::get_for_min_version(),
            metrics,
        )
    }

    #[cfg(test)]
    pub(crate) fn with_committee(mut self, committee: Committee) -> Self {
        self.committee = committee;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_authority_index(mut self, authority: AuthorityIndex) -> Self {
        self.own_index = authority;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_parameters(mut self, parameters: Parameters) -> Self {
        self.parameters = parameters;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_protocol_config(mut self, protocol_config: ProtocolConfig) -> Self {
        self.protocol_config = protocol_config;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_metrics(mut self, metrics: Arc<Metrics>) -> Self {
        self.metrics = metrics;
        self
    }
}
