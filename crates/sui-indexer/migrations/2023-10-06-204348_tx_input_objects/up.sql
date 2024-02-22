CREATE TABLE tx_input_objects (
    tx_sequence_number          BIGINT       NOT NULL,
    -- Object ID in bytes. 
    object_id                   BYTEA        NOT NULL,
    PRIMARY KEY(object_id, tx_sequence_number)
);
CREATE TABLE tx_input_objects_partition_0 PARTITION OF tx_input_objects FOR VALUES FROM (0) TO (MAXVALUE);
