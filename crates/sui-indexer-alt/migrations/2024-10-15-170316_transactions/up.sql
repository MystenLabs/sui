CREATE TABLE IF NOT EXISTS kv_transactions
(
    tx_sequence_number          BIGINT        NOT NULL,
    cp_sequence_number          BIGINT        NOT NULL,
    timestamp_ms                BIGINT        NOT NULL,
    -- BCS serialized TransactionData
    raw_transaction             BYTEA         NOT NULL,
    -- BCS serialized TransactionEffects
    raw_effects                 BYTEA         NOT NULL,
    -- BCS serialized array of Events
    events                      BYTEA         NOT NULL,
    -- BCS serialized array of BalanceChanges
    balance_changes             BYTEA         NOT NULL,
    PRIMARY KEY (tx_sequence_number)
);
