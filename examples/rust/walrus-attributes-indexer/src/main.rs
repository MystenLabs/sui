// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use sui_indexer_alt_framework::{
    cluster::{self, IndexerClusterBuilder},
    pipeline::sequential::SequentialConfig,
    Result,
};
use url::Url;
use walrus_attributes_indexer::{handlers::BlogPostPipeline, MIGRATIONS};

// Indexers should be chain agnostic, so in a production deployment, this should be a value that
// is passed to the service, rather than hardcoded here.
const METADATA_DYNAMIC_FIELD_TYPE: &str =
    "0x2::dynamic_field::Field<vector<u8>, 0xfdc88f7d7cf30afab2f82e8380d11ee8f70efb90e863d1de8616fae1bb09ea77::metadata::Metadata>";

#[derive(clap::Parser, Debug)]
struct Args {
    #[clap(
        long,
        default_value = "postgres://postgres:postgrespw@localhost:5432/walrus_attributes"
    )]
    database_url: Url,

    #[clap(flatten)]
    cluster_args: cluster::Args,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // The `IndexerClusterBuilder` offers a convenient way to quickly set up an `IndexerCluster`,
    // which consists of the base indexer, metrics service, and a cancellation token.
    let mut indexer = IndexerClusterBuilder::new()
        .with_database_url(args.database_url)
        .with_args(args.cluster_args)
        .with_migrations(&MIGRATIONS)
        .build()
        .await?;

    let blog_post_pipeline = BlogPostPipeline::new(METADATA_DYNAMIC_FIELD_TYPE).unwrap();

    // Other pipelines can be easily added with `.sequential_pipeline()` or
    // `.concurrent_pipeline()`.
    indexer
        .sequential_pipeline(blog_post_pipeline, SequentialConfig::default())
        .await?;

    let _ = indexer.run().await?.await;
    Ok(())
}
