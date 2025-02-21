DROP INDEX IF EXISTS tx_digests_tx_sequence_number;

CREATE INDEX IF NOT EXISTS tx_digests_tx_digest
ON tx_digests (tx_digest);
