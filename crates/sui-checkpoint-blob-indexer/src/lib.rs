// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod handlers;

pub use handlers::{CheckpointBlob, CheckpointBlobPipeline, EpochCheckpoint, EpochsPipeline};

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use object_store::memory::InMemory;
    use object_store::path::Path as ObjectPath;
    use std::sync::Arc;
    use sui_indexer_alt_framework::pipeline::concurrent::Handler;
    use sui_indexer_alt_framework_store_traits::Store;
    use sui_indexer_alt_object_store::ObjectStore;

    #[tokio::test]
    async fn test_checkpoint_blob_handler_uncompressed() {
        let store = ObjectStore::new(Arc::new(InMemory::new()), None);
        let mut conn = store.connect().await.unwrap();

        let blob = CheckpointBlob {
            sequence_number: 100,
            proto_bytes: Bytes::from(vec![1, 2, 3, 4, 5]),
        };

        let pipeline = CheckpointBlobPipeline {
            compression_level: None,
        };
        let count = pipeline.commit(&Some(blob), &mut conn).await.unwrap();
        assert_eq!(count, 1);

        let path = ObjectPath::from("100.binpb");
        let result = conn.object_store().get(&path).await.unwrap();
        let bytes = result.bytes().await.unwrap();
        assert_eq!(bytes.as_ref(), &[1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn test_checkpoint_blob_handler_compressed() {
        let store = ObjectStore::new(Arc::new(InMemory::new()), None);
        let mut conn = store.connect().await.unwrap();

        let test_data = vec![0u8; 1000];
        let blob = CheckpointBlob {
            sequence_number: 200,
            proto_bytes: Bytes::from(test_data.clone()),
        };

        let pipeline = CheckpointBlobPipeline {
            compression_level: Some(3),
        };
        let count = pipeline.commit(&Some(blob), &mut conn).await.unwrap();
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
        let store = ObjectStore::new(Arc::new(InMemory::new()), None);
        let path = ObjectPath::from("epochs.json");

        // Test 1: Create new file with first epoch
        {
            let mut conn = store.connect().await.unwrap();
            let epoch = EpochCheckpoint {
                checkpoint_number: 100,
            };
            let pipeline = EpochsPipeline;
            let count = pipeline.commit(&Some(epoch), &mut conn).await.unwrap();
            assert_eq!(count, 1);

            let result = conn.object_store().get(&path).await.unwrap();
            let bytes = result.bytes().await.unwrap();
            let epochs: Vec<u64> = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(epochs, vec![100]);
        }

        // Test 2: Append in order
        {
            let mut conn = store.connect().await.unwrap();
            let epoch = EpochCheckpoint {
                checkpoint_number: 200,
            };
            let pipeline = EpochsPipeline;
            pipeline.commit(&Some(epoch), &mut conn).await.unwrap();

            let result = conn.object_store().get(&path).await.unwrap();
            let bytes = result.bytes().await.unwrap();
            let epochs: Vec<u64> = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(epochs, vec![100, 200]);
        }

        // Test 3: Insert out of order (should maintain sorted order)
        {
            let mut conn = store.connect().await.unwrap();
            let epoch = EpochCheckpoint {
                checkpoint_number: 150,
            };
            let pipeline = EpochsPipeline;
            pipeline.commit(&Some(epoch), &mut conn).await.unwrap();

            let result = conn.object_store().get(&path).await.unwrap();
            let bytes = result.bytes().await.unwrap();
            let epochs: Vec<u64> = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(epochs, vec![100, 150, 200]);
        }

        // Test 4: Duplicate should be ignored (idempotent)
        {
            let mut conn = store.connect().await.unwrap();
            let epoch = EpochCheckpoint {
                checkpoint_number: 100,
            };
            let pipeline = EpochsPipeline;
            let count = pipeline.commit(&Some(epoch), &mut conn).await.unwrap();
            assert_eq!(count, 0);

            let result = conn.object_store().get(&path).await.unwrap();
            let bytes = result.bytes().await.unwrap();
            let epochs: Vec<u64> = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(epochs, vec![100, 150, 200]);
        }
    }
}
