// set checkpoints_bucket as a variable or pass it in the command line
// This sets up an external stage backed by a gcs bucket used for reading checkpoint data from
CREATE STORAGE INTEGRATION checkpoints_data_loader
    TYPE = EXTERNAL_STAGE
    STORAGE_PROVIDER = GCS
    ENABLED = TRUE
    STORAGE_ALLOWED_LOCATIONS = ('&{checkpoints_bucket}/checkpoints', '&{checkpoints_bucket}/events','&{checkpoints_bucket}/move_call.sql','&{checkpoints_bucket}/move_package','&{checkpoints_bucket}/objects','&{checkpoints_bucket}/transaction_objects','&{checkpoints_bucket}/transactions');

// This sets up pubsub_subscription_id as the pubsub topic subscriber id
CREATE NOTIFICATION INTEGRATION checkpoints_data_loader_notification
    TYPE = QUEUE
    NOTIFICATION_PROVIDER = GCP_PUBSUB
    ENABLED = true
    GCP_PUBSUB_SUBSCRIPTION_NAME = '&{pubsub_subscription_id}';

// This sets up the parquet file format used for all queries
CREATE OR REPLACE File Format parquet_format
    TYPE = parquet
    SNAPPY_COMPRESSION = TRUE;