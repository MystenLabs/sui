// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::EpochId;

/// The static epoch information that is accessible to move smart contracts
pub struct EpochData {
    epoch_id: EpochId,
}

impl EpochData {
    pub fn new(epoch_id: EpochId) -> Self {
        Self { epoch_id }
    }

    pub fn genesis() -> Self {
        Self { epoch_id: 0 }
    }

    pub fn epoch_id(&self) -> EpochId {
        self.epoch_id
    }
}
