CREATE TABLE IF NOT EXISTS cp_bloom_blocks
(
    cp_block_id BIGINT NOT NULL,
    bloom_block_index SMALLINT NOT NULL,
    bloom_filter BYTEA NOT NULL,
    PRIMARY KEY (cp_block_id, bloom_block_index)
);