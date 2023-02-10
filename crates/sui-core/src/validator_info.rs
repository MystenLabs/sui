// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_config::ValidatorInfo;
use sui_types::{
    committee::{Committee, EpochId, ProtocolVersion},
    error::SuiResult,
};

pub fn make_committee(
    epoch: EpochId,
    protocol_version: ProtocolVersion,
    validator_set: &[ValidatorInfo],
) -> SuiResult<Committee> {
    Committee::new(
        epoch,
        protocol_version,
        ValidatorInfo::voting_rights(validator_set),
    )
}
