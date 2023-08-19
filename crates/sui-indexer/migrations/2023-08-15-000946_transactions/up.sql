CREATE TABLE transactions (
    tx_sequence_number          BIGINT       PRIMARY KEY,
    transaction_digest          bytea        NOT NULL,
    raw_transaction             bytea        NOT NULL,
    raw_effects                 bytea        NOT NULL,
    checkpoint_sequence_number  BIGINT       NOT NULL,
    timestamp_ms                BIGINT       NOT NULL,
    object_changes              bytea[]      NOT NULL,
    balance_changes             bytea[]      NOT NULL,
    events                      bytea[]      NOT NULL,
    transaction_kind            smallint     NOT NULL
);

CREATE INDEX transactions_transaction_digest ON transactions USING HASH (transaction_digest);
CREATE INDEX transactions_checkpoint_sequence_number ON transactions (checkpoint_sequence_number);
-- only create index for system transactions
CREATE INDEX transactions_transaction_kind ON transactions USING HASH (transaction_kind) WHERE transaction_kind <> 3;
