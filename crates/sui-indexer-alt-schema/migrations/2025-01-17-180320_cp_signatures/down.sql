ALTER TABLE kv_checkpoints
DROP COLUMN IF EXISTS checkpoint_summary,
DROP COLUMN IF EXISTS validator_signatures,
ADD COLUMN IF NOT EXISTS certified_checkpoint BYTEA NOT NULL;
