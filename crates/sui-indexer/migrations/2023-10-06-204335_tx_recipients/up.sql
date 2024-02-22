CREATE TABLE tx_recipients (
    tx_sequence_number          BIGINT       NOT NULL,
    -- SuiAddress in bytes.
    recipient                   BYTEA        NOT NULL,
    PRIMARY KEY(recipient, tx_sequence_number)
) PARTITION BY RANGE(tx_sequence_number);
CREATE TABLE tx_recipients_partition_0 PARTITION OF tx_recipients FOR VALUES FROM (0) TO (MAXVALUE);

CREATE INDEX tx_recipients_tx_sequence_number_index ON tx_recipients (tx_sequence_number ASC);
