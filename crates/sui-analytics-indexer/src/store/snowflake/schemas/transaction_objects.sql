CREATE OR REPLACE TABLE TRANSACTION_OBJECT
(
    object_id          STRING        NOT NULL,
    version            NUMBER(20, 0),
    transaction_digest STRING        NOT NULL,
    checkpoint         NUMBER(20, 0) NOT NULL,
    epoch              NUMBER(20, 0) NOT NULL,
    timestamp_ms       NUMBER(20, 0) NOT NULL,
    input_kind         STRING,
    object_status      STRING
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
CREATE OR REPLACE STAGE transaction_objects_parquet_stage
    URL = '&{checkpoints_bucket}/transaction_objects/'
    STORAGE_INTEGRATION = checkpoints_data_loader
    FILE_FORMAT = parquet_format;

// Set up the checkpoint auto ingestion pipe
CREATE
    OR REPLACE PIPE transaction_object_pipe
    AUTO_INGEST = true
    INTEGRATION = 'CHECKPOINTS_DATA_LOADER_NOTIFICATION'
    AS
        copy into TRANSACTION_OBJECT (object_id, version, transaction_digest, checkpoint, epoch, timestamp_ms,
                                      input_kind, object_status)
            from (SELECT t.$1:object_id          as object_id,
                         t.$1:version            as version,
                         t.$1:transaction_digest as transaction_digest,
                         t.$1:checkpoint         as checkpoint,
                         t.$1:epoch              as epoch,
                         t.$1:timestamp_ms       as timestamp_ms,
                         t.$1:input_kind         as input_kind,
                         t.$1:object_status      as object_status
                  from @transaction_objects_parquet_stage (file_format => 'parquet_format', pattern => '.*[.]parquet') t)
            file_format = parquet_format;