// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_config::ValidatorInfo;
use sui_types::{
    committee::{Committee, EpochId},
    error::SuiResult,
};

pub fn make_committee(epoch: EpochId, validator_set: &[ValidatorInfo]) -> SuiResult<Committee> {
    Committee::new(epoch, ValidatorInfo::voting_rights(validator_set))
}
