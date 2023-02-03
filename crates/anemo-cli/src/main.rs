// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use narwhal_types::*;
use sui_network::discovery::*;
use sui_network::state_sync::*;

// TODO: fix ron_method macro to use `ty` instead of `path` and remove this.
type Unit = ();

#[tokio::main]
async fn main() {
    // TODO: implement `ServiceInfo` generation in anemo-build and use here.
    let config = anemo_cli::Config::new()
        // Narwhal primary-to-primary
        .add_service(
            "PrimaryToPrimary",
            anemo_cli::ServiceInfo::new()
                .add_method(
                    "SendMessage",
                    anemo_cli::ron_method!(PrimaryToPrimaryClient, send_message, PrimaryMessage),
                )
                .add_method(
                    "RequestVote",
                    anemo_cli::ron_method!(
                        PrimaryToPrimaryClient,
                        request_vote,
                        RequestVoteRequest
                    ),
                )
                .add_method(
                    "GetPayloadAvailability",
                    anemo_cli::ron_method!(
                        PrimaryToPrimaryClient,
                        get_payload_availability,
                        PayloadAvailabilityRequest
                    ),
                )
                .add_method(
                    "GetCertificates",
                    anemo_cli::ron_method!(
                        PrimaryToPrimaryClient,
                        get_certificates,
                        GetCertificatesRequest
                    ),
                )
                .add_method(
                    "FetchCertificates",
                    anemo_cli::ron_method!(
                        PrimaryToPrimaryClient,
                        fetch_certificates,
                        FetchCertificatesRequest
                    ),
                ),
        )
        // Narwhal worker-to-worker
        .add_service(
            "WorkerToWorker",
            anemo_cli::ServiceInfo::new()
                .add_method(
                    "ReportBatch",
                    anemo_cli::ron_method!(WorkerToWorkerClient, report_batch, WorkerBatchMessage),
                )
                .add_method(
                    "RequestBatch",
                    anemo_cli::ron_method!(
                        WorkerToWorkerClient,
                        request_batch,
                        RequestBatchRequest
                    ),
                ),
        )
        // Sui discovery
        .add_service(
            "Discovery",
            anemo_cli::ServiceInfo::new()
                .add_method(
                    "GetExternalAddress",
                    anemo_cli::ron_method!(DiscoveryClient, get_external_address, Unit),
                )
                .add_method(
                    "GetKnownPeers",
                    anemo_cli::ron_method!(DiscoveryClient, get_known_peers, Unit),
                ),
        )
        // Sui state sync
        .add_service(
            "StateSync",
            anemo_cli::ServiceInfo::new()
                .add_method(
                    "PushCheckpointSummary",
                    anemo_cli::ron_method!(
                        StateSyncClient,
                        push_checkpoint_summary,
                        sui_types::messages_checkpoint::CertifiedCheckpointSummary
                    ),
                )
                .add_method(
                    "GetCheckpointSummary",
                    anemo_cli::ron_method!(
                        StateSyncClient,
                        get_checkpoint_summary,
                        GetCheckpointSummaryRequest
                    ),
                )
                .add_method(
                    "GetCheckpointContents",
                    anemo_cli::ron_method!(
                        StateSyncClient,
                        get_checkpoint_contents,
                        sui_types::messages_checkpoint::CheckpointContentsDigest
                    ),
                )
                .add_method(
                    "GetTransactionAndEffects",
                    anemo_cli::ron_method!(
                        StateSyncClient,
                        get_transaction_and_effects,
                        sui_types::base_types::ExecutionDigests
                    ),
                ),
        );
    anemo_cli::main(config).await;
}
