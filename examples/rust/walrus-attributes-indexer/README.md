# Walrus Attributes Indexer

This is an extention of the [Custom Indexer guide](https://docs.sui.io/guides/developer/advanced/custom-indexer) to show how to index Walrus blobs and their associated `Metadata` dynamic fields.

## Quickstart

Given a service that allows users to upload blog posts to Walrus and creates an associated `Metadata` dynamic field with `view_count`, `title`, and `publisher` (Sui address that created the Walrus blob), you can write a corresponding indexer that commits these attributes to a store of your choice to emulate a blog post platform. Then, users can:
- Upload blog posts with titles
- View their own posts and metrics
- Delete posts they created
- Edit post titles
- Browse posts by other publishers


To run the indexer:

```sh
RUST_LOG=info cargo run --release -- \
    --remote-store-url https://checkpoints.mainnet.sui.io
```

Other useful commands:
```sh
# Get the status of a blob, such as its expiry epoch, when it was initially certified, etc.
walrus blob-status --blob-id {BLOB_ID}
# List all blobs for the current address, including expired ones.
walrus list-blobs --include-expired
# Set a path: value attribute pair on the Metadata dynamic field of a Blob object on Sui.
walrus set-blob-attribute {Sui blob object id} --attr "title" {title} --attr "view_count" {view_count}
```

```sh
# Creates a database and sets up the __diesel_schema_migrations table. Does not run any migrations.
diesel setup                                                                \
    --database-url=... \
    --migration-dir migrations
# Applies all pending migrations and updates the __diesel_schema_migrations table.
diesel migration run                                                        \
    --database-url=... \
    --migration-dir migrations
# Drops the entire database and recreates it from scratch by running all migrations from the beginning. Deletes all existing data.
diesel database reset --database-url=... --migration-dir migrations
```

## Blog Post Pipeline

The Blog Post pipeline is a sequential pipeline that writes the latest state of the `Metadata` dynamic fields to the `blog_post` table. It operates on a checkpoint granularity, and upserts records such that only the final update to an object in a checkpoint is persisted.

## Chain-agnostic Indexer

For the purpose of this guide, the StructTag of the `Metadata` dynamic field is hardcoded in `main.rs`. Ideally, in a production deployment, this should be a value that is passed to the service.

## Defaults

As of writing, the SequentialConfig is defined [here](https://github.com/MystenLabs/sui/blob/main/crates/sui-indexer-alt-framework/src/pipeline/sequential/mod.rs#L68) consisting of a committer config and a checkpoint lag. The default values set `checkpoint_lag` to 0, and the committer config as follows:
```
/// Configuration for a sequential pipeline
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct SequentialConfig {
    /// Configuration for the writer, that makes forward progress.
    pub committer: CommitterConfig,

    /// How many checkpoints to hold back writes for.
    pub checkpoint_lag: u64,
}

// Defaults
impl Default for CommitterConfig {
    fn default() -> Self {
        Self {
            write_concurrency: 5,
            collect_interval_ms: 500,
            watermark_interval_ms: 500,
        }
    }
}
```

The ingestion config is defined [here](https://github.com/MystenLabs/sui/blob/main/crates/sui-indexer-alt-framework/src/ingestion/mod.rs#L59) with defaults configured to:
```
impl Default for IngestionConfig {
    fn default() -> Self {
        Self {
            checkpoint_buffer_size: 5000,
            ingest_concurrency: 200,
            retry_interval_ms: 200,
        }
    }
}
```

This means that by default, the blog post pipeline will have a write concurrency of 5, and the regulator will buffer at most 5000 checkpoints from the latest checkpoint committed by the blog post pipeline.

## Follow-Along
The following uploads the `blog_post.rs` file to Walrus, and runs the indexer locally with `--last-checkpoint` to verify that the indexer is working correctly.

```
walrus store src/handlers/blog_post.rs

# Blob ID: IPYp_WbBwnNRTqeiYtvA6VQ0XUkS6m3ActV-0PIQfjQ
# Sui object ID: 0xcfb3d474c9a510fde93262d4b7de66cad62a2005a54f31a63e96f3033f465ed3

# Checkpoint 178907908
walrus set-blob-attribute 0xcfb3d474c9a510fde93262d4b7de66cad62a2005a54f31a63e96f3033f465ed3 --attr view_count 5 --attr title "Blog post module" --attr publisher "0xfe9c7a465f63388e5b95c8fd2db857fad4356fc873f96900f4d8b6e7fc1e760e"

walrus get-blob-attribute 0xcfb3d474c9a510fde93262d4b7de66cad62a2005a54f31a63e96f3033f465ed3
# Attribute
# view_count: 5
# title: Blog post module
# blob_id: IPYp_WbBwnNRTqeiYtvA6VQ0XUkS6m3ActV-0PIQfjQ
# publisher: 0xfe9c7a465f63388e5b95c8fd2db857fad4356fc873f96900f4d8b6e7fc1e760e
```

Attributes then modified again at 178908405 and 178908459

You should ultimately see something like:
```
                          dynamic_field_id                          | df_version |                             publisher                              |                            blob_obj_id                             | view_count |      title
--------------------------------------------------------------------+------------+--------------------------------------------------------------------+--------------------------------------------------------------------+------------+------------------
 \x40b5ae12e780ae815d7b0956281291253c02f227657fe2b7a8ccf003a5f597f7 |  608253371 | \xfe9c7a465f63388e5b95c8fd2db857fad4356fc873f96900f4d8b6e7fc1e760e | \xcfb3d474c9a510fde93262d4b7de66cad62a2005a54f31a63e96f3033f465ed3 |         10 | Blog Post Module
 ```
