CREATE TABLE IF NOT EXISTS cp_bloom_items_wal
(
    cp_block_id BIGINT NOT NULL,
    cp_sequence_number BIGINT NOT NULL,
    items BYTEA[] NOT NULL,
    PRIMARY KEY (cp_block_id, cp_sequence_number)
);

CREATE INDEX IF NOT EXISTS idx_cp_bloom_items_wal_block
ON cp_bloom_items_wal (cp_block_id);
