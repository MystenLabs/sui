CREATE OR REPLACE TABLE WRAPPED_STRUCT
(
    object_id              STRING,
    root_object_id         STRING        NOT NULL,
    root_object_version    NUMBER(20, 0) NOT NULL,
    checkpoint             NUMBER(20, 0) NOT NULL,
    epoch                  NUMBER(20, 0) NOT NULL,
    timestamp_ms           NUMBER(20, 0) NOT NULL,
    json_path              STRING        NOT NULL,
    struct_tag             STRING
) STAGE_FILE_FORMAT = parquet_format
    STAGE_COPY_OPTIONS =
(
    ABORT_STATEMENT
)
    ENABLE_SCHEMA_EVOLUTION = TRUE
    CLUSTER BY
(
    root_object_id, root_object_version, json_path
);

// Define the wrapped object stage
CREATE OR REPLACE STAGE wrapped_struct_parquet_stage
    URL = '&{checkpoints_bucket}/wrapped_object/'
    STORAGE_INTEGRATION = checkpoints_data_loader
    FILE_FORMAT = parquet_format;

// Set up the checkpoint auto ingestion pipe
CREATE
    OR REPLACE PIPE wrapped_struct_pipe
    AUTO_INGEST = true
    INTEGRATION = 'CHECKPOINTS_DATA_LOADER_NOTIFICATION'
    AS
        copy into WRAPPED_STRUCT (object_id, root_object_id, root_object_version, checkpoint, epoch, timestamp_ms, json_path, struct_tag)
            from (SELECT t.$1:object_id               as object_id,
                         t.$1:root_object_id          as root_object_id,
                         t.$1:root_object_version     as root_object_version,
                         t.$1:checkpoint              as checkpoint,
                         t.$1:epoch                   as epoch,
                         t.$1:timestamp_ms            as timestamp_ms,
                         t.$1:json_path               as json_path,
                         t.$1:struct_tag              as struct_tag
                  from @wrapped_struct_parquet_stage (file_format => 'parquet_format', pattern => '.*[.]parquet') t)
            file_format = parquet_format;