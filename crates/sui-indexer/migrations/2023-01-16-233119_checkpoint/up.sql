CREATE TABLE checkpoints (
    sequence_number BIGINT PRIMARY KEY,
    checkpoint_digest VARCHAR(255) NOT NULL,
    epoch BIGINT NOT NULL,
    transactions TEXT[] NOT NULL,
    previous_checkpoint_digest VARCHAR(255),
    -- derived from EndOfEpochData
    next_epoch_committee TEXT,
    next_epoch_protocol_version BIGINT,
    -- derived from GasCostSummary
    total_gas_cost BIGINT NOT NULL,
    total_computation_cost BIGINT NOT NULL,
    total_storage_cost BIGINT NOT NULL,
    total_storage_rebate BIGINT NOT NULL,
    -- derived from transaction count from genesis
    total_transactions BIGINT NOT NULL,
    total_transactions_current_epoch BIGINT NOT NULL,
    total_transactions_from_genesis BIGINT NOT NULL,
    -- number of milliseconds from the Unix epoch
    timestamp_ms BIGINT NOT NULL,
    timestamp_ms_str TIMESTAMP NOT NULL,
    checkpoint_tps REAL NOT NULL
);

CREATE INDEX checkpoints_epoch ON checkpoints (epoch);
CREATE INDEX checkpoints_timestamp ON checkpoints (timestamp_ms_str);
CREATE INDEX checkpoints_checkpoint_digest ON checkpoints (checkpoint_digest);
