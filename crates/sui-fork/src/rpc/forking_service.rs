// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! gRPC service for administrative control of the forked network: advancing the clock, creating
//! checkpoints, and querying status.
//!
//! Each method is a thin delegate to the corresponding [`Context`] operation, which the in-process
//! [`crate::ForkNode`] handle also delegates to, so the two surfaces share one contract.

use std::sync::Arc;
use std::time::Duration;

use tonic::Request;
use tonic::Response;
use tonic::Status;

use crate::context::Context;
use crate::proto::forking::AdvanceCheckpointRequest;
use crate::proto::forking::AdvanceCheckpointResponse;
use crate::proto::forking::AdvanceClockRequest;
use crate::proto::forking::AdvanceClockResponse;
use crate::proto::forking::GetStatusRequest;
use crate::proto::forking::GetStatusResponse;
use crate::proto::forking::forking_service_server::ForkingService;

const DEFAULT_ADVANCE_CLOCK_MS: u64 = 1;

pub(crate) struct ForkingServiceImpl {
    context: Arc<Context>,
}

impl ForkingServiceImpl {
    pub(crate) fn new(context: Arc<Context>) -> Self {
        Self { context }
    }
}

#[tonic::async_trait]
impl ForkingService for ForkingServiceImpl {
    async fn advance_clock(
        &self,
        request: Request<AdvanceClockRequest>,
    ) -> Result<Response<AdvanceClockResponse>, Status> {
        let duration_ms = request
            .into_inner()
            .duration_ms
            .unwrap_or(DEFAULT_ADVANCE_CLOCK_MS);

        let advanced = self
            .context
            .advance_clock(Duration::from_millis(duration_ms))
            .await;

        Ok(Response::new(AdvanceClockResponse {
            timestamp_ms: advanced.timestamp_ms,
            tx_digest: advanced.tx_digest.to_string(),
        }))
    }

    async fn advance_checkpoint(
        &self,
        _request: Request<AdvanceCheckpointRequest>,
    ) -> Result<Response<AdvanceCheckpointResponse>, Status> {
        let checkpoint = self.context.create_checkpoint().await;

        Ok(Response::new(AdvanceCheckpointResponse {
            checkpoint_sequence_number: checkpoint.sequence_number,
            timestamp_ms: checkpoint.timestamp_ms,
        }))
    }

    async fn get_status(
        &self,
        _request: Request<GetStatusRequest>,
    ) -> Result<Response<GetStatusResponse>, Status> {
        let status = self.context.status().await;

        Ok(Response::new(GetStatusResponse {
            epoch: status.epoch,
            checkpoint_sequence_number: status.checkpoint_sequence_number,
            timestamp_ms: status.timestamp_ms,
            forked_at_checkpoint: status.forked_at_checkpoint,
        }))
    }
}
