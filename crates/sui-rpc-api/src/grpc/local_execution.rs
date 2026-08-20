// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Handler for the node-local `LocalExecutionService` (see `build.rs`). It waits until this node
//! has locally executed a transaction and returns its effects — before checkpointing or indexing.

use std::time::Duration;

use sui_types::local_execution::WaitForLocalEffectsRequest;
use sui_types::local_execution::WaitForLocalEffectsResponse;
use tonic::Request;
use tonic::Response;
use tonic::Status;

use crate::RpcService;

mod generated {
    include!(concat!(env!("OUT_DIR"), "/sui.node.LocalExecutionService.rs"));
}

pub use generated::local_execution_service_client::LocalExecutionServiceClient;
pub use generated::local_execution_service_server::LocalExecutionService;
pub use generated::local_execution_service_server::LocalExecutionServiceServer;

/// Wait bound applied when the request does not specify one.
const DEFAULT_WAIT_TIMEOUT: Duration = Duration::from_secs(60);
/// Upper bound on the client-supplied wait, so a caller cannot pin a request indefinitely.
const MAX_WAIT_TIMEOUT: Duration = Duration::from_secs(120);

#[tonic::async_trait]
impl LocalExecutionService for RpcService {
    async fn wait_for_local_effects(
        &self,
        request: Request<WaitForLocalEffectsRequest>,
    ) -> Result<Response<WaitForLocalEffectsResponse>, Status> {
        let request = request.into_inner();

        let waiter = self
            .local_effects_waiter
            .as_ref()
            .ok_or_else(|| Status::unimplemented("local execution service is not enabled"))?;

        let timeout = request
            .timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(DEFAULT_WAIT_TIMEOUT)
            .min(MAX_WAIT_TIMEOUT);

        let wait =
            waiter.wait_for_local_effects(request.transaction_digest, request.include_details);

        match tokio::time::timeout(timeout, wait).await {
            Ok(Ok(local)) => Ok(Response::new(WaitForLocalEffectsResponse::Executed {
                effects_digest: local.effects_digest,
                effects: local.effects,
            })),
            Ok(Err(e)) => Err(Status::internal(e.to_string())),
            Err(_elapsed) => Ok(Response::new(WaitForLocalEffectsResponse::TimedOut)),
        }
    }
}
