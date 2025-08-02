CREATE TABLE transaction_digests (
    tx_digest TEXT PRIMARY KEY,
    checkpoint_sequence_number BIGINT NOT NULL
);
