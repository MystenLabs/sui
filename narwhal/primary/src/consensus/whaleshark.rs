// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use types::{Certificate, CommittedSubDag};

use crate::{
    consensus::{ConsensusState, Protocol},
    ConsensusError, Outcome,
};

pub struct Whaleshark {}

impl Whaleshark {
    pub fn new() -> Self {
        Self {}
    }
}

impl Protocol for Whaleshark {
    fn process_certificate(
        &mut self,
        state: &mut ConsensusState,
        certificate: Certificate,
    ) -> Result<(Outcome, Vec<CommittedSubDag>), ConsensusError> {
        Ok((Outcome::Commit, Vec::new()))
    }
}
