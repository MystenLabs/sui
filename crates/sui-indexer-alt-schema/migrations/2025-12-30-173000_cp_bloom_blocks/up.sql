CREATE TABLE IF NOT EXISTS cp_bloom_blocks
(
    cp_block_index BIGINT NOT NULL,
    bloom_block_index SMALLINT NOT NULL,
    bloom_filter BYTEA NOT NULL,
    PRIMARY KEY (cp_block_index, bloom_block_index)
);