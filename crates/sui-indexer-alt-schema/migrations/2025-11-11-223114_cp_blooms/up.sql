CREATE TABLE IF NOT EXISTS cp_blooms
(
    cp_sequence_number BIGINT NOT NULL,
    bloom_filter BYTEA NOT NULL,
    num_items BIGINT DEFAULT NULL,
    PRIMARY KEY (cp_sequence_number)
);
