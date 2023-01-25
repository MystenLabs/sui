// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::AuthorityName;
use crate::committee::{Committee, StakeUnit};
use crate::crypto::{AuthoritySignInfo, AuthorityStrongQuorumSignInfo};
use crate::error::{SuiError, SuiResult};
use std::collections::HashSet;
use tracing::debug;

pub struct StakeAccumulator {
    signed_authorities: HashSet<AuthorityName>,
    auth_sigs: Vec<AuthoritySignInfo>,
    total_stake: StakeUnit,
    committee: Committee,
    cert: Option<AuthorityStrongQuorumSignInfo>,
}

impl StakeAccumulator {
    pub fn new(committee: Committee) -> Self {
        Self {
            signed_authorities: HashSet::new(),
            auth_sigs: vec![],
            total_stake: 0,
            committee,
            cert: None,
        }
    }

    pub fn new_with_initial_stake(initial_sig: AuthoritySignInfo, committee: Committee) -> Self {
        let mut this = Self::new(committee);
        this.add_stake(initial_sig);
        this
    }

    /// Add a new authority signature. If we have reached quorum, return the certificate.
    /// Otherwise return None.
    pub fn add_stake(&mut self, sig: AuthoritySignInfo) -> Option<AuthorityStrongQuorumSignInfo> {
        if let Err(err) = self.add_stake_impl(sig) {
            debug!("Failed to add stake to the accumulator: {:?}", err);
        }
        self.cert.clone()
    }

    pub fn total_stake(&self) -> StakeUnit {
        self.total_stake
    }

    fn add_stake_impl(&mut self, sig: AuthoritySignInfo) -> SuiResult {
        fp_ensure!(
            self.committee.epoch == sig.epoch,
            SuiError::WrongEpoch {
                expected_epoch: self.committee.epoch,
                actual_epoch: sig.epoch,
            }
        );
        let stake = self.committee.weight(&sig.authority);
        if stake > 0 && self.signed_authorities.insert(sig.authority) {
            self.total_stake += stake;
            self.auth_sigs.push(sig);
            if self.total_stake >= AuthorityStrongQuorumSignInfo::quorum_threshold(&self.committee)
            {
                self.cert = Some(AuthorityStrongQuorumSignInfo::new_from_auth_sign_infos(
                    self.auth_sigs.clone(),
                    &self.committee,
                )?);
            }
        }
        Ok(())
    }
}
