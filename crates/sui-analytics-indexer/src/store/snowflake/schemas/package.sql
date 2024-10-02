CREATE OR REPLACE TABLE MOVE_PACKAGE
(
    package_id         STRING        NOT NULL,
    checkpoint         NUMBER(20, 0) NOT NULL,
    epoch              NUMBER(20, 0) NOT NULL,
    timestamp_ms       NUMBER(20, 0) NOT NULL,
    bcs                STRING        NOT NULL,
    transaction_digest STRING
) STAGE_FILE_FORMAT = parquet_format
    STAGE_COPY_OPTIONS =
(
    ABORT_STATEMENT
)
    ENABLE_SCHEMA_EVOLUTION = TRUE
    CLUSTER BY
(
    timestamp_ms
);

// Define the object stage
CREATE OR REPLACE STAGE packages_parquet_stage
    URL = '&{checkpoints_bucket}/move_package/'
    STORAGE_INTEGRATION = checkpoints_data_loader
    FILE_FORMAT = parquet_format;

// Set up the checkpoint auto ingestion pipe
CREATE
    OR REPLACE PIPE package_pipe
    AUTO_INGEST = true
    INTEGRATION = 'CHECKPOINTS_DATA_LOADER_NOTIFICATION'
    AS
        copy into MOVE_PACKAGE (package_id, checkpoint, epoch, timestamp_ms, bcs, transaction_digest)
            from (SELECT t.$1:package_id         as package_id,
                         t.$1:checkpoint         as checkpoint,
                         t.$1:epoch              as epoch,
                         t.$1:timestamp_ms       as timestamp_ms,
                         t.$1:bcs                as bcs,
                         t.$1:transaction_digest as transaction_digest
                  from @packages_parquet_stage (file_format => 'parquet_format', pattern => '.*[.]parquet') t)
            file_format = parquet_format;