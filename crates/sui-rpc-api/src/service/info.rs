// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::proto::node::v2::GetNodeInfoResponse;
use crate::proto::types::timestamp_ms_to_proto;
use crate::Result;
use crate::RpcService;
use sui_sdk_types::CheckpointDigest;
use tap::Pipe;

impl RpcService {
    pub fn get_node_info(&self) -> Result<GetNodeInfoResponse> {
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

        GetNodeInfoResponse {
            chain_id: Some(CheckpointDigest::new(self.chain_id().as_bytes().to_owned()).into()),
            chain: Some(self.chain_id().chain().as_str().into()),
            epoch: Some(latest_checkpoint.epoch()),
            checkpoint_height: Some(latest_checkpoint.sequence_number),
            timestamp: Some(timestamp_ms_to_proto(latest_checkpoint.timestamp_ms)),
            lowest_available_checkpoint,
            lowest_available_checkpoint_objects,
            software_version: Some(self.software_version().into()),
        }
        .pipe(Ok)
    }
}
