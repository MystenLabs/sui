// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use sui_types::committee::Committee;

#[derive(Clone, Serialize, Deserialize)]
pub struct EpochInfoLocals {
    pub committee: Committee,
    pub validator_halted: bool,
    // TODO: Eventually, we should also add last_checkpoint.
    // For now, we can assume that epoch changes every constant number
    // of checkpoints.
}
