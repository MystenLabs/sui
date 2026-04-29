// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! gRPC service for administrative control of the forked network:
//! advancing the clock, creating checkpoints, and querying status.

use std::sync::Arc;
use std::time::Duration;

use simulacrum::SimulatorStore as _;
use sui_types::effects::TransactionEffectsAPI as _;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait as _;
use tonic::Request;
use tonic::Response;
use tonic::Status;
use tracing::info;

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

        let ((tx_digest, timestamp_ms), checkpoint_metadata) = self
            .context
            .run_with_new_checkpoint(|sim| {
                let effects = sim.advance_clock(Duration::from_millis(duration_ms));
                let tx_digest = *effects.transaction_digest();
                let timestamp_ms = sim.store().get_clock().timestamp_ms;
                (tx_digest, timestamp_ms)
            })
            .await;

        info!(
            %tx_digest,
            duration_ms,
            timestamp_ms,
            checkpoint_sequence_number = checkpoint_metadata.sequence_number,
            "clock advanced"
        );

        Ok(Response::new(AdvanceClockResponse {
            timestamp_ms,
            tx_digest: tx_digest.to_string(),
        }))
    }

    async fn advance_checkpoint(
        &self,
        _request: Request<AdvanceCheckpointRequest>,
    ) -> Result<Response<AdvanceCheckpointResponse>, Status> {
        let (_, checkpoint_metadata) = self.context.run_with_new_checkpoint(|_| ()).await;

        info!(
            checkpoint_sequence_number = checkpoint_metadata.sequence_number,
            timestamp_ms = checkpoint_metadata.timestamp_ms,
            "checkpoint created"
        );

        Ok(Response::new(AdvanceCheckpointResponse {
            checkpoint_sequence_number: checkpoint_metadata.sequence_number,
            timestamp_ms: checkpoint_metadata.timestamp_ms,
        }))
    }

    async fn get_status(
        &self,
        _request: Request<GetStatusRequest>,
    ) -> Result<Response<GetStatusResponse>, Status> {
        let sim = self.context.simulacrum().read().await;
        let epoch = sim.epoch_start_state().epoch();
        let timestamp_ms = sim.store().get_clock().timestamp_ms;
        let checkpoint_sequence_number = sim
            .store()
            .get_highest_checkpint()
            .map(|cp| cp.data().sequence_number)
            .unwrap_or(0);

        let forked_at_checkpoint = sim.store().forked_at_checkpoint();

        Ok(Response::new(GetStatusResponse {
            epoch,
            checkpoint_sequence_number,
            timestamp_ms,
            forked_at_checkpoint,
        }))
    }
}
