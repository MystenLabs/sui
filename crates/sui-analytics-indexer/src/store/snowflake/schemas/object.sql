CREATE OR REPLACE TABLE OBJECT
(
    object_id              STRING        NOT NULL,
    version                NUMBER(20, 0) NOT NULL,
    digest                 STRING        NOT NULL,
    type                   STRING,
    checkpoint             NUMBER(20, 0) NOT NULL,
    epoch                  NUMBER(20, 0) NOT NULL,
    timestamp_ms           NUMBER(20, 0) NOT NULL,
    owner_type             STRING        NOT NULL,
    owner_address          STRING,
    object_status          STRING,
    initial_shared_version NUMBER(20, 0),
    previous_transaction   STRING        NOT NULL,
    has_public_transfer    BOOLEAN       NOT NULL,
    storage_rebate         NUMBER(20, 0) NOT NULL,
    bcs                    STRING        NOT NULL,
    coin_type              STRING,
    coin_balance           NUMBER(20, 0),
    struct_tag             STRING,
    object_json            variant
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
CREATE OR REPLACE STAGE objects_parquet_stage
    URL = '&{checkpoints_bucket}/objects/'
    STORAGE_INTEGRATION = checkpoints_data_loader
    FILE_FORMAT = parquet_format;

// Set up the checkpoint auto ingestion pipe
CREATE
    OR REPLACE PIPE object_pipe
    AUTO_INGEST = true
    INTEGRATION = 'CHECKPOINTS_DATA_LOADER_NOTIFICATION'
    AS
        copy into OBJECT (object_id, version, digest, type, checkpoint, epoch, timestamp_ms, owner_type, owner_address,
                          object_status, initial_shared_version, previous_transaction, has_public_transfer,
                          storage_rebate, bcs, coin_type, coin_balance, struct_tag, object_json)
            from (SELECT t.$1:object_id               as object_id,
                         t.$1:version                 as version,
                         t.$1:digest                  as digest,
                         t.$1:type_                   as type,
                         t.$1:checkpoint              as checkpoint,
                         t.$1:epoch                   as epoch,
                         t.$1:timestamp_ms            as timestamp_ms,
                         t.$1:owner_type              as owner_type,
                         t.$1:owner_address           as owner_address,
                         t.$1:object_status           as object_status,
                         t.$1:initial_shared_version  as initial_shared_version,
                         t.$1:previous_transaction    as previous_transaction,
                         t.$1:has_public_transfer     as has_public_transfer,
                         t.$1:storage_rebate          as storage_rebate,
                         t.$1:bcs                     as bcs,
                         t.$1:coin_type               as coin_type,
                         t.$1:coin_balance            as coin_balance,
                         t.$1:struct_tag              as struct_tag,
                         parse_json(t.$1:object_json) as object_json
                  from @objects_parquet_stage (file_format => 'parquet_format', pattern => '.*[.]parquet') t)
            file_format = parquet_format;