CREATE TABLE IF NOT EXISTS tx_balance_changes
(
    tx_sequence_number          BIGINT        PRIMARY KEY,
    -- BCS serialized array of BalanceChanges
    balance_changes             BYTEA         NOT NULL
);
