// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, LazyLock};

use anyhow::Context;
use bytes::{BufMut, Bytes, BytesMut};
use object_store::path::Path as ObjectPath;
use object_store::{Error as ObjectStoreError, PutMode, PutPayload};
use prost::Message;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::concurrent::Handler;
use sui_indexer_alt_framework::store::StoreTypes;
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

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut <Self::Store as StoreTypes>::Connection<'a>,
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

pub struct EpochsPipeline;

pub struct EpochCheckpoint {
    pub checkpoint_number: u64,
}

#[async_trait::async_trait]
impl Processor for EpochsPipeline {
    const NAME: &'static str = "epochs";
    type Value = EpochCheckpoint;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        if checkpoint.summary.is_last_checkpoint_of_epoch() {
            Ok(vec![EpochCheckpoint {
                checkpoint_number: checkpoint.summary.sequence_number,
            }])
        } else {
            Ok(vec![])
        }
    }
}

#[async_trait::async_trait]
impl Handler for EpochsPipeline {
    type Store = ObjectStore;
    type Batch = Option<Self::Value>;

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut <Self::Store as StoreTypes>::Connection<'a>,
    ) -> anyhow::Result<usize> {
        let Some(epoch_checkpoint) = batch else {
            return Ok(0);
        };
        let checkpoint_num = epoch_checkpoint.checkpoint_number;

        let path = ObjectPath::from("epochs.json");
        let store = conn.object_store();

        let (mut epochs, version) = match store.get(&path).await {
            Ok(result) => {
                let version = result.meta.e_tag.clone();
                let bytes = result.bytes().await?;
                let epochs: Vec<u64> =
                    serde_json::from_slice(&bytes).context("Failed to parse epochs.json")?;
                (epochs, version)
            }
            Err(ObjectStoreError::NotFound { .. }) => (Vec::new(), None),
            Err(e) => return Err(e.into()),
        };

        match epochs.binary_search(&checkpoint_num) {
            Ok(_) => return Ok(0),
            Err(pos) => epochs.insert(pos, checkpoint_num),
        }

        let json_bytes = serde_json::to_vec(&epochs)?;
        let payload: PutPayload = Bytes::from(json_bytes).into();

        if let Some(e_tag) = version {
            store
                .put_opts(
                    &path,
                    payload,
                    PutMode::Update(object_store::UpdateVersion {
                        e_tag: Some(e_tag),
                        version: None,
                    })
                    .into(),
                )
                .await?;
        } else {
            store
                .put_opts(&path, payload, PutMode::Create.into())
                .await?;
        }

        Ok(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::memory::InMemory;
    use sui_indexer_alt_framework_store_traits::Store;

    #[tokio::test]
    async fn test_checkpoint_blob_handler_uncompressed() {
        let store = ObjectStore::new(Arc::new(InMemory::new()));
        let mut conn = store.connect().await.unwrap();

        let blob = CheckpointBlob {
            sequence_number: 100,
            proto_bytes: Bytes::from(vec![1, 2, 3, 4, 5]),
        };
        let batch = Some(blob);

        let pipeline = CheckpointBlobPipeline {
            compression_level: None,
        };
        let count = pipeline.commit(&batch, &mut conn).await.unwrap();
        assert_eq!(count, 1);

        let path = ObjectPath::from("100.binpb");
        let result = conn.object_store().get(&path).await.unwrap();
        let bytes = result.bytes().await.unwrap();
        assert_eq!(bytes.as_ref(), &[1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn test_checkpoint_blob_handler_compressed() {
        let store = ObjectStore::new(Arc::new(InMemory::new()));
        let mut conn = store.connect().await.unwrap();

        let test_data = vec![0u8; 1000];
        let blob = CheckpointBlob {
            sequence_number: 200,
            proto_bytes: Bytes::from(test_data.clone()),
        };
        let batch = Some(blob);

        let pipeline = CheckpointBlobPipeline {
            compression_level: Some(3),
        };
        let count = pipeline.commit(&batch, &mut conn).await.unwrap();
        assert_eq!(count, 1);

        let path = ObjectPath::from("200.binpb.zst");
        let result = conn.object_store().get(&path).await.unwrap();
        let compressed_bytes = result.bytes().await.unwrap();

        let decompressed = zstd::decode_all(&compressed_bytes[..]).unwrap();
        assert_eq!(decompressed, test_data);
        assert!(compressed_bytes.len() < test_data.len());
    }

    #[tokio::test]
    async fn test_epochs_handler() {
        let store = ObjectStore::new(Arc::new(InMemory::new()));
        let pipeline = EpochsPipeline;
        let path = ObjectPath::from("epochs.json");

        // Test 1: Create new file with first epoch
        {
            let mut conn = store.connect().await.unwrap();
            let batch = Some(EpochCheckpoint {
                checkpoint_number: 100,
            });
            let count = pipeline.commit(&batch, &mut conn).await.unwrap();
            assert_eq!(count, 1);

            let result = conn.object_store().get(&path).await.unwrap();
            let bytes = result.bytes().await.unwrap();
            let epochs: Vec<u64> = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(epochs, vec![100]);
        }

        // Test 2: Append in order
        {
            let mut conn = store.connect().await.unwrap();
            let batch = Some(EpochCheckpoint {
                checkpoint_number: 200,
            });
            pipeline.commit(&batch, &mut conn).await.unwrap();

            let result = conn.object_store().get(&path).await.unwrap();
            let bytes = result.bytes().await.unwrap();
            let epochs: Vec<u64> = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(epochs, vec![100, 200]);
        }

        // Test 3: Insert out of order (should maintain sorted order)
        {
            let mut conn = store.connect().await.unwrap();
            let batch = Some(EpochCheckpoint {
                checkpoint_number: 150,
            });
            pipeline.commit(&batch, &mut conn).await.unwrap();

            let result = conn.object_store().get(&path).await.unwrap();
            let bytes = result.bytes().await.unwrap();
            let epochs: Vec<u64> = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(epochs, vec![100, 150, 200]);
        }

        // Test 4: Duplicate should be ignored (idempotent)
        {
            let mut conn = store.connect().await.unwrap();
            let batch = Some(EpochCheckpoint {
                checkpoint_number: 100,
            });
            let count = pipeline.commit(&batch, &mut conn).await.unwrap();
            assert_eq!(count, 0);

            let result = conn.object_store().get(&path).await.unwrap();
            let bytes = result.bytes().await.unwrap();
            let epochs: Vec<u64> = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(epochs, vec![100, 150, 200]);
        }
    }
}
