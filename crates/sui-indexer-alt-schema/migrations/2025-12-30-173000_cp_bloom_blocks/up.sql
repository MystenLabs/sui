CREATE TABLE IF NOT EXISTS cp_bloom_blocks
(
    cp_block_id BIGINT NOT NULL,
    bloom_block_index SMALLINT NOT NULL,
    cp_sequence_number_lo BIGINT NOT NULL,
    cp_sequence_number_hi BIGINT NOT NULL,
    bloom_filter BYTEA NOT NULL,
    num_items BIGINT DEFAULT NULL,
    PRIMARY KEY (cp_block_id, bloom_block_index)
);

CREATE INDEX IF NOT EXISTS idx_cp_bloom_blocks_by_bloom_block
ON cp_bloom_blocks (bloom_block_index, cp_block_id, cp_sequence_number_lo, cp_sequence_number_hi);

CREATE INDEX IF NOT EXISTS idx_cp_bloom_blocks_by_cp_range
ON cp_bloom_blocks (cp_sequence_number_lo, cp_sequence_number_hi);
