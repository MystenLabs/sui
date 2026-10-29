// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! gRPC service for administrative control of the forked network: advancing the clock, creating
//! checkpoints, and querying status.

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

        Ok(Response::new(
            self.context
                .advance_clock(Duration::from_millis(duration_ms))
                .await,
        ))
    }

    async fn advance_checkpoint(
        &self,
        _request: Request<AdvanceCheckpointRequest>,
    ) -> Result<Response<AdvanceCheckpointResponse>, Status> {
        Ok(Response::new(self.context.advance_checkpoint().await))
    }

    async fn get_status(
        &self,
        _request: Request<GetStatusRequest>,
    ) -> Result<Response<GetStatusResponse>, Status> {
        Ok(Response::new(self.context.status().await))
    }
}
