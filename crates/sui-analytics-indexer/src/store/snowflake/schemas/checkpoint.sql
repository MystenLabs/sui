// Define the checkpoint table schema
CREATE
    OR REPLACE TABLE CHECKPOINT
(
    checkpoint_digest                   STRING
        CONSTRAINT checkpoint_digest_unique UNIQUE    NOT NULL,
    sequence_number                     NUMBER(20, 0)
        CONSTRAINT sequence_num_pk PRIMARY KEY        NOT NULL,
    epoch                               NUMBER(20, 0) NOT NULL,
    timestamp_ms                        NUMBER(20, 0) NOT NULL,
    previous_checkpoint_digest          STRING,
    end_of_epoch                        BOOLEAN       NOT NULL,
    total_gas_cost                      NUMBER(20, 0) NOT NULL,
    computation_cost                    NUMBER(20, 0) NOT NULL,
    storage_cost                        NUMBER(20, 0) NOT NULL,
    storage_rebate                      NUMBER(20, 0) NOT NULL,
    non_refundable_storage_fee          NUMBER(20, 0) NOT NULL,
    total_transaction_blocks            NUMBER(20, 0) NOT NULL,
    total_transactions                  NUMBER(20, 0) NOT NULL,
    total_successful_transaction_blocks NUMBER(20, 0) NOT NULL,
    total_successful_transactions       NUMBER(20, 0) NOT NULL,
    network_total_transaction           NUMBER(20, 0) NOT NULL,
    validator_signature                 STRING        NOT NULL
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

// Define the checkpoint stage
CREATE OR REPLACE STAGE checkpoints_parquet_stage
    URL = '&{checkpoints_bucket}/checkpoints/'
    STORAGE_INTEGRATION = checkpoints_data_loader
    FILE_FORMAT = parquet_format;

// Set up the checkpoint auto ingestion pipe
CREATE
    OR REPLACE PIPE checkpoint_pipe
    AUTO_INGEST = true
    INTEGRATION = 'CHECKPOINTS_DATA_LOADER_NOTIFICATION'
    AS
        COPY INTO CHECKPOINT (checkpoint_digest, sequence_number, epoch, timestamp_ms, previous_checkpoint_digest,
                              end_of_epoch, total_gas_cost, computation_cost, storage_cost, storage_rebate,
                              non_refundable_storage_fee, total_transaction_blocks, total_transactions,
                              total_successful_transaction_blocks, total_successful_transactions,
                              network_total_transaction, validator_signature)
            from (SELECT t.$1:checkpoint_digest                   as checkpoint_digest,
                         t.$1:sequence_number                     as sequence_number,
                         t.$1:epoch                               as epoch,
                         t.$1:timestamp_ms                        as timestamp_ms,
                         t.$1:previous_checkpoint_digest          as previous_checkpoint_digest,
                         t.$1:end_of_epoch                        as end_of_epoch,
                         t.$1:total_gas_cost                      as total_gas_cost,
                         t.$1:computation_cost                    as computation_cost,
                         t.$1:storage_cost                        as storage_cost,
                         t.$1:storage_rebate                      as storage_rebate,
                         t.$1:non_refundable_storage_fee          as non_refundable_storage_fee,
                         t.$1:total_transaction_blocks            as total_transaction_blocks,
                         t.$1:total_transactions                  as total_transactions,
                         t.$1:total_successful_transaction_blocks as total_successful_transaction_blocks,
                         t.$1:total_successful_transactions       as total_successful_transactions,
                         t.$1:network_total_transaction           as network_total_transaction,
                         t.$1:validator_signature                 as validator_signature
                  from @checkpoints_parquet_stage (file_format => 'parquet_format', pattern => '.*[.]parquet') t)
            file_format = parquet_format;