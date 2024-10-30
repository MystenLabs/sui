CREATE TABLE tx_affected_objects (
    tx_sequence_number          BIGINT       NOT NULL,
    affected                    BYTEA        NOT NULL,
    sender                      BYTEA        NOT NULL,
    PRIMARY KEY(affected, tx_sequence_number)
);

CREATE INDEX tx_affected_objects_tx_sequence_number_index ON tx_affected_objects (tx_sequence_number);
CREATE INDEX tx_affected_objects_sender ON tx_affected_objects (sender, affected, tx_sequence_number);
