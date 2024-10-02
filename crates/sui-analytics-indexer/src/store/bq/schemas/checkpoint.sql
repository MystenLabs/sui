// Define the checkpoint table schema
CREATE TABLE IF NOT EXISTS chaindata.CHECKPOINT
(
    checkpoint_digest                   STRING          NOT NULL,
    sequence_number                     INT64           NOT NULL,
    epoch                               INT64           NOT NULL,
    timestamp_ms                        INT64           NOT NULL,
    previous_checkpoint_digest          STRING,
    end_of_epoch                        BOOL            NOT NULL,
    total_gas_cost                      NUMERIC(20, 0)  NOT NULL,
    computation_cost                    NUMERIC(20, 0)  NOT NULL,
    storage_cost                        NUMERIC(20, 0)  NOT NULL,
    storage_rebate                      NUMERIC(20, 0)  NOT NULL,
    non_refundable_storage_fee          NUMERIC(20, 0)  NOT NULL,
    total_transaction_blocks            NUMERIC(20, 0)  NOT NULL,
    total_transactions                  NUMERIC(20, 0)  NOT NULL,
    total_successful_transaction_blocks NUMERIC(20, 0)  NOT NULL,
    total_successful_transactions       NUMERIC(20, 0)  NOT NULL,
    network_total_transaction           NUMERIC(20, 0)  NOT NULL,
    validator_signature                 STRING          NOT NULL
)
PARTITION BY RANGE_BUCKET(epoch, GENERATE_ARRAY(0, 100000, 10))
CLUSTER BY epoch, sequence_number