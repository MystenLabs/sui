// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use sui_types::base_types::TransactionDigest;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CertificateDenyConfig {
    /// A list of certificate digests that are known to be either deterministically crashing
    /// every validator, or causing every validator to hang forever, i.e. there is no way
    /// for such transaction to execute successfully today.
    /// Now with this config, a validator will decide that this transaction will always yield
    /// ExecutionError and charge gas accordingly.
    /// This config is meant for a fast temporary fix for a known issue, and should be removed
    /// once the issue is fixed. However, since a certificate once executed will be included
    /// in checkpoints, all future executions of this transaction through replay must also lead
    /// to the same result (i.e. ExecutionError). So when we remove this config, we need to make
    /// sure it's added to the constant certificate deny list in the Rust code (TODO: code link).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    certificate_deny_list: Vec<TransactionDigest>,

    /// In-memory cache for faster lookup of the certificate deny list.
    #[serde(skip)]
    certificate_deny_set: OnceCell<HashSet<TransactionDigest>>,
}

impl CertificateDenyConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn certificate_deny_set(&self) -> &HashSet<TransactionDigest> {
        self.certificate_deny_set.get_or_init(|| {
            self.certificate_deny_list
                .iter()
                .cloned()
                .collect::<HashSet<_>>()
        })
    }
}

#[derive(Default)]
pub struct CertificateDenyConfigBuilder {
    config: CertificateDenyConfig,
}

impl CertificateDenyConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build(self) -> CertificateDenyConfig {
        self.config
    }

    pub fn add_certificate_deny(mut self, certificate: TransactionDigest) -> Self {
        self.config.certificate_deny_list.push(certificate);
        self
    }
}
