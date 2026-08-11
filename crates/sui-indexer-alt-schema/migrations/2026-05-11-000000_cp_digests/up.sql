-- Maps a checkpoint's sequence number to its digest, so checkpoints can be
-- looked up by digest. The reverse direction is served by the secondary
-- index on `cp_digest`. Mirrors the `tx_digests` table's shape.
--
-- New deployments populate this table from genesis; existing deployments
-- will only have entries for checkpoints indexed after this migration runs.
CREATE TABLE IF NOT EXISTS cp_digests
(
    cp_sequence_number BIGINT PRIMARY KEY,
    cp_digest          BYTEA  NOT NULL
);

CREATE INDEX IF NOT EXISTS cp_digests_cp_digest
    ON cp_digests (cp_digest);
