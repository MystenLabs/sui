// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! TODO: add description

pub struct BridgeQuorumDriver {}

impl BridgeQuorumDriver {
    pub async fn get_committee_signatures(
        _event: SuiBridgeEvent,
        _committee: Arc<BridgeCommittee>,
    ) -> BridgeResult<BridgeCommitteeValiditySignInfo> {
        unimplemented!()
    }
}
