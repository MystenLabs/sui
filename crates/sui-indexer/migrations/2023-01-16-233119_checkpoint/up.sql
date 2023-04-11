CREATE TABLE checkpoints
(
    sequence_number            BIGINT       PRIMARY KEY,
    checkpoint_digest          VARCHAR(255) NOT NULL,
    epoch                      BIGINT       NOT NULL,
    transactions               TEXT[]       NOT NULL,
    previous_checkpoint_digest VARCHAR(255),
    end_of_epoch               BOOLEAN      NOT NULL,
    -- derived from GasCostSummary
    total_gas_cost             BIGINT       NOT NULL,
    total_computation_cost     BIGINT       NOT NULL,
    total_storage_cost         BIGINT       NOT NULL,
    total_storage_rebate       BIGINT       NOT NULL,
    -- derived from transaction count from genesis
    total_transaction_blocks   BIGINT       NOT NULL,
    total_transactions         BIGINT       NOT NULL,
    network_total_transactions BIGINT       NOT NULL,
    -- number of milliseconds from the Unix epoch
    timestamp_ms               BIGINT       NOT NULL
);

CREATE INDEX checkpoints_epoch ON checkpoints (epoch);
CREATE INDEX checkpoints_timestamp ON checkpoints (timestamp_ms);