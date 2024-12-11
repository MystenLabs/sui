CREATE TABLE tx_affected_addresses (
    tx_sequence_number          BIGINT       NOT NULL,
    affected                    BYTEA        NOT NULL,
    sender                      BYTEA        NOT NULL,
    PRIMARY KEY(affected, tx_sequence_number)
);

CREATE INDEX tx_affected_addresses_tx_sequence_number_index ON tx_affected_addresses (tx_sequence_number);
CREATE INDEX tx_affected_addresses_sender ON tx_affected_addresses (sender, affected, tx_sequence_number);
