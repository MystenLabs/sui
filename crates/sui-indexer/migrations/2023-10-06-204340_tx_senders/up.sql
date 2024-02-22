-- Your SQL goes here
CREATE TABLE tx_senders (
    tx_sequence_number          BIGINT       NOT NULL,
    -- SuiAddress in bytes.
    sender                      BYTEA        NOT NULL,
    PRIMARY KEY(sender, tx_sequence_number)
) PARTITION BY RANGE (tx_sequence_number);
CREATE TABLE tx_senders_partition_0 PARTITION OF tx_senders FOR VALUES FROM (0) TO (MAXVALUE);

CREATE INDEX tx_senders_tx_sequence_number_index ON tx_senders (tx_sequence_number ASC);
