
CREATE OR REPLACE TABLE MOVE_CALL(
                                     transaction_digest STRING NOT NULL,
                                     checkpoint NUMBER(20, 0) NOT NULL,
                                     epoch NUMBER(20, 0) NOT NULL,
                                     timestamp_ms NUMBER(20, 0) NOT NULL,
                                     package STRING NOT NULL,
                                     module STRING NOT NULL,
                                     function_ STRING NOT NULL
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
CREATE OR REPLACE STAGE move_call_parquet_stage
    URL = '&{checkpoints_bucket}/move_call/'
    STORAGE_INTEGRATION = checkpoints_data_loader
    FILE_FORMAT = parquet_format;

// Set up the checkpoint auto ingestion pipe
CREATE
    OR REPLACE PIPE move_call_pipe
    AUTO_INGEST = true
    INTEGRATION = 'CHECKPOINTS_DATA_LOADER_NOTIFICATION'
    AS
        copy into MOVE_CALL (transaction_digest,
                         checkpoint,
                         epoch,
                         timestamp_ms,
                         package,
                         module,
                         function_)
            from (SELECT t.$1:transaction_digest     as transaction_digest,
                         t.$1:checkpoint             as checkpoint,
                         t.$1:epoch                  as epoch,
                         t.$1:timestamp_ms           as timestamp_ms,
                         t.$1:package                as package,
                         t.$1:module                 as module,
                         t.$1:function_              as function_
                  from @move_call_parquet_stage (file_format => 'parquet_format', pattern => '.*[.]parquet') t)
            file_format = parquet_format;