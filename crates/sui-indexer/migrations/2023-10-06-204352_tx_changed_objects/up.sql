CREATE TABLE tx_changed_objects (
    tx_sequence_number          BIGINT       NOT NULL,
    -- Object Id in bytes.
    object_id                   BYTEA        NOT NULL,
    PRIMARY KEY(object_id, tx_sequence_number)
);
CREATE TABLE tx_changed_objects_partition_0 PARTITION OF tx_changed_objects FOR VALUES FROM (0) TO (MAXVALUE);
