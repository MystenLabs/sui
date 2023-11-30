CREATE OR REPLACE TABLE TRANSACTION
(
    transaction_digest         STRING        NOT NULL,
    checkpoint                 NUMBER(20, 0) NOT NULL,
    epoch                      NUMBER(20, 0) NOT NULL,
    timestamp_ms               NUMBER(20, 0) NOT NULL,
    sender                     STRING        NOT NULL,
    transaction_kind           STRING        NOT NULL,
    is_system_txn              BOOLEAN       NOT NULL,
    is_sponsored_tx            BOOLEAN       NOT NULL,
    transaction_count          NUMBER(20, 0) NOT NULL,
    execution_success          BOOLEAN       NOT NULL,
    input                      NUMBER(20, 0) NOT NULL,
    shared_input               NUMBER(20, 0) NOT NULL,
    gas_coins                  NUMBER(20, 0) NOT NULL,
    created                    NUMBER(20, 0) NOT NULL,
    mutated                    NUMBER(20, 0) NOT NULL,
    deleted                    NUMBER(20, 0) NOT NULL,
    transfers                  NUMBER(20, 0) NOT NULL,
    split_coins                NUMBER(20, 0) NOT NULL,
    merge_coins                NUMBER(20, 0) NOT NULL,
    publish                    NUMBER(20, 0) NOT NULL,
    upgrade                    NUMBER(20, 0) NOT NULL,
    others                     NUMBER(20, 0) NOT NULL,
    move_calls                 NUMBER(20, 0) NOT NULL,
    packages                   STRING,
    gas_owner                  STRING        NOT NULL,
    gas_object_id              STRING        NOT NULL,
    gas_object_sequence        NUMBER(20, 0) NOT NULL,
    gas_object_digest          STRING        NOT NULL,
    gas_budget                 NUMBER(20, 0) NOT NULL,
    total_gas_cost             NUMBER(20, 0) NOT NULL,
    computation_cost           NUMBER(20, 0) NOT NULL,
    storage_cost               NUMBER(20, 0) NOT NULL,
    storage_rebate             NUMBER(20, 0) NOT NULL,
    non_refundable_storage_fee NUMBER(20, 0) NOT NULL,
    gas_price                  NUMBER(20, 0) NOT NULL,
    raw_transaction            STRING        NOT NULL,
    has_zklogin_sig            BOOLEAN,
    has_upgraded_multisig      BOOLEAN
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

// Define the transaction stage
CREATE OR REPLACE STAGE transaction_parquet_stage
    URL = '&{checkpoints_bucket}/transactions/'
    STORAGE_INTEGRATION = checkpoints_data_loader
    FILE_FORMAT = parquet_format;

// Set up the checkpoint auto ingestion pipe
CREATE
    OR REPLACE PIPE transaction_pipe
    AUTO_INGEST = true
    INTEGRATION = 'CHECKPOINTS_DATA_LOADER_NOTIFICATION'
    AS
        COPY INTO TRANSACTION (transaction_digest, checkpoint, epoch, timestamp_ms, sender,
                               transaction_kind, is_system_txn, is_sponsored_tx, transaction_count, execution_success,
                               input, shared_input, gas_coins,
                               created, mutated, deleted, transfers, split_coins, merge_coins, publish, upgrade, others,
                               move_calls, packages, gas_owner, gas_object_id, gas_object_sequence, gas_object_digest,
                               gas_budget, total_gas_cost, computation_cost, storage_cost, storage_rebate,
                               non_refundable_storage_fee,
                               gas_price, raw_transaction, has_zklogin_sig, has_upgraded_multisig
            )
            from (SELECT t.$1:transaction_digest         as transaction_digest,
                         t.$1:checkpoint                 as checkpoint,
                         t.$1:epoch                      as epoch,
                         t.$1:timestamp_ms               as timestamp_ms,
                         t.$1:sender                     as sender,
                         t.$1:transaction_kind           as transaction_kind,
                         t.$1:is_system_txn              as is_system_txn,
                         t.$1:is_sponsored_tx            as is_sponsored_tx,
                         t.$1:transaction_count          as transaction_count,
                         t.$1:execution_success          as execution_success,
                         t.$1:input                      as input,
                         t.$1:shared_input               as shared_input,
                         t.$1:gas_coins                  as gas_coins,
                         t.$1:created                    as created,
                         t.$1:mutated                    as mutated,
                         t.$1:deleted                    as deleted,
                         t.$1:transfers                  as transfers,
                         t.$1:split_coins                as split_coins,
                         t.$1:merge_coins                as merge_coins,
                         t.$1:publish                    as publish,
                         t.$1:upgrade                    as upgrade,
                         t.$1:others                     as others,
                         t.$1:move_calls                 as move_calls,
                         t.$1:packages                   as packages,
                         t.$1:gas_owner                  as gas_owner,
                         t.$1:gas_object_id              as gas_object_id,
                         t.$1:gas_object_sequence        as gas_object_sequence,
                         t.$1:gas_object_digest          as gas_object_digest,
                         t.$1:gas_budget                 as gas_budget,
                         t.$1:total_gas_cost             as total_gas_cost,
                         t.$1:computation_cost           as computation_cost,
                         t.$1:storage_cost               as storage_cost,
                         t.$1:storage_rebate             as storage_rebate,
                         t.$1:non_refundable_storage_fee as non_refundable_storage_fee,
                         t.$1:gas_price                  as gas_price,
                         t.$1:raw_transaction            as raw_transaction,
                         t.$1:has_zklogin_sig            as has_zklogin_sig,
                         t.$1:has_upgraded_multisig      as has_upgraded_multisig
                  from @transaction_parquet_stage (file_format => 'parquet_format', pattern => '.*[.]parquet') t)
            file_format = parquet_format;