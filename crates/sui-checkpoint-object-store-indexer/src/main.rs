use std::sync::{Arc, LazyLock};

use anyhow::Context;
use prost::Message;
use sui_indexer_alt_framework::pipeline::concurrent::Handler;
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
impl Handler for CheckpointBlobIndexer {
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

#[derive(clap::Parser)]
#[command(name = "sui-checkpoint-object-store-indexer")]
#[command(about = "Indexer that writes checkpoints as compressed proto blobs to object storage")]
struct Args {
    /// PostgreSQL database URL for watermark tracking
    #[arg(long, env = "DATABASE_URL")]
    database_url: url::Url,

    /// Object store URL for checkpoint storage
    /// Supports: file://, gs://, s3://, azure://
    /// Examples:
    ///   file:///tmp/checkpoints
    ///   gs://bucket-name/path
    ///   s3://bucket-name/path
    #[arg(long, env = "OBJECT_STORE_URL")]
    object_store_url: url::Url,

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

fn create_object_store(url: &url::Url) -> anyhow::Result<Box<dyn object_store::ObjectStore>> {
    match url.scheme() {
        "file" => {
            let path = url.path();
            Ok(Box::new(
                object_store::local::LocalFileSystem::new_with_prefix(path)?,
            ))
        }
        "gs" => {
            let store = object_store::gcp::GoogleCloudStorageBuilder::from_env()
                .with_url(url.as_str())
                .build()
                .context("Failed to create GCS object store")?;
            Ok(Box::new(store))
        }
        "s3" => {
            let store = object_store::aws::AmazonS3Builder::from_env()
                .with_url(url.as_str())
                .build()
                .context("Failed to create S3 object store")?;
            Ok(Box::new(store))
        }
        "azure" | "az" => {
            let store = object_store::azure::MicrosoftAzureBuilder::from_env()
                .with_url(url.as_str())
                .build()
                .context("Failed to create Azure object store")?;
            Ok(Box::new(store))
        }
        scheme => anyhow::bail!("Unsupported object store scheme: {}", scheme),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use clap::Parser;
    use sui_indexer_alt_framework::{
        ingestion::{ClientArgs, IngestionConfig},
        pipeline::concurrent::ConcurrentConfig,
    };
    use sui_pg_db::DbArgs;

    let args = Args::parse();

    tracing_subscriber::fmt::init();

    let db = sui_pg_db::Db::for_write(args.database_url.clone(), DbArgs::default()).await?;

    // Run framework migrations (creates watermarks table, etc.)
    db.run_migrations(None)
        .await
        .context("Failed to run database migrations")?;

    let object_store =
        create_object_store(&args.object_store_url).context("Failed to create object store")?;

    let store = ObjectStore::new(db, object_store, args.compression_level);

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
