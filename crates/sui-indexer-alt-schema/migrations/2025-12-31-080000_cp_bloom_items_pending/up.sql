CREATE TABLE IF NOT EXISTS cp_bloom_items_pending
(
    cp_block_id BIGINT NOT NULL,
    cp_sequence_number BIGINT NOT NULL,
    items BYTEA[] NOT NULL,
    PRIMARY KEY (cp_block_id, cp_sequence_number)
) PARTITION BY LIST (cp_block_id);
