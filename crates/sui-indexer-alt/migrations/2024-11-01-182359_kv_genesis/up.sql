-- Stores information related to to the genesis checkpoint.
CREATE TABLE IF NOT EXISTS kv_genesis
(
    -- The checkpoint digest of the genesis checkpoint
    genesis_digest              BYTEA         PRIMARY KEY,
    -- The protocol version from the gensis system state
    initial_protocol_version    BIGINT        NOT NULL
);

-- Index to ensure there can only be one row in the genesis table.
CREATE UNIQUE INDEX IF NOT EXISTS kv_genesis_unique
ON kv_genesis ((0));
