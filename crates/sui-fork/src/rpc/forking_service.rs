// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! gRPC service for administrative control of the forked network: advancing the clock, creating
//! checkpoints, querying status, and seeding pre-fork state while the fork runs.

use std::fmt::Display;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use tonic::Request;
use tonic::Response;
use tonic::Status;
use tracing::info;

use simulacrum::SimulatorStore as _;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::effects::TransactionEffectsAPI as _;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait as _;

use crate::context::Context;
use crate::proto::forking::AdvanceCheckpointRequest;
use crate::proto::forking::AdvanceCheckpointResponse;
use crate::proto::forking::AdvanceClockRequest;
use crate::proto::forking::AdvanceClockResponse;
use crate::proto::forking::GetStatusRequest;
use crate::proto::forking::GetStatusResponse;
use crate::proto::forking::SeedAddressRequest;
use crate::proto::forking::SeedAddressResponse;
use crate::proto::forking::SeedDynamicFieldsOfRequest;
use crate::proto::forking::SeedDynamicFieldsOfResponse;
use crate::proto::forking::SeedObjectRequest;
use crate::proto::forking::SeedObjectResponse;
use crate::proto::forking::SeedOutcome;
use crate::proto::forking::SeededObject;
use crate::proto::forking::forking_service_server::ForkingService;
use crate::seed::RuntimeSeedError;
use crate::seed::RuntimeSeedRequest;
use crate::seed::resolve_runtime_seed;

const DEFAULT_ADVANCE_CLOCK_MS: u64 = 1;

pub(crate) struct ForkingServiceImpl {
    context: Arc<Context>,
}

impl ForkingServiceImpl {
    pub(crate) fn new(context: Arc<Context>) -> Self {
        Self { context }
    }

    /// Resolve a runtime seed request against the remote, then load what it names into the fork.
    ///
    /// Resolution runs before local execution is held off, because a dynamic-field reconstruction
    /// can take many remote requests and transactions should not wait on them.
    async fn seed(&self, request: RuntimeSeedRequest) -> Result<SeedOutcome, Status> {
        let store = self.context.simulacrum().read().await.store().clone();
        let objects = resolve_runtime_seed(&store, request)
            .await
            .map_err(seed_error_status)?;
        let outcome = self
            .context
            .seed_objects(objects)
            .await
            .map_err(|err| Status::internal(format!("{err:#}")))?;

        info!(
            ?request,
            seeded = outcome.seeded.len(),
            already_present = outcome.already_present.len(),
            "runtime seed applied"
        );

        Ok(SeedOutcome {
            seeded: outcome
                .seeded
                .into_iter()
                .map(|(object_id, version, digest)| SeededObject {
                    object_id: object_id.to_string(),
                    version: version.value(),
                    digest: digest.to_string(),
                })
                .collect(),
            already_present: outcome
                .already_present
                .iter()
                .map(ToString::to_string)
                .collect(),
        })
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

    async fn seed_object(
        &self,
        request: Request<SeedObjectRequest>,
    ) -> Result<Response<SeedObjectResponse>, Status> {
        let object_id: ObjectID = parse_field("object_id", request.into_inner().object_id)?;
        let outcome = self.seed(RuntimeSeedRequest::Object(object_id)).await?;
        Ok(Response::new(SeedObjectResponse {
            outcome: Some(outcome),
        }))
    }

    async fn seed_address(
        &self,
        request: Request<SeedAddressRequest>,
    ) -> Result<Response<SeedAddressResponse>, Status> {
        let address: SuiAddress = parse_field("address", request.into_inner().address)?;
        let outcome = self.seed(RuntimeSeedRequest::Address(address)).await?;
        Ok(Response::new(SeedAddressResponse {
            outcome: Some(outcome),
        }))
    }

    async fn seed_dynamic_fields_of(
        &self,
        request: Request<SeedDynamicFieldsOfRequest>,
    ) -> Result<Response<SeedDynamicFieldsOfResponse>, Status> {
        let parent: ObjectID = parse_field("parent", request.into_inner().parent)?;
        let outcome = self
            .seed(RuntimeSeedRequest::DynamicFieldsOf(parent))
            .await?;
        Ok(Response::new(SeedDynamicFieldsOfResponse {
            outcome: Some(outcome),
        }))
    }
}

/// Parse a required string request field, reporting a missing or malformed value as an invalid
/// argument.
fn parse_field<T>(field: &str, value: Option<String>) -> Result<T, Status>
where
    T: FromStr,
    T::Err: Display,
{
    let value = value.ok_or_else(|| Status::invalid_argument(format!("missing {field}")))?;
    value
        .parse()
        .map_err(|err| Status::invalid_argument(format!("invalid {field} {value}: {err}")))
}

/// Map a runtime seed error to the gRPC status a caller can act on.
fn seed_error_status(err: RuntimeSeedError) -> Status {
    match err {
        RuntimeSeedError::NotFound(message) => Status::not_found(message),
        RuntimeSeedError::Refused(message) => Status::failed_precondition(message),
        RuntimeSeedError::Failed(err) => Status::internal(format!("{err:#}")),
    }
}
