// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::Result;
use crate::{types::NodeInfo, RpcService};
use sui_sdk_types::CheckpointDigest;
use tap::Pipe;

impl RpcService {
    pub fn get_node_info(&self) -> Result<NodeInfo> {
        let latest_checkpoint = self.reader.inner().get_latest_checkpoint()?;
        let lowest_available_checkpoint = self
            .reader
            .inner()
            .get_lowest_available_checkpoint()?
            .pipe(Some);
        let lowest_available_checkpoint_objects = self
            .reader
            .inner()
            .get_lowest_available_checkpoint_objects()?
            .pipe(Some);

        NodeInfo {
            checkpoint_height: latest_checkpoint.sequence_number,
            lowest_available_checkpoint,
            lowest_available_checkpoint_objects,
            timestamp_ms: latest_checkpoint.timestamp_ms,
            epoch: latest_checkpoint.epoch(),
            chain_id: CheckpointDigest::new(self.chain_id().as_bytes().to_owned()),
            chain: self.chain_id().chain().as_str().into(),
            software_version: self.software_version().into(),
        }
        .pipe(Ok)
    }
}
