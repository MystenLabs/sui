CREATE TABLE checkpoints
(
    sequence_number                     bigint       PRIMARY KEY,
    checkpoint_digest                   bytea        NOT NULL,
    epoch                               bigint       NOT NULL,
    network_total_transactions          bigint       NOT NULL,
    previous_checkpoint_digest          bytea,
    end_of_epoch                        boolean      NOT NULL,
    tx_digests                          bytea[]      NOT NULL,
    timestamp_ms                        BIGINT       NOT NULL,
    -- derived from GasCostSummary
    total_gas_cost                      BIGINT       NOT NULL,
    computation_cost                    BIGINT       NOT NULL,
    storage_cost                        BIGINT       NOT NULL,
    storage_rebate                      BIGINT       NOT NULL,
    non_refundable_storage_fee          BIGINT       NOT NULL,
    checkpoint_commitments              bytea        NOT NULL,
    validator_signature                 bytea        NOT NULL
);

CREATE INDEX checkpoints_epoch ON checkpoints (epoch);
CREATE INDEX checkpoints_digest ON checkpoints USING HASH (checkpoint_digest);