CREATE TABLE tx_calls (
    tx_sequence_number          BIGINT       NOT NULL,
    package                     BYTEA        NOT NULL,
    module                      TEXT         NOT NULL,
    func                        TEXT         NOT NULL,
    -- 1. Using Primary Key as a unique index.
    -- 2. Diesel does not like tables with no primary key.
    PRIMARY KEY(package, tx_sequence_number)
) PARTITION BY RANGE (tx_sequence_number);
CREATE TABLE tx_calls_partition_0 PARTITION OF tx_calls FOR VALUES FROM (0) TO (MAXVALUE);

CREATE INDEX tx_calls_module ON tx_calls (package, module, tx_sequence_number);
CREATE INDEX tx_calls_func ON tx_calls (package, module, func, tx_sequence_number);
CREATE INDEX tx_calls_tx_sequence_number ON tx_calls (tx_sequence_number);
