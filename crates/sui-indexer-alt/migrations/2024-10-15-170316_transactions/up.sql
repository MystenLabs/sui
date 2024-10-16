CREATE TABLE IF NOT EXISTS kv_transactions
(
    tx_digest                   BYTEA         PRIMARY KEY,
    cp_sequence_number          BIGINT        NOT NULL,
    timestamp_ms                BIGINT        NOT NULL,
    -- BCS serialized TransactionData
    raw_transaction             BYTEA         NOT NULL,
    -- BCS serialized TransactionEffects
    raw_effects                 BYTEA         NOT NULL,
    -- BCS serialized array of Events
    events                      BYTEA         NOT NULL
);
