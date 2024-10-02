CREATE OR REPLACE TABLE EVENT
(
    transaction_digest STRING        NOT NULL,
    event_index        NUMBER(20, 0) NOT NULL,
    checkpoint         NUMBER(20, 0) NOT NULL,
    epoch              NUMBER(20, 0) NOT NULL,
    timestamp_ms       NUMBER(20, 0) NOT NULL,
    sender             STRING        NOT NULL,
    package            STRING        NOT NULL,
    module             STRING        NOT NULL,
    event_type         STRING        NOT NULL,
    bcs                STRING        NOT NULL,
    event_json         VARIANT
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
CREATE OR REPLACE STAGE events_parquet_stage
    URL = '&{checkpoints_bucket}/events/'
    STORAGE_INTEGRATION = checkpoints_data_loader
    FILE_FORMAT = parquet_format;

// Set up the checkpoint auto ingestion pipe
CREATE
    OR REPLACE PIPE event_pipe
    AUTO_INGEST = true
    INTEGRATION = 'CHECKPOINTS_DATA_LOADER_NOTIFICATION'
    AS
        copy into EVENT (transaction_digest,
                         event_index,
                         checkpoint,
                         epoch,
                         timestamp_ms,
                         sender,
                         package,
                         module,
                         event_type,
                         bcs,
                         event_json)
            from (SELECT t.$1:transaction_digest     as transaction_digest,
                         t.$1:event_index            as event_index,
                         t.$1:checkpoint             as checkpoint,
                         t.$1:epoch                  as epoch,
                         t.$1:timestamp_ms           as timestamp_ms,
                         t.$1:sender                 as sender,
                         t.$1:package                as package,
                         t.$1:module                 as module,
                         t.$1:event_type             as event_type,
                         t.$1:bcs                    as bcs,
                         parse_json(t.$1:event_json) as event_json
                  from @events_parquet_stage (file_format => 'parquet_format', pattern => '.*[.]parquet') t)
            file_format = parquet_format;