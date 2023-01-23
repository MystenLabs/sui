// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ReconfigCertStatus {
    AcceptAllCerts,
    RejectUserCerts,
    RejectAllCerts,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReconfigState {
    status: ReconfigCertStatus,
}

impl Default for ReconfigState {
    fn default() -> Self {
        Self {
            status: ReconfigCertStatus::AcceptAllCerts,
        }
    }
}

impl ReconfigState {
    pub fn close_user_certs(&mut self) {
        if matches!(self.status, ReconfigCertStatus::AcceptAllCerts) {
            self.status = ReconfigCertStatus::RejectUserCerts;
        }
    }

    pub fn is_reject_user_certs(&self) -> bool {
        matches!(self.status, ReconfigCertStatus::RejectUserCerts)
    }

    pub fn close_all_certs(&mut self) {
        self.status = ReconfigCertStatus::RejectAllCerts;
    }

    pub fn should_accept_user_certs(&self) -> bool {
        matches!(self.status, ReconfigCertStatus::AcceptAllCerts)
    }

    pub fn should_accept_consensus_certs(&self) -> bool {
        !matches!(self.status, ReconfigCertStatus::RejectAllCerts)
    }
}

pub trait ReconfigurationInitiator {
    fn close_epoch(&self, epoch_store: &Arc<AuthorityPerEpochStore>);
}
