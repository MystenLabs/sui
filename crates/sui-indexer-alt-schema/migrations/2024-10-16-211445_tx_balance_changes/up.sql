CREATE TABLE IF NOT EXISTS tx_balance_changes
(
    tx_sequence_number          BIGINT        NOT NULL,
    -- BCS serialized array of BalanceChanges
    balance_changes             BYTEA         NOT NULL,
    cp_sequence_number          BIGINT        NOT NULL,
    PRIMARY KEY (tx_sequence_number, cp_sequence_number)
) PARTITION BY RANGE (cp_sequence_number);

CREATE INDEX IF NOT EXISTS tx_balance_changes_cp_sequence_number
ON tx_balance_changes (cp_sequence_number);
