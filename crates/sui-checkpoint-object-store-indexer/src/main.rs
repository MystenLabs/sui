use std::sync::{Arc, LazyLock};

use anyhow::Context;
use bytes::Bytes;
use object_store::path::Path as StorePath;
use prost::Message;
use sui_config::object_storage_config::ObjectStoreConfig;
use sui_indexer_alt_framework::pipeline::concurrent;
use sui_indexer_alt_framework::store::Store;
use sui_indexer_alt_framework::{FieldCount, Indexer, IndexerArgs, pipeline::Processor};
use sui_indexer_object_store::ObjectStore;
use sui_rpc::field::{FieldMask, FieldMaskUtil};
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc;
use sui_types::full_checkpoint_content::Checkpoint;

#[cfg(unix)]
async fn signal_terminate() {
    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("Failed to install SIGTERM handler")
        .recv()
        .await;
}

#[cfg(not(unix))]
async fn signal_terminate() {
    std::future::pending::<()>().await;
}

#[derive(FieldCount)]
pub struct CheckpointBlob {
    pub sequence_number: u64,
    pub proto_bytes: Vec<u8>,
}

#[derive(FieldCount)]
pub struct EpochBoundary {
    pub checkpoint: u64,
}

pub struct CheckpointBlobIndexer;

#[async_trait::async_trait]
impl Processor for CheckpointBlobIndexer {
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
        let proto_bytes = proto_checkpoint.encode_to_vec();

        Ok(vec![CheckpointBlob {
            sequence_number,
            proto_bytes,
        }])
    }
}

#[async_trait::async_trait]
impl concurrent::Handler for CheckpointBlobIndexer {
    type Store = ObjectStore;

    async fn commit<'a>(
        values: &[CheckpointBlob],
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> anyhow::Result<usize> {
        for blob in values {
            let path = format!("{}.pb", blob.sequence_number);
            conn.write(path, &blob.proto_bytes).await?;
        }
        Ok(values.len())
    }
}

pub struct EpochBoundaryIndexer;

#[async_trait::async_trait]
impl Processor for EpochBoundaryIndexer {
    const NAME: &'static str = "epoch_boundary";
    type Value = EpochBoundary;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        if checkpoint.summary.end_of_epoch_data.is_some() {
            Ok(vec![EpochBoundary {
                checkpoint: checkpoint.summary.sequence_number,
            }])
        } else {
            Ok(vec![])
        }
    }
}

#[async_trait::async_trait]
impl concurrent::Handler for EpochBoundaryIndexer {
    type Store = ObjectStore;

    async fn commit<'a>(
        values: &[EpochBoundary],
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> anyhow::Result<usize> {
        if values.is_empty() {
            return Ok(0);
        }

        let path = StorePath::from("epochs.json");

        // Read existing epochs.json
        let mut epochs: Vec<u64> = match conn.object_store().get(&path).await {
            Ok(result) => {
                let bytes = result.bytes().await?;
                serde_json::from_slice(&bytes)?
            }
            Err(object_store::Error::NotFound { .. }) => vec![],
            Err(e) => return Err(e.into()),
        };

        // Add new epoch boundaries
        for boundary in values {
            if !epochs.contains(&boundary.checkpoint) {
                epochs.push(boundary.checkpoint);
            }
        }

        epochs.sort_unstable();
        epochs.dedup();

        // Write back
        let json = serde_json::to_vec(&epochs)?;
        conn.object_store()
            .put(&path, Bytes::from(json).into())
            .await?;

        tracing::info!(
            boundaries = values.len(),
            "Updated epochs.json with epoch boundaries"
        );

        Ok(values.len())
    }
}

#[derive(clap::Parser)]
#[command(name = "sui-checkpoint-object-store-indexer")]
#[command(about = "Indexer that writes checkpoints as compressed proto blobs to object storage")]
struct Args {
    #[command(flatten)]
    object_store_config: ObjectStoreConfig,

    /// gRPC API URL to fetch checkpoints from
    #[arg(long, env = "RPC_API_URL")]
    rpc_api_url: url::Url,

    /// Optional username for gRPC authentication
    #[arg(long, env = "RPC_USERNAME")]
    rpc_username: Option<String>,

    /// Optional password for gRPC authentication
    #[arg(long, env = "RPC_PASSWORD")]
    rpc_password: Option<String>,

    /// Optional Zstd compression level. If not provided, data will be stored uncompressed
    #[arg(long)]
    compression_level: Option<i32>,

    #[command(flatten)]
    indexer_args: IndexerArgs,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use clap::Parser;
    use sui_indexer_alt_framework::{
        ingestion::{ClientArgs, IngestionConfig},
        pipeline::concurrent::ConcurrentConfig,
    };

    let args = Args::parse();

    tracing_subscriber::fmt::init();

    let object_store = args
        .object_store_config
        .make()
        .context("Failed to create object store")?;

    let store = ObjectStore::new(object_store, args.compression_level);

    let client_args = ClientArgs {
        rpc_api_url: Some(args.rpc_api_url),
        rpc_username: args.rpc_username,
        rpc_password: args.rpc_password,
        remote_store_url: None,
        local_ingestion_path: None,
    };

    let registry = prometheus::Registry::new();
    let cancel = tokio_util::sync::CancellationToken::new();

    let mut indexer = Indexer::new(
        store,
        args.indexer_args,
        client_args,
        IngestionConfig::default(),
        Some("checkpoint_indexer"),
        &registry,
        cancel.clone(),
    )
    .await?;

    indexer
        .concurrent_pipeline(CheckpointBlobIndexer, ConcurrentConfig::default())
        .await?;

    // Use write_concurrency=1 to ensure serial writes (no read-modify-write races)
    indexer
        .concurrent_pipeline(
            EpochBoundaryIndexer,
            ConcurrentConfig {
                committer: sui_indexer_alt_framework::pipeline::CommitterConfig {
                    write_concurrency: 1,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await?;

    let handle = indexer.run().await?;

    tokio::select! {
        _ = handle => {},
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received SIGINT, shutting down...");
            cancel.cancel();
        }
        _ = signal_terminate() => {
            tracing::info!("Received SIGTERM, shutting down...");
            cancel.cancel();
        }
    }

    Ok(())
}
