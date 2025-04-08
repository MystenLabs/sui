CREATE TABLE IF NOT EXISTS kv_checkpoints
(
    sequence_number                     BIGINT       PRIMARY KEY,
    certified_checkpoint                BYTEA        NOT NULL,
    checkpoint_contents                 BYTEA        NOT NULL
);
