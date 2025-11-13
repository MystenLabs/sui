// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, LazyLock};

use bytes::{BufMut, Bytes, BytesMut};
use object_store::path::Path as ObjectPath;
use prost::Message;
use sui_indexer_alt_framework::pipeline::concurrent::Handler;
use sui_indexer_alt_framework::pipeline::{Processor, concurrent::BatchStatus};
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_object_store::ObjectStore;
use sui_rpc::field::{FieldMask, FieldMaskUtil};
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc;
use sui_types::full_checkpoint_content::Checkpoint;

pub struct CheckpointBlob {
    pub sequence_number: u64,
    pub proto_bytes: Bytes,
}

pub struct CheckpointBlobPipeline {
    pub compression_level: Option<i32>,
}

#[async_trait::async_trait]
impl Processor for CheckpointBlobPipeline {
    const NAME: &'static str = "checkpoint_blob";
    type Value = CheckpointBlob;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        static MASK: LazyLock<sui_rpc::field::FieldMaskTree> = LazyLock::new(|| {
            FieldMask::from_paths([
                rpc::v2::Checkpoint::path_builder().sequence_number(),
                rpc::v2::Checkpoint::path_builder().summary().bcs().value(),
                rpc::v2::Checkpoint::path_builder().signature().finish(),
                rpc::v2::Checkpoint::path_builder().contents().bcs().value(),
                rpc::v2::Checkpoint::path_builder()
                    .transactions()
                    .transaction()
                    .bcs()
                    .value(),
                rpc::v2::Checkpoint::path_builder()
                    .transactions()
                    .effects()
                    .bcs()
                    .value(),
                rpc::v2::Checkpoint::path_builder()
                    .transactions()
                    .effects()
                    .unchanged_loaded_runtime_objects()
                    .finish(),
                rpc::v2::Checkpoint::path_builder()
                    .transactions()
                    .events()
                    .bcs()
                    .value(),
                rpc::v2::Checkpoint::path_builder()
                    .objects()
                    .objects()
                    .bcs()
                    .value(),
            ])
            .into()
        });

        let sequence_number = checkpoint.summary.sequence_number;
        let proto_checkpoint = rpc::v2::Checkpoint::merge_from(checkpoint.as_ref(), &MASK);
        let proto_bytes = Bytes::from(proto_checkpoint.encode_to_vec());

        Ok(vec![CheckpointBlob {
            sequence_number,
            proto_bytes,
        }])
    }
}

#[async_trait::async_trait]
impl Handler for CheckpointBlobPipeline {
    type Store = ObjectStore;
    type Batch = Option<Self::Value>;

    fn batch(
        &self,
        batch: &mut Self::Batch,
        values: &mut std::vec::IntoIter<Self::Value>,
    ) -> BatchStatus {
        if batch.is_none() && values.len() > 0 {
            *batch = values.next();
            BatchStatus::Ready
        } else {
            BatchStatus::Pending
        }
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> anyhow::Result<usize> {
        let Some(blob) = batch else {
            return Ok(0);
        };

        let mut path = format!("{}.binpb", blob.sequence_number);
        let data: Bytes = if let Some(level) = self.compression_level {
            path = format!("{}.zst", path);
            tokio::task::spawn_blocking({
                let bytes = blob.proto_bytes.clone();
                move || {
                    let compressed = BytesMut::new();
                    let mut writer = compressed.writer();
                    let mut encoder = zstd::Encoder::new(&mut writer, level)?;
                    std::io::copy(&mut &bytes[..], &mut encoder)?;
                    encoder.finish()?;
                    Ok::<Bytes, std::io::Error>(writer.into_inner().freeze())
                }
            })
            .await??
        } else {
            blob.proto_bytes.clone()
        };

        conn.object_store()
            .put(&ObjectPath::from(path), data.into())
            .await?;
        Ok(1)
    }
}
