// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_kvstore::CheckpointData;
use sui_rpc::field::{FieldMask, FieldMaskTree, FieldMaskUtil};
use sui_rpc::proto::sui::rpc::v2::get_checkpoint_request::CheckpointId;
use sui_rpc::proto::sui::rpc::v2::{Checkpoint, GetCheckpointRequest, GetCheckpointResponse};
use sui_rpc_api::{
    CheckpointNotFoundError, ErrorReason, RpcError, proto::google::rpc::bad_request::FieldViolation,
};
use sui_types::digests::CheckpointDigest;

use crate::bigtable_client::BigTableClient;
use crate::config::{PipelineStage, ResolvedStageConfig, StagesConfig};
use crate::render;
use crate::resolve;
use crate::served_height::ServedHeight;

pub const READ_MASK_DEFAULT: &str = sui_rpc_api::read_mask_defaults::CHECKPOINT;

/// Serve the checkpoint the request names, or the latest one, up to `served_height`. A checkpoint
/// above it is `NotFound`, as if not yet written: its `checkpoints` row can already exist while the
/// `transactions` and `objects` rows a full read mask joins are still being written.
pub async fn get_checkpoint(
    client: BigTableClient,
    stages: &StagesConfig,
    served_height: ServedHeight,
    request: GetCheckpointRequest,
) -> Result<GetCheckpointResponse, RpcError> {
    let read_mask = {
        let read_mask = request
            .read_mask
            .unwrap_or_else(|| FieldMask::from_str(READ_MASK_DEFAULT));
        read_mask.validate::<Checkpoint>().map_err(|path| {
            FieldViolation::new("read_mask")
                .with_description(format!("invalid read_mask path: {path}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;
        FieldMaskTree::from(read_mask)
    };
    let needs_full = resolve::needs_transactions_or_objects(&read_mask);
    let columns = resolve::list_checkpoint_columns(&read_mask, needs_full);
    let checkpoint = match request.checkpoint_id {
        Some(CheckpointId::Digest(digest)) => {
            let digest = digest.parse::<CheckpointDigest>().map_err(|e| {
                FieldViolation::new("digest")
                    .with_description(format!("invalid digest: {e}"))
                    .with_reason(ErrorReason::FieldInvalid)
            })?;
            let checkpoint = client
                .get_checkpoint_by_digest_filtered(digest, Some(&columns))
                .await?
                .ok_or(CheckpointNotFoundError::digest(digest.into()))?;
            if checkpoint
                .summary
                .as_ref()
                .is_some_and(|summary| summary.sequence_number > served_height.checkpoint)
            {
                return Err(CheckpointNotFoundError::digest(digest.into()).into());
            }
            checkpoint
        }
        checkpoint_id => {
            let sequence_number = match checkpoint_id {
                Some(CheckpointId::SequenceNumber(sequence_number)) => sequence_number,
                _ => served_height.checkpoint,
            };
            if sequence_number > served_height.checkpoint {
                return Err(CheckpointNotFoundError::sequence_number(sequence_number).into());
            }
            client
                .get_checkpoints_filtered(&[sequence_number], Some(&columns))
                .await?
                .pop()
                .ok_or(CheckpointNotFoundError::sequence_number(sequence_number))?
        }
    };

    let message = if needs_full {
        let transactions_stage = stages.stage(PipelineStage::Transactions);
        let objects_stage = stages.stage(PipelineStage::Objects);
        resolve_full_checkpoint(
            client,
            checkpoint,
            &read_mask,
            transactions_stage,
            objects_stage,
        )
        .await?
    } else {
        render::checkpoint_to_response(checkpoint, &read_mask)?
    };
    Ok(GetCheckpointResponse::new(message))
}

/// Heavy path: resolve the checkpoint's transactions and (when requested)
/// objects from BigTable, then render the full proto `Checkpoint`.
async fn resolve_full_checkpoint(
    client: BigTableClient,
    checkpoint: CheckpointData,
    read_mask: &FieldMaskTree,
    transactions_stage: ResolvedStageConfig,
    objects_stage: ResolvedStageConfig,
) -> Result<Checkpoint, RpcError> {
    let cp_seq = checkpoint
        .summary
        .as_ref()
        .map(|s| s.sequence_number)
        .ok_or_else(|| RpcError::new(tonic::Code::Internal, "checkpoint summary column missing"))?;

    let (_, cp_data, txs, objects) = resolve::resolve_checkpoint(
        client,
        read_mask,
        transactions_stage,
        objects_stage,
        cp_seq,
        checkpoint,
    )
    .await?;

    render::render_full_checkpoint(cp_data, txs, objects, read_mask)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use sui_kvstore::tables::checkpoints_by_digest;
    use sui_kvstore::testing::{MockBigtableServer, insert_checkpoint_rows};
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;
    use crate::bigtable_client::{Metrics, stage};

    /// A mock BigTable holding checkpoints 0 and 1, one transaction each, both findable by
    /// digest. Returns the mock, its server task, a client connected to it, and checkpoint 1's
    /// digest.
    async fn two_checkpoint_replica() -> (
        MockBigtableServer,
        tokio::task::JoinHandle<()>,
        sui_kvstore::BigTableClient,
        CheckpointDigest,
    ) {
        let mock = MockBigtableServer::new();
        let (addr, server) = mock.start().await.expect("start mock BigTable");
        let client = sui_kvstore::BigTableClient::new_local(addr.to_string(), "test".to_string())
            .await
            .expect("connect to mock BigTable");
        let mut builder = TestCheckpointBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(10)
            .finish_transaction();
        let checkpoint_0 = builder.build_checkpoint();
        let checkpoint_1 = builder
            .start_transaction(0)
            .create_owned_object(11)
            .finish_transaction()
            .build_checkpoint();
        for checkpoint in [&checkpoint_0, &checkpoint_1] {
            insert_checkpoint_rows(&mock, checkpoint).await;
            mock.insert_row(
                checkpoints_by_digest::NAME,
                checkpoints_by_digest::encode_key(checkpoint.summary.digest()),
                checkpoints_by_digest::encode(checkpoint.summary.sequence_number),
            )
            .await;
        }
        (mock, server, client, *checkpoint_1.summary.digest())
    }

    #[tokio::test]
    async fn checkpoint_above_served_height_is_not_found() {
        let (_mock, server, client, digest_1) = two_checkpoint_replica().await;
        let client = BigTableClient::new(client, 2, Metrics::for_testing().0, "test");
        let digest_1 = digest_1.to_string();

        let stages = StagesConfig::default();
        let get = |served_checkpoint: u64, checkpoint_id: Option<CheckpointId>| {
            let mut request = GetCheckpointRequest::default();
            request.checkpoint_id = checkpoint_id;
            request.read_mask = Some(FieldMask::from_paths([
                "sequence_number",
                "transactions.digest",
            ]));
            get_checkpoint(
                client.clone(),
                &stages,
                ServedHeight {
                    checkpoint: served_checkpoint,
                    epoch: 0,
                },
                request,
            )
        };
        let sequence_number = |response: Result<GetCheckpointResponse, RpcError>| {
            let checkpoint = response
                .expect("served checkpoint is found")
                .checkpoint
                .expect("response carries a checkpoint");
            assert_eq!(checkpoint.transactions.len(), 1);
            checkpoint.sequence_number.expect("sequence_number is set")
        };
        let code = |response: Result<GetCheckpointResponse, RpcError>| {
            tonic::Status::from(response.expect_err("checkpoint above the served height")).code()
        };

        // Checkpoint 1 is written, but above a served height of 0.
        assert_eq!(
            code(get(0, Some(CheckpointId::SequenceNumber(1))).await),
            tonic::Code::NotFound
        );
        assert_eq!(
            code(get(0, Some(CheckpointId::Digest(digest_1.clone()))).await),
            tonic::Code::NotFound
        );
        assert_eq!(sequence_number(get(0, None).await), 0);

        assert_eq!(
            sequence_number(get(1, Some(CheckpointId::SequenceNumber(1))).await),
            1
        );
        assert_eq!(
            sequence_number(get(1, Some(CheckpointId::Digest(digest_1))).await),
            1
        );
        assert_eq!(sequence_number(get(1, None).await), 1);

        server.abort();
    }

    /// GetCheckpoint reads its checkpoint through the request's limited client under one
    /// `checkpoints` permit, so a request with a single permit finishes: a digest reads the
    /// `checkpoints_by_digest` index, then, if indexed, the `checkpoints` row, both under the
    /// same permit. A digest without an index row, or whose index row names a checkpoint without
    /// a row, is `NotFound` by that digest.
    #[tokio::test]
    async fn checkpoint_reads_go_through_the_request_limiter() {
        let (mock, server, kv_client, digest_1) = two_checkpoint_replica().await;
        // Indexed at checkpoint 3, which is within the served height but has no row.
        let dangling_digest = CheckpointDigest::new([7; 32]);
        mock.insert_row(
            checkpoints_by_digest::NAME,
            checkpoints_by_digest::encode_key(&dangling_digest),
            checkpoints_by_digest::encode(3),
        )
        .await;
        let unindexed_digest = CheckpointDigest::new([8; 32]);
        let stages = StagesConfig::default();

        // Each request, the gated `checkpoints` reads it starts, and the sequence number it
        // serves or the digest it is `NotFound` by.
        for (checkpoint_id, expected_reads, expected) in [
            (CheckpointId::SequenceNumber(1), 1, Ok(1)),
            (CheckpointId::Digest(digest_1.to_string()), 1, Ok(1)),
            (
                CheckpointId::Digest(unindexed_digest.to_string()),
                1,
                Err(unindexed_digest),
            ),
            (
                CheckpointId::Digest(dangling_digest.to_string()),
                1,
                Err(dangling_digest),
            ),
        ] {
            let (metrics, _registry) = Metrics::for_testing();
            let client = BigTableClient::new(kv_client.clone(), 1, metrics.clone(), "test");
            let mut request = GetCheckpointRequest::default();
            request.checkpoint_id = Some(checkpoint_id.clone());
            request.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
            let response = tokio::time::timeout(
                Duration::from_secs(30),
                get_checkpoint(
                    client,
                    &stages,
                    ServedHeight {
                        checkpoint: 5,
                        epoch: 0,
                    },
                    request,
                ),
            )
            .await
            .unwrap_or_else(|_| panic!("{checkpoint_id:?}: request did not finish"));

            match expected {
                Ok(sequence_number) => assert_eq!(
                    response
                        .expect("indexed checkpoint is found")
                        .checkpoint
                        .expect("response carries a checkpoint")
                        .sequence_number,
                    Some(sequence_number),
                    "{checkpoint_id:?}"
                ),
                Err(digest) => {
                    let status = tonic::Status::from(response.expect_err("checkpoint is missing"));
                    let expected = tonic::Status::from(RpcError::from(
                        CheckpointNotFoundError::digest(digest.into()),
                    ));
                    assert_eq!(status.code(), tonic::Code::NotFound, "{checkpoint_id:?}");
                    assert_eq!(status.message(), expected.message(), "{checkpoint_id:?}");
                }
            }
            assert_eq!(
                metrics
                    .reads_started
                    .with_label_values(&["test", stage::CHECKPOINTS])
                    .get(),
                expected_reads,
                "{checkpoint_id:?}"
            );
        }

        server.abort();
    }
}
