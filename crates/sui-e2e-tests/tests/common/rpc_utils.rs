// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use mysten_common::backoff::ExponentialBackoff;
use sui_rpc::proto::sui::rpc::v2alpha::GetCheckpointObjectProofRequest;
use sui_rpc::proto::sui::rpc::v2alpha::GetCheckpointObjectProofResponse;
use sui_rpc::proto::sui::rpc::v2alpha::proof_service_client::ProofServiceClient;

/// Retries `request` until the object-proof index catches up to the requested
/// checkpoint and returns the first indexed response. The proof RPC returns
/// NotFound until the checkpoint is indexed (see test_checkpoint_not_yet_indexed),
/// and the index runs behind the fullnode's latest checkpoint, so tests that pick
/// a checkpoint from fullnode state must wait for the index to catch up. Sleeps
/// are virtual under the simulator, so the generous budget costs nothing once the
/// index catches up.
pub async fn get_object_proof_when_indexed(
    proof_client: &mut ProofServiceClient<tonic::transport::Channel>,
    request: GetCheckpointObjectProofRequest,
) -> GetCheckpointObjectProofResponse {
    const TIMEOUT: Duration = Duration::from_secs(30);
    let mut backoff = ExponentialBackoff::new(Duration::from_millis(100), Duration::from_secs(1));
    tokio::time::timeout(TIMEOUT, async {
        loop {
            match proof_client
                .get_checkpoint_object_proof(request.clone())
                .await
            {
                Ok(response) => return response.into_inner(),
                Err(status)
                    if status.code() == tonic::Code::NotFound
                        && status.message().contains("not yet indexed") =>
                {
                    tokio::time::sleep(backoff.next().unwrap()).await;
                }
                Err(status) => panic!("proof request should succeed once indexed: {status:?}"),
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("object proof index did not catch up within {TIMEOUT:?}"))
}
